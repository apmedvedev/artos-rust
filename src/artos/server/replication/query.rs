use crate::artos::common::{Error, ResponseHandler};
use crate::artos::message::client::{
    QueryRequest, QueryRequestItem, QueryResponse, QueryResponseItem, SubscriptionRequest, SubscriptionResponse,
    SubscriptionResponseItem,
};
use crate::artos::message::Response;
use crate::artos::server::channel::Context;
use crate::artos::server::replication::session::ClientSession;
use crate::artos::server::replication::{NullReplicationProtocol, ReplicationProtocol};
use crate::common::utils::Time;
use crate::compartment::Timer;
use crate::track;
use crate::track_method;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

include!("query_tests.rs");

//////////////////////////////////////////////////////////
// QueryProtocol API
//////////////////////////////////////////////////////////

#[cfg_attr(test, mockall::automock)]
pub trait QueryProtocol {
    fn start(&self) {
        unimplemented!()
    }

    fn stop(&self) {
        unimplemented!()
    }

    fn on_timer(&self, _current_time: Instant) {
        unimplemented!()
    }

    fn on_commit(&self, _session: &mut ClientSession) {
        unimplemented!()
    }

    fn on_follower(&self) {
        unimplemented!()
    }

    fn on_session_removed(&self, _session: &ClientSession) {
        unimplemented!()
    }

    fn handle_query_request(&self, _request: QueryRequest, _response_handler: Arc<dyn ResponseHandler>) {
        unimplemented!()
    }

    fn handle_subscription_request(&self, _request: SubscriptionRequest, _response_handler: Arc<dyn ResponseHandler>) {
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
    context: Rc<Context>,
    state: RefCell<State>,
}

struct State {
    replication_protocol: Rc<dyn ReplicationProtocol>,
    subscription_timeout_timer: Timer,
}

impl QueryProtocolImpl {
    fn new(context: Rc<Context>) -> QueryProtocolImpl {
        let subscription_timeout = context.server_channel_factory_configuration().subscription_timeout();
        QueryProtocolImpl {
            endpoint: context.local_server().endpoint().clone(),
            context,
            state: RefCell::new(State {
                replication_protocol: NullReplicationProtocol::new(),
                subscription_timeout_timer: Timer::new(false, subscription_timeout),
            }),
        }
    }

    pub fn set_replication_protocol(&self, value: Rc<dyn ReplicationProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.replication_protocol = value;
    }
}

impl QueryProtocol for QueryProtocolImpl {
    fn start(&self) {
        track_method!(t, "start");

        let state = &mut self.state.borrow_mut();
        state.subscription_timeout_timer.start();

        track!(t, "end");
    }

    fn stop(&self) {
        track_method!(t, "stop");

        let state = &mut self.state.borrow_mut();
        state.subscription_timeout_timer.stop();

        track!(t, "end");
    }

    fn on_timer(&self, current_time: Instant) {
        track_method!(t, "on_timer");

        let state = &mut self.state.borrow_mut();
        if state.subscription_timeout_timer.is_fired(current_time) {
            self.handle_subscription_timeout(state);
        }

        track!(t, "end");
    }

    fn on_commit(&self, session: &mut ClientSession) {
        track_method!(t, "on_commit");

        let last_committed_message_id = session.last_committed_message_id();
        let query_queue = session.query_queue();
        loop {
            if let Some((query, response_handler)) = query_queue.front() {
                let last_published_message_id = query.last_published_message_id();
                if last_published_message_id > last_committed_message_id {
                    break;
                }
                self.process_query(query, response_handler.clone());
            } else {
                break;
            }

            query_queue.pop_front();
        }

        track!(t, "end");
    }

    fn on_follower(&self) {
        track_method!(t, "on_follower");

        let state = &mut self.state.borrow_mut();

        let mut client_session_manager = state.replication_protocol.client_session_manager();
        for session in client_session_manager.sessions_mut() {
            session.query_queue().clear();
        }

        track!(t, "end");
    }

    fn on_session_removed(&self, session: &ClientSession) {
        track_method!(t, "on_session_removed");

        if !session.subscriptions().is_empty() {
            self.process_subscriptions(None, session.subscriptions(), None);
        }

        track!(t, "end");
    }

    fn handle_query_request(&self, request: QueryRequest, response_handler: Arc<dyn ResponseHandler>) {
        track_method!(t, "handle_query_request");

        let state = &mut self.state.borrow_mut();

        let mut client_session_manager = state.replication_protocol.client_session_manager();
        let session = client_session_manager.ensure_session(request.client_id());

        if !request.queries().is_empty() {
            for query in request.queries() {
                let last_published_message_id = query.last_published_message_id();
                if last_published_message_id > session.last_committed_message_id() {
                    track!(t, "early_query");
                    session.add_query((query.clone(), response_handler.clone()));
                } else {
                    self.process_query(query, response_handler.clone());
                }
            }
        } else {
            track!(t, "empty_query");
            let response = Response::Query(QueryResponse::new(self.context.local_server().id(), true, vec![]));

            let serialized_response = bincode::serialize(&response).unwrap();
            response_handler.on_succeeded(serialized_response);
        }

        track!(t, "end");
    }

    fn handle_subscription_request(&self, request: SubscriptionRequest, response_handler: Arc<dyn ResponseHandler>) {
        track_method!(t, "handle_subscription_request");

        let state = &mut self.state.borrow_mut();

        let mut client_session_manager = state.replication_protocol.client_session_manager();
        let session = client_session_manager.ensure_session(request.client_id());

        if !request.subscriptions().is_empty() {
            let subscription_ids: Vec<Uuid> = request.subscriptions().iter().map(|v| v.subscription_id()).collect();

            let mut removed_subscription_ids = vec![];
            for subscription in session.subscriptions() {
                if !subscription_ids.contains(subscription) {
                    removed_subscription_ids.push(*subscription);
                }
            }

            session.set_subscriptions(subscription_ids);
            self.process_subscriptions(Some(request), &removed_subscription_ids, Some(response_handler));
        }

        track!(t, "end");
    }
}

impl QueryProtocolImpl {
    fn process_query(&self, query: &QueryRequestItem, response_handler: Arc<dyn ResponseHandler>) {
        track_method!(t, "process_query");

        let transaction = self.context.state_machine().begin_transaction(true);
        transaction.query(
            query.request(),
            QueryResponseHandler::new(
                self.context.local_server().id(),
                query.query_id(),
                response_handler.clone(),
            ),
        );
        transaction.commit();

        track!(t, "end");
    }

    fn process_subscriptions(
        &self,
        request: Option<SubscriptionRequest>,
        removed_subscription_ids: &Vec<Uuid>,
        response_handler: Option<Arc<dyn ResponseHandler>>,
    ) {
        track_method!(t, "process_subscriptions");

        let transaction = self.context.state_machine().begin_transaction(true);
        for subscription_id in removed_subscription_ids {
            transaction.unsubscribe(*subscription_id);
        }

        if let Some(request) = request {
            let response_handler = response_handler.unwrap();
            for subscription in request.subscriptions() {
                transaction.subscribe(
                    subscription.subscription_id(),
                    subscription.request(),
                    SubscriptionResponseHandler::new(
                        self.context.local_server().id(),
                        subscription.subscription_id(),
                        response_handler.clone(),
                    ),
                );
            }
        }

        transaction.commit();

        track!(t, "end");
    }

    fn handle_subscription_timeout(&self, state: &mut State) {
        track_method!(t, "handle_subscription_timeout");

        let current_time = Time::now();

        let mut client_session_manager = state.replication_protocol.client_session_manager();
        for session in client_session_manager.sessions_mut() {
            if current_time
                >= session.last_update_subscription_time()
                    + self
                        .context
                        .server_channel_factory_configuration()
                        .subscription_timeout()
            {
                track!(t, "timed_out");
                let transaction = self.context.state_machine().begin_transaction(true);
                for subscription_id in session.subscriptions() {
                    transaction.unsubscribe(*subscription_id);
                }

                transaction.commit();

                session.set_subscriptions(vec![]);
            }
        }

        track!(t, "end");
    }
}

//////////////////////////////////////////////////////////
// QueryResponseHandler
//////////////////////////////////////////////////////////

struct QueryResponseHandler {
    server_id: Uuid,
    query_id: u64,
    delegate: Arc<dyn ResponseHandler>,
}

impl QueryResponseHandler {
    fn new(server_id: Uuid, query_id: u64, delegate: Arc<dyn ResponseHandler>) -> Box<QueryResponseHandler> {
        Box::new(QueryResponseHandler {
            server_id,
            query_id,
            delegate,
        })
    }
}

impl ResponseHandler for QueryResponseHandler {
    fn on_succeeded(&self, result: Vec<u8>) {
        let item = QueryResponseItem::new(self.query_id, result);
        let response = Response::Query(QueryResponse::new(self.server_id, true, vec![item]));

        let serialized_response = bincode::serialize(&response).unwrap();
        self.delegate.on_succeeded(serialized_response);
    }

    fn on_failed(&self, error: Error) {
        self.delegate.on_failed(error);
    }
}

//////////////////////////////////////////////////////////
// SubscriptionResponseHandler
//////////////////////////////////////////////////////////

struct SubscriptionResponseHandler {
    server_id: Uuid,
    subscription_id: Uuid,
    delegate: Arc<dyn ResponseHandler>,
}

impl SubscriptionResponseHandler {
    fn new(
        server_id: Uuid,
        subscription_id: Uuid,
        delegate: Arc<dyn ResponseHandler>,
    ) -> Box<SubscriptionResponseHandler> {
        Box::new(SubscriptionResponseHandler {
            server_id,
            subscription_id,
            delegate,
        })
    }
}

impl ResponseHandler for SubscriptionResponseHandler {
    fn on_succeeded(&self, result: Vec<u8>) {
        let item = SubscriptionResponseItem::new(self.subscription_id, result);
        let response = Response::Subscribe(SubscriptionResponse::new(self.server_id, true, vec![item]));

        let serialized_response = bincode::serialize(&response).unwrap();
        self.delegate.on_succeeded(serialized_response);
    }

    fn on_failed(&self, error: Error) {
        self.delegate.on_failed(error);
    }
}
