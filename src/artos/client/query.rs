use crate::artos::client::api::CompletionHandler;
use crate::artos::client::channel::{ClientResponseHandler, Event, EventType};
use crate::artos::client::publish::{NullPublishProtocol, PublishProtocol};
use crate::artos::common::api::{GroupConfiguration, MessageSender, MessageSenderFactory, ServerConfiguration};
use crate::artos::common::utils::VecExt;
use crate::artos::common::Error;
use crate::artos::common::{FlowController, FlowType};
use crate::artos::message::client::{
    QueryRequest, QueryRequestItem, QueryResponse, SubscriptionRequest, SubscriptionRequestItem, SubscriptionResponse,
};
use crate::artos::message::{Request, Response};
use crate::common::utils::Time;
use crate::compartment::{Sender, Timer};
use log::debug;
use log::info;
use log::trace;
use log::warn;
use rand::Rng;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};
use uuid::Uuid;

//////////////////////////////////////////////////////////
// QueryProtocol API
//////////////////////////////////////////////////////////

pub trait QueryProtocol {
    fn update_configuration(&self, _configuration: GroupConfiguration) {
        unimplemented!()
    }

    fn on_leader_connected(&self, _message_sender: Rc<dyn MessageSender>, _server_configuration: ServerConfiguration) {
        unimplemented!()
    }

    fn on_leader_disconnected(&self) {
        unimplemented!()
    }
}

pub struct NullQueryProtocol {}

impl NullQueryProtocol {
    pub fn new() -> Rc<dyn QueryProtocol> {
        Rc::new(NullQueryProtocol {})
    }
}

impl QueryProtocol for NullQueryProtocol {}

//////////////////////////////////////////////////////////
// QueryProtocolImpl
//////////////////////////////////////////////////////////

pub struct QueryProtocolImpl {
    endpoint: String,
    client_id: Uuid,
    group_id: Uuid,
    message_sender_factory: Box<dyn MessageSenderFactory>,
    lock_queue_capacity: usize,
    unlock_queue_capacity: usize,
    max_batch_size: usize,
    resend_query_period: Duration,
    sender: Sender<Event>,
    handler: Rc<dyn CompletionHandler>,
    flow_controller: Rc<dyn FlowController>,
    state: RefCell<State>,
}

struct State {
    server_timeout_timer: Timer,
    heartbeat_timer: Timer,
    resubscription_timer: Timer,
    pending_queries: VecDeque<QueryInfo>,
    sent_queries: Vec<QueryInfo>,
    subscriptions: Vec<SubscriptionInfo>,
    next_message_id: u64,
    flow_locked: bool,
    configuration: Option<GroupConfiguration>,
    server_configuration: Option<ServerConfiguration>,
    message_sender: Option<Rc<dyn MessageSender>>,
    resubscription_required: bool,
    publish_protocol: Rc<dyn PublishProtocol>,
}

struct QueryInfo {
    query: QueryRequestItem,
    query_id: u64,
    user_data: usize,
    last_sent_time: Instant,
}

struct SubscriptionInfo {
    subscription: SubscriptionRequestItem,
    subscription_id: Uuid,
    user_data: usize,
}

impl QueryProtocolImpl {
    pub fn new(
        endpoint: String,
        client_id: Uuid,
        group_id: Uuid,
        message_sender_factory: Box<dyn MessageSenderFactory>,
        lock_queue_capacity: usize,
        unlock_queue_capacity: usize,
        max_batch_size: usize,
        resend_query_period: Duration,
        server_timeout: Duration,
        heartbeat_period: Duration,
        resubscription_period: Duration,
        sender: Sender<Event>,
        handler: Rc<dyn CompletionHandler>,
        flow_controller: Rc<dyn FlowController>,
    ) -> QueryProtocolImpl {
        QueryProtocolImpl {
            endpoint,
            client_id,
            group_id,
            message_sender_factory,
            lock_queue_capacity,
            unlock_queue_capacity,
            max_batch_size,
            resend_query_period,
            sender,
            handler,
            flow_controller,
            state: RefCell::new(State {
                server_timeout_timer: Timer::new(false, server_timeout),
                heartbeat_timer: Timer::new(false, heartbeat_period),
                resubscription_timer: Timer::new(false, resubscription_period),
                pending_queries: VecDeque::new(),
                sent_queries: Vec::new(),
                subscriptions: Vec::new(),
                next_message_id: 1,
                flow_locked: false,
                configuration: None,
                server_configuration: None,
                message_sender: None,
                resubscription_required: false,
                publish_protocol: NullPublishProtocol::new(),
            }),
        }
    }

    pub fn set_publish_protocol(&self, value: Rc<dyn PublishProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.publish_protocol = value;
    }

    pub fn start(&self) {
        let state = &mut self.state.borrow_mut();
        if !state.configuration.as_ref().unwrap().is_single_server() {
            state.heartbeat_timer.start();
        }

        state.resubscription_timer.start();
    }

    pub fn stop(&self) {
        let state = &mut self.state.borrow_mut();

        self.on_server_disconnected(false, state);

        state.heartbeat_timer.stop();
        state.resubscription_timer.stop();
    }

    pub fn process(&self) {
        let state = &mut self.state.borrow_mut();
        if state.message_sender.is_some() {
            if let Some(request) = self.create_request(state) {
                self.send_request(request, state);
            }
        }
    }

    pub fn on_timer(&self, current_time: Instant) {
        let state = &mut self.state.borrow_mut();

        if state.server_timeout_timer.is_fired(current_time) {
            self.handle_server_timeout(state);
        }

        if state.heartbeat_timer.is_fired(current_time) {
            self.request_heartbeat(state);
        }

        if state.resubscription_timer.is_fired(current_time) {
            self.resubscribe(state);
        }

        if state.message_sender.is_some() {
            if state.resubscription_required {
                self.resubscribe(state);
                state.resubscription_required = false;
            }
        }
    }

    pub fn query(&self, request: Vec<u8>, user_data: usize) {
        let state = &mut self.state.borrow_mut();

        let last_published_message_id = state.publish_protocol.next_message_id() - 1;

        let query_id = state.next_message_id;
        state.next_message_id += 1;
        state.pending_queries.push_back(QueryInfo {
            query: QueryRequestItem::new(query_id, last_published_message_id, request),
            query_id,
            user_data,
            last_sent_time: Time::now(),
        });

        self.check_flow(state);
    }

    pub fn subscribe(&self, subscription_id: Uuid, request: Vec<u8>, user_data: usize) {
        let state = &mut self.state.borrow_mut();

        state.subscriptions.update_or_insert(
            |v| v.subscription_id == subscription_id,
            SubscriptionInfo {
                subscription: SubscriptionRequestItem::new(subscription_id, request),
                subscription_id,
                user_data,
            },
        );

        state.resubscription_required = true;
    }

    pub fn unsubscribe(&self, subscription_id: Uuid) {
        let state = &mut self.state.borrow_mut();
        if state
            .subscriptions
            .remove_exists(|v| v.subscription_id == subscription_id)
        {
            state.resubscription_required = true;
        }
    }

    pub fn handle_succeeded_response(&self, server_id: Uuid, serialized_response: Vec<u8>) {
        let state = &mut self.state.borrow_mut();
        if let Result::Ok(response) = bincode::deserialize::<Response>(&serialized_response) {
            match response {
                Response::Query(response) => self.handle_query_response(server_id, response, state),
                Response::Subscribe(response) => self.handle_subscription_response(server_id, response, state),
                _ => {}
            }
        }
    }

    pub fn handle_failed_response(&self, server_configuration: ServerConfiguration, error: Error) {
        let state = &mut self.state.borrow_mut();

        warn!(
            "[{}] Response error received from server {}: {:?}.",
            self.endpoint,
            server_configuration.endpoint(),
            error
        );

        self.on_server_disconnected(true, state);
    }
}

impl QueryProtocol for QueryProtocolImpl {
    fn update_configuration(&self, configuration: GroupConfiguration) {
        let state = &mut self.state.borrow_mut();
        if state.configuration.is_none() || state.configuration.as_ref().unwrap() != &configuration {
            state.configuration = Some(configuration.clone());

            if state.server_configuration.is_some()
                && configuration
                    .servers()
                    .iter()
                    .find(|v| v.id() == state.server_configuration.as_ref().unwrap().id())
                    .is_none()
            {
                self.on_server_disconnected(false, state);
            }
        }
    }

    fn on_leader_connected(&self, message_sender: Rc<dyn MessageSender>, server_configuration: ServerConfiguration) {
        let state = &mut self.state.borrow_mut();
        if state.configuration.is_some() && state.configuration.as_ref().unwrap().is_single_server() {
            state.message_sender = Some(message_sender);
            state.server_configuration = Some(server_configuration);
        }
    }

    fn on_leader_disconnected(&self) {
        let state = &mut self.state.borrow_mut();
        self.on_server_disconnected(false, state);
    }
}

impl QueryProtocolImpl {
    fn create_request(&self, state: &mut State) -> Option<Request> {
        let current_time = Time::now();

        let mut queries = Vec::new();
        let mut batch_size = 0;
        loop {
            if let Some(info) = state.pending_queries.front() {
                if info.query.request().len() + batch_size > self.max_batch_size {
                    break;
                }
            } else {
                break;
            }

            let mut info = state.pending_queries.pop_front().unwrap();
            queries.push(info.query.clone());
            batch_size += info.query.request().len();

            info.last_sent_time = current_time;
            state.sent_queries.push(info);
        }

        for mut info in &mut state.sent_queries {
            if current_time < info.last_sent_time + self.resend_query_period {
                continue;
            }
            if info.query.request().len() + batch_size > self.max_batch_size {
                break;
            }

            queries.push(info.query.clone());
            batch_size += info.query.request().len();
            info.last_sent_time = current_time;
        }

        if !queries.is_empty() {
            Some(Request::Query(QueryRequest::new(
                self.client_id,
                self.group_id,
                queries,
            )))
        } else {
            None
        }
    }

    fn send_request(&self, request: Request, state: &mut State) {
        self.ensure_message_sender(state);
        if state.message_sender.is_some() {
            trace!("[{}] Request sent: {:?}.", self.endpoint, request);

            if !state.configuration.as_ref().unwrap().is_single_server() {
                state.heartbeat_timer.restart();
            }

            let serialized_request = bincode::serialize(&request).unwrap();
            state.message_sender.as_ref().unwrap().send(serialized_request);
        }
    }

    fn handle_query_response(&self, server_id: Uuid, response: QueryResponse, state: &mut State) {
        if response.server_id() != server_id {
            return;
        }

        trace!("[{}] Response received: {:?}.", self.endpoint, response);

        if response.is_accepted() {
            for result in response.results() {
                if let Some((index, info)) = state
                    .sent_queries
                    .iter()
                    .enumerate()
                    .find(|(_, v)| v.query_id == result.query_id())
                {
                    self.handler.on_query(result.value(), info.user_data);
                    state.sent_queries.remove(index);
                }
            }

            if !state.configuration.as_ref().unwrap().is_single_server() {
                state.server_timeout_timer.restart();
            }

            self.check_flow(state);
        } else {
            self.on_server_disconnected(true, state);
        }
    }

    fn handle_subscription_response(&self, server_id: Uuid, response: SubscriptionResponse, state: &mut State) {
        if response.server_id() != server_id {
            return;
        }

        trace!("[{}] Response received: {:?}.", self.endpoint, response);

        if response.is_accepted() {
            for result in response.results() {
                if let Some(info) = state
                    .subscriptions
                    .iter()
                    .find(|v| v.subscription_id == result.subscription_id())
                {
                    self.handler.on_subscription_event(result.value(), info.user_data);
                }
            }

            if !state.configuration.as_ref().unwrap().is_single_server() {
                state.server_timeout_timer.restart();
            }
        } else {
            self.on_server_disconnected(true, state);
        }
    }

    fn request_heartbeat(&self, state: &mut State) {
        self.send_request(
            Request::Query(QueryRequest::new(self.client_id, self.group_id, vec![])),
            state,
        );
    }

    fn resubscribe(&self, state: &mut State) {
        self.send_request(
            Request::Subscribe(SubscriptionRequest::new(
                self.client_id,
                self.group_id,
                state.subscriptions.iter().map(|v| v.subscription.clone()).collect(),
            )),
            state,
        );
    }

    fn ensure_message_sender(&self, state: &mut State) {
        if !state.publish_protocol.is_connected() {
            return;
        }

        if state.message_sender.is_none() {
            assert!(!state.configuration.as_ref().unwrap().is_single_server());

            state.server_configuration = Some(self.server_configuration(state));

            match self.message_sender_factory.create_sender(
                state.server_configuration.as_ref().unwrap().endpoint().clone(),
                Box::new(ClientResponseHandler::new(
                    EventType::Query,
                    self.sender.clone(),
                    state.server_configuration.as_ref().unwrap().clone(),
                )),
            ) {
                Ok(message_sender) => {
                    state.message_sender = Some(Rc::from(message_sender));
                    self.on_server_connected(state);
                }

                Err(error) => {
                    warn!("[{}] Could not create message sender: {:?}.", self.endpoint, error);

                    self.on_server_disconnected(false, state);
                }
            }
        }
    }

    fn check_flow(&self, state: &mut State) {
        if !state.flow_locked && state.pending_queries.len() + state.sent_queries.len() >= self.lock_queue_capacity {
            state.flow_locked = true;
            self.flow_controller.lock_flow(FlowType::Query);
        }

        if state.flow_locked && state.pending_queries.len() + state.sent_queries.len() <= self.unlock_queue_capacity {
            state.flow_locked = false;
            self.flow_controller.unlock_flow(FlowType::Query);
        }
    }

    fn on_server_connected(&self, state: &mut State) {
        state.server_timeout_timer.start();

        info!(
            "[{}] Server is connected: {:?}.",
            self.endpoint,
            state.server_configuration.as_ref().unwrap()
        );
    }

    fn on_server_disconnected(&self, notify_publish_protocol: bool, state: &mut State) {
        if state.configuration.as_ref().unwrap().is_single_server() {
            state.message_sender = None;

            if notify_publish_protocol {
                state.publish_protocol.on_disconnected();
            }
        }

        if state.server_configuration.is_some() {
            info!(
                "[{}] Server is disconnected: {:?}.",
                self.endpoint,
                state.server_configuration.as_ref().unwrap()
            );

            state.server_configuration = None;
            state.message_sender = None;
            state.server_timeout_timer.stop();

            for info in &mut state.sent_queries {
                info.last_sent_time -= self.resend_query_period;
            }

            state.resubscription_required = true;
        }
    }

    fn handle_server_timeout(&self, state: &mut State) {
        warn!(
            "[{}] Timeout has occurred, when waiting response from server: {:?}.",
            self.endpoint,
            state.server_configuration.as_ref().unwrap()
        );

        self.on_server_disconnected(false, state);
    }

    fn server_configuration(&self, state: &mut State) -> ServerConfiguration {
        let group_configuration = state.publish_protocol.group_configuration();
        let mut random = rand::thread_rng();
        let index = random.gen_range(0, group_configuration.servers().len());
        let server_configuration = group_configuration.servers()[index].clone();

        debug!(
            "[{}] Server has been selected as query group endpoint: {:?}.",
            self.endpoint, server_configuration
        );

        server_configuration
    }
}
