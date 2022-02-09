use crate::artos::client::api::{ClientChannelConfiguration, ClientChannelFactoryConfiguration, CompletionHandler};
use crate::artos::client::flow::blocking::{BlockingFlowController, FlowWaiter};
use crate::artos::client::publish::{NullPublishProtocol, PublishProtocolImpl};
use crate::artos::client::query::{NullQueryProtocol, QueryProtocolImpl};
use crate::artos::common::api::{ResponseHandler, ServerConfiguration};
use crate::artos::common::Error;
use crate::artos::common::{FlowController, FlowType};
use crate::compartment::{Compartment, Delayed, Handler, HandlerFactory, Sender};
use std::rc::Rc;
use std::time::Instant;
use uuid::Uuid;

//////////////////////////////////////////////////////////
// Client API
//////////////////////////////////////////////////////////

pub struct ClientChannel {
    compartment: Compartment<Event>,
    flow_waiter: FlowWaiter,
    max_value_size: usize,
}

impl ClientChannel {
    pub fn new(
        configuration: ClientChannelConfiguration,
        completion_handler: Box<dyn CompletionHandler>,
    ) -> ClientChannel {
        Self::with_config(Default::default(), configuration, completion_handler)
    }

    pub fn with_config(
        factory_configuration: ClientChannelFactoryConfiguration,
        configuration: ClientChannelConfiguration,
        completion_handler: Box<dyn CompletionHandler>,
    ) -> ClientChannel {
        let (flow_waiter, flow_controller) = BlockingFlowController::new();
        let max_value_size = factory_configuration.max_value_size();
        ClientChannel {
            compartment: Compartment::new(ClientImplFactory::new(
                factory_configuration,
                configuration,
                flow_controller,
                completion_handler,
            )),
            flow_waiter,
            max_value_size,
        }
    }

    pub fn publish(&self, value: Vec<u8>, user_data: usize) -> bool {
        if value.len() > self.max_value_size {
            return false;
        }

        self.flow_waiter.wait(FlowType::Publish);

        self.compartment.send(Event::PublishRequest(value, user_data));

        true
    }

    pub fn query(&self, request: Vec<u8>, user_data: usize) -> bool {
        if request.len() > self.max_value_size {
            return false;
        }

        self.flow_waiter.wait(FlowType::Query);

        self.compartment.send(Event::QueryRequest(request, user_data));

        true
    }

    pub fn subscribe(&self, subscription_id: Uuid, request: Vec<u8>, user_data: usize) -> bool {
        if request.len() > self.max_value_size {
            return false;
        }

        self.compartment
            .send(Event::SubscriptionRequest(subscription_id, request, user_data));

        true
    }

    pub fn unsubscribe(&self, subscription_id: Uuid) {
        self.compartment.send(Event::UnsubscriptionRequest(subscription_id));
    }
}

impl Clone for ClientChannel {
    fn clone(&self) -> Self {
        ClientChannel {
            compartment: self.compartment.clone(),
            flow_waiter: self.flow_waiter.clone(),
            max_value_size: self.max_value_size,
        }
    }
}

//////////////////////////////////////////////////////////
// Client implementation
//////////////////////////////////////////////////////////

#[derive(Clone)]
pub enum EventType {
    Publish,
    Query,
}

pub enum Event {
    PublishRequest(Vec<u8>, usize),
    PublishResponse(Vec<u8>),
    PublishError(ServerConfiguration, Error),
    QueryRequest(Vec<u8>, usize),
    QueryResponse(Uuid, Vec<u8>),
    QueryError(ServerConfiguration, Error),
    SubscriptionRequest(Uuid, Vec<u8>, usize),
    UnsubscriptionRequest(Uuid),
}

pub struct ClientResponseHandler {
    event_type: EventType,
    sender: Sender<Event>,
    configuration: ServerConfiguration,
}

impl ClientResponseHandler {
    pub fn new(
        event_type: EventType,
        sender: Sender<Event>,
        configuration: ServerConfiguration,
    ) -> ClientResponseHandler {
        ClientResponseHandler {
            event_type,
            sender,
            configuration,
        }
    }
}

impl ResponseHandler for ClientResponseHandler {
    fn on_succeeded(&self, response: Vec<u8>) {
        match self.event_type {
            EventType::Publish => self.sender.send(Event::PublishResponse(response)),
            EventType::Query => self
                .sender
                .send(Event::QueryResponse(self.configuration.id(), response)),
        }
    }

    fn on_failed(&self, error: Error) {
        match self.event_type {
            EventType::Publish => self.sender.send(Event::PublishError(self.configuration.clone(), error)),
            EventType::Query => self.sender.send(Event::QueryError(self.configuration.clone(), error)),
        }
    }
}

//////////////////////////////////////////////////////////
// ClientImpl
//////////////////////////////////////////////////////////

struct ClientImpl {
    publish_protocol: Rc<PublishProtocolImpl>,
    query_protocol: Rc<QueryProtocolImpl>,
}

impl Handler<Event> for ClientImpl {
    fn process(&self) {
        self.publish_protocol.process();
        self.query_protocol.process();
    }

    fn on_timer(&self, current_time: Instant) {
        self.publish_protocol.on_timer(current_time);
        self.query_protocol.on_timer(current_time);
    }

    fn handle(&self, event: Event) {
        match event {
            Event::PublishRequest(request, user_data) => {
                self.publish_protocol.publish(request, user_data);
            }
            Event::PublishResponse(response) => {
                self.publish_protocol.handle_succeeded_response(response);
            }
            Event::PublishError(configuration, error) => {
                self.publish_protocol.handle_failed_response(configuration, error);
            }
            Event::QueryRequest(request, user_data) => {
                self.query_protocol.query(request, user_data);
            }
            Event::QueryResponse(server_id, response) => {
                self.query_protocol.handle_succeeded_response(server_id, response)
            }
            Event::QueryError(configuration, error) => self.query_protocol.handle_failed_response(configuration, error),
            Event::SubscriptionRequest(subscription_id, request, user_data) => {
                self.query_protocol.subscribe(subscription_id, request, user_data);
            }
            Event::UnsubscriptionRequest(subscription_id) => {
                self.query_protocol.unsubscribe(subscription_id);
            }
        }
    }
}

impl Drop for ClientImpl {
    fn drop(&mut self) {
        self.stop();
    }
}

impl ClientImpl {
    fn start(&mut self) {
        self.publish_protocol.start();
        self.query_protocol.start();
    }

    fn stop(&mut self) {
        self.publish_protocol.stop();
        self.query_protocol.stop();

        self.publish_protocol.set_query_protocol(NullQueryProtocol::new());

        self.query_protocol.set_publish_protocol(NullPublishProtocol::new());
    }
}

//////////////////////////////////////////////////////////
// ClientImplFactory
//////////////////////////////////////////////////////////

struct ClientImplFactory {
    factory_configuration: ClientChannelFactoryConfiguration,
    configuration: ClientChannelConfiguration,
    flow_controller: Box<dyn FlowController>,
    completion_handler: Box<dyn CompletionHandler>,
}

impl ClientImplFactory {
    pub fn new(
        factory_configuration: ClientChannelFactoryConfiguration,
        configuration: ClientChannelConfiguration,
        flow_controller: Box<dyn FlowController>,
        completion_handler: Box<dyn CompletionHandler>,
    ) -> ClientImplFactory {
        ClientImplFactory {
            factory_configuration,
            configuration,
            flow_controller,
            completion_handler,
        }
    }
}

impl HandlerFactory<ClientImpl, Event> for ClientImplFactory {
    fn build(self, sender: Sender<Event>, _delayed: Delayed<Event>) -> ClientImpl {
        let client_id = Uuid::new_v4();
        let flow_controller: Rc<dyn FlowController> = Rc::from(self.flow_controller);
        let completion_handler: Rc<dyn CompletionHandler> = Rc::from(self.completion_handler);

        let publish_protocol = Rc::new(PublishProtocolImpl::new(
            self.configuration.endpoint().clone(),
            client_id,
            self.configuration.group().id(),
            self.configuration.message_sender().build(),
            self.configuration.client_store().build(),
            self.configuration.group().clone(),
            sender.clone(),
            self.factory_configuration.heartbeat_period(),
            self.factory_configuration.update_configuration_period(),
            self.factory_configuration.server_timeout(),
            self.factory_configuration.lock_queue_capacity(),
            self.factory_configuration.unlock_queue_capacity(),
            self.factory_configuration.max_batch_size(),
            flow_controller.clone(),
            completion_handler.clone(),
        ));
        let query_protocol = Rc::new(QueryProtocolImpl::new(
            self.configuration.endpoint().clone(),
            client_id,
            self.configuration.group().id(),
            self.configuration.message_sender().build(),
            self.factory_configuration.lock_queue_capacity(),
            self.factory_configuration.unlock_queue_capacity(),
            self.factory_configuration.max_batch_size(),
            self.factory_configuration.resend_query_period(),
            self.factory_configuration.server_timeout(),
            self.factory_configuration.heartbeat_period(),
            self.factory_configuration.resubscription_period(),
            sender.clone(),
            completion_handler.clone(),
            flow_controller.clone(),
        ));

        publish_protocol.set_query_protocol(query_protocol.clone());
        query_protocol.set_publish_protocol(publish_protocol.clone());

        let mut client = ClientImpl {
            publish_protocol,
            query_protocol,
        };

        client.start();

        client
    }
}
