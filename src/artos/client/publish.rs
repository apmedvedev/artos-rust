use crate::artos::client::channel::{ClientResponseHandler, Event, EventType};
use crate::artos::client::query::{NullQueryProtocol, QueryProtocol};
use crate::artos::client::{ClientStore, CompletionHandler};
use crate::artos::common::Error;
use crate::artos::common::{FlowController, FlowType};
use crate::artos::common::{GroupConfiguration, MessageSender, MessageSenderFactory, ServerConfiguration};
use crate::artos::message::client::{
    ConfigurationRequest, LogEntry, PublishRequest, PublishResponse, RestartSessionRequest,
};
use crate::compartment::{Sender, Timer};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};
use uuid::Uuid;

use log::debug;
use log::info;
use log::trace;
use log::warn;

use crate::artos::message::{Request, Response};
use rand::Rng;

//////////////////////////////////////////////////////////
// PublishProtocol API
//////////////////////////////////////////////////////////

pub trait PublishProtocol {
    fn is_connected(&self) -> bool {
        unimplemented!()
    }

    fn group_configuration(&self) -> GroupConfiguration {
        unimplemented!()
    }

    fn next_message_id(&self) -> u64 {
        unimplemented!()
    }

    fn on_disconnected(&self) {
        unimplemented!()
    }
}

pub struct NullPublishProtocol {}

impl NullPublishProtocol {
    pub fn new() -> Rc<dyn PublishProtocol> {
        Rc::new(NullPublishProtocol {})
    }
}

impl PublishProtocol for NullPublishProtocol {}

//////////////////////////////////////////////////////////
// PublishProtocolImpl
//////////////////////////////////////////////////////////

pub struct PublishProtocolImpl {
    endpoint: String,
    client_id: Uuid,
    group_id: Uuid,
    lock_queue_capacity: usize,
    unlock_queue_capacity: usize,
    max_batch_size: usize,
    flow_controller: Rc<dyn FlowController>,
    handler: Rc<dyn CompletionHandler>,
    message_sender_factory: Box<dyn MessageSenderFactory>,
    initial_configuration: GroupConfiguration,
    sender: Sender<Event>,
    state: RefCell<State>,
}

struct State {
    client_store: Box<dyn ClientStore>,
    leader_timeout_timer: Timer,
    heartbeat_timer: Timer,
    update_configuration_timer: Timer,
    leader_id: Option<Uuid>,
    leader_endpoint: Option<String>,
    message_sender: Option<Rc<dyn MessageSender>>,
    configuration: GroupConfiguration,
    in_progress: bool,
    query_protocol: Rc<dyn QueryProtocol>,
    queue: VecDeque<EntryInfo>,
    next_message_id: u64,
    last_sent_message_id: u64,
    last_committed_message_id: u64,
    flow_locked: bool,
}

struct EntryInfo {
    log_entry: LogEntry,
    user_data: usize,
}

impl PublishProtocolImpl {
    pub fn new(
        endpoint: String,
        client_id: Uuid,
        group_id: Uuid,
        message_sender_factory: Box<dyn MessageSenderFactory>,
        client_store: Box<dyn ClientStore>,
        initial_configuration: GroupConfiguration,
        sender: Sender<Event>,
        acknowledge_request_period: Duration,
        update_configuration_period: Duration,
        leader_timeout: Duration,
        lock_queue_capacity: usize,
        unlock_queue_capacity: usize,
        max_batch_size: usize,
        flow_controller: Rc<dyn FlowController>,
        handler: Rc<dyn CompletionHandler>,
    ) -> PublishProtocolImpl {
        PublishProtocolImpl {
            endpoint,
            client_id,
            group_id,
            lock_queue_capacity,
            unlock_queue_capacity,
            max_batch_size,
            flow_controller,
            handler,
            message_sender_factory,
            initial_configuration: initial_configuration.clone(),
            sender,
            state: RefCell::new(State {
                client_store,
                leader_timeout_timer: Timer::new(false, leader_timeout),
                heartbeat_timer: Timer::new(false, acknowledge_request_period),
                update_configuration_timer: Timer::new(false, update_configuration_period),
                leader_id: None,
                leader_endpoint: None,
                message_sender: None,
                configuration: initial_configuration,
                in_progress: false,
                query_protocol: NullQueryProtocol::new(),
                queue: VecDeque::new(),
                next_message_id: 1,
                last_sent_message_id: 0,
                last_committed_message_id: 0,
                flow_locked: false,
            }),
        }
    }

    pub fn set_query_protocol(&self, value: Rc<dyn QueryProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.query_protocol = value;
    }

    pub fn start(&self) {
        let state = &mut self.state.borrow_mut();
        if let Some(configuration) = state
            .client_store
            .load_group_configuration(self.initial_configuration.id())
        {
            state.configuration = configuration;
        } else {
            let configuration = state.configuration.clone();
            state.client_store.save_group_configuration(configuration);
        }

        state.query_protocol.update_configuration(state.configuration.clone());
        state.heartbeat_timer.start();
    }

    pub fn stop(&self) {
        let state = &mut self.state.borrow_mut();

        self.on_leader_disconnected(true, state);
        state.heartbeat_timer.stop();
    }

    pub fn process(&self) {
        let state = &mut self.state.borrow_mut();
        if state.message_sender.is_some() && !state.in_progress {
            if let Some(request) = self.create_request(state) {
                self.send_request(request, state);
            }
        }
    }

    pub fn on_timer(&self, current_time: Instant) {
        let state = &mut self.state.borrow_mut();
        if state.leader_timeout_timer.is_fired(current_time) {
            self.handle_leader_timeout(state);
        }

        if state.update_configuration_timer.is_fired(current_time) {
            self.request_configuration_update(state);
        }

        if state.heartbeat_timer.is_fired(current_time) {
            self.request_heartbeat(state);
        }
    }

    pub fn publish(&self, value: Vec<u8>, user_data: usize) {
        let state = &mut self.state.borrow_mut();

        let log_entry = LogEntry::new(state.next_message_id, value);
        state.next_message_id += 1;
        state.queue.push_back(EntryInfo { log_entry, user_data });

        self.check_flow(state);
    }

    pub fn handle_succeeded_response(&self, serialized_response: Vec<u8>) {
        let state = &mut self.state.borrow_mut();
        state.in_progress = false;

        if let bincode::Result::Ok(response) = bincode::deserialize::<Response>(&serialized_response) {
            if let Response::Publish(response) = response {
                match state.leader_id {
                    Some(leader_id) => {
                        if response.server_id() != leader_id {
                            return;
                        }
                    }
                    None => return,
                }

                trace!("[{}] Response received: {:?}.", self.endpoint, response);

                if response.is_accepted() {
                    if let Some(configuration) = response.configuration() {
                        self.update_configuration(configuration.clone(), state);
                    }

                    self.handle_accepted_publish_response(response, state);

                    state.leader_timeout_timer.restart();
                } else if response.leader_id().is_none() || response.leader_id() != state.leader_id {
                    debug!(
                        "[{}] Group endpoint is not a leader: {:?}:{:?}.",
                        self.endpoint, state.leader_endpoint, state.leader_id
                    );

                    self.on_leader_disconnected(true, state);

                    if response.leader_id().is_some() {
                        state.leader_id = response.leader_id();
                    }
                }
            }
        }
    }

    pub fn handle_failed_response(&self, server_configuration: ServerConfiguration, error: Error) {
        let state = &mut self.state.borrow_mut();
        state.in_progress = false;

        warn!(
            "[{}] Response error received from server {}: {:?}.",
            self.endpoint,
            server_configuration.endpoint(),
            error
        );

        self.on_leader_disconnected(true, state);
    }
}

impl PublishProtocol for PublishProtocolImpl {
    fn is_connected(&self) -> bool {
        self.state.borrow().message_sender.is_some()
    }

    fn group_configuration(&self) -> GroupConfiguration {
        self.state.borrow().configuration.clone()
    }

    fn next_message_id(&self) -> u64 {
        self.state.borrow().next_message_id
    }

    fn on_disconnected(&self) {
        let state = &mut self.state.borrow_mut();
        self.on_leader_disconnected(false, state);
    }
}

//////////////////////////////////////////////////////////
// Publish Protocol implementation
//////////////////////////////////////////////////////////

impl PublishProtocolImpl {
    fn handle_accepted_publish_response(&self, response: PublishResponse, state: &mut State) {
        if response.is_out_of_order_received() {
            let last_received_message_id = response.last_received_message_id();
            if last_received_message_id == 0 && state.last_committed_message_id > 0 {
                self.request_restart_session(state);
            } else {
                assert!(last_received_message_id >= state.last_committed_message_id);
                state.last_sent_message_id = last_received_message_id;
            }
        }

        let last_committed_message_id = response.last_committed_message_id();
        if last_committed_message_id != 0 {
            self.remove_committed_entries(last_committed_message_id, state);
        }
    }

    fn handle_leader_timeout(&self, state: &mut State) {
        warn!(
            "[{}] Timeout has occurred, when waiting response from leader ''{:?}''.",
            self.endpoint, state.leader_id
        );

        self.on_leader_disconnected(true, state);
    }

    fn create_request(&self, state: &State) -> Option<Request> {
        let mut entries = Vec::new();
        let mut batch_size = 0;

        assert!(state.last_sent_message_id >= state.last_committed_message_id);
        let start = (state.last_sent_message_id - state.last_committed_message_id) as usize;

        for i in start..state.queue.len() {
            let log_entry = state.queue.get(i).unwrap().log_entry.clone();
            let value_length = log_entry.value().len();
            if value_length + batch_size > self.max_batch_size {
                break;
            }

            entries.push(log_entry);
            batch_size += value_length;
        }

        if !entries.is_empty() {
            Some(Request::Publish(PublishRequest::new(
                self.client_id,
                self.group_id,
                entries,
            )))
        } else {
            None
        }
    }

    fn send_request(&self, request: Request, state: &mut State) {
        self.ensure_message_sender(state);

        if state.message_sender.is_some() {
            trace!("[{}] Request sent: {:?}.", self.endpoint, request);

            state.heartbeat_timer.restart();

            state.in_progress = true;
            let serialized_request = bincode::serialize(&request).unwrap();

            state.message_sender.as_ref().unwrap().send(serialized_request);
        }
    }

    fn ensure_message_sender(&self, state: &mut State) {
        if state.message_sender.is_none() {
            let (leader_id, leader_endpoint) = self.leader_configuration(state.leader_id, state);
            state.leader_id = Some(leader_id);
            state.leader_endpoint = Some(leader_endpoint.clone());

            match self.message_sender_factory.create_sender(
                leader_endpoint.clone(),
                Box::new(ClientResponseHandler::new(
                    EventType::Publish,
                    self.sender.clone(),
                    ServerConfiguration::new(leader_id, leader_endpoint),
                )),
            ) {
                Ok(message_sender) => {
                    state.message_sender = Some(Rc::from(message_sender));
                    self.on_leader_connected(state);
                }

                Err(error) => {
                    warn!("[{}] Could not create message sender: {:?}.", self.endpoint, error);

                    self.on_leader_disconnected(true, state);
                }
            }
        }
    }

    fn on_leader_connected(&self, state: &mut State) {
        state.in_progress = false;
        state.leader_timeout_timer.start();

        if !state.configuration.is_single_server() {
            state.update_configuration_timer.start();
        }

        state.query_protocol.on_leader_connected(
            state.message_sender.as_ref().unwrap().clone(),
            ServerConfiguration::new(
                state.leader_id.unwrap(),
                state.leader_endpoint.as_ref().unwrap().to_string(),
            ),
        );

        info!(
            "[{}] Group endpoint has been connected: {:?}:{:?}.",
            self.endpoint, state.leader_endpoint, state.leader_id
        );
    }

    fn on_leader_disconnected(&self, notify_query_protocol: bool, state: &mut State) {
        if state.leader_id.is_some() {
            info!(
                "[{}] Group endpoint has been disconnected: {:?}:{:?}.",
                self.endpoint, state.leader_endpoint, state.leader_id
            );

            state.leader_id = None;
            state.leader_endpoint = None;
            state.in_progress = false;
            state.message_sender = None;
            state.last_sent_message_id = state.last_committed_message_id;

            if notify_query_protocol {
                state.query_protocol.on_leader_disconnected();
            }

            state.leader_timeout_timer.stop();
            state.update_configuration_timer.stop();
        }
    }

    fn leader_configuration(&self, leader_id: Option<Uuid>, state: &State) -> (Uuid, String) {
        if let Some(leader_id) = leader_id {
            let server_configuration = state.configuration.servers().iter().find(|v| v.id() == leader_id);
            if let Some(server_configuration) = server_configuration {
                return (server_configuration.id(), server_configuration.endpoint().clone());
            }
        }

        let mut random = rand::thread_rng();
        let index = random.gen_range(0, state.configuration.servers().len());
        let server_configuration = state.configuration.servers()[index].clone();

        debug!(
            "[{}] Random server has been selected as primary group endpoint: {:?}.",
            self.endpoint, server_configuration
        );

        (server_configuration.id(), server_configuration.endpoint().clone())
    }

    fn request_heartbeat(&self, state: &mut State) {
        if state.in_progress {
            return;
        }

        self.send_request(
            Request::Publish(PublishRequest::new(self.client_id, self.group_id, vec![])),
            state,
        );
    }

    fn request_restart_session(&self, state: &mut State) {
        let request = Request::RestartSession(RestartSessionRequest::new(
            self.client_id,
            self.group_id,
            state.last_sent_message_id,
        ));

        self.send_request(request, state);
    }

    fn request_configuration_update(&self, state: &mut State) {
        if state.in_progress {
            state.update_configuration_timer.delayed_fire();
            return;
        }

        self.send_request(
            Request::Configuration(ConfigurationRequest::new(self.client_id, self.group_id)),
            state,
        );
    }

    fn remove_committed_entries(&self, last_committed_message_id: u64, state: &mut State) {
        loop {
            if let Some(info) = state.queue.front() {
                if info.log_entry.message_id() <= last_committed_message_id {
                    let user_data = info.user_data;

                    state.last_committed_message_id = info.log_entry.message_id();
                    if state.last_sent_message_id < state.last_committed_message_id {
                        state.last_sent_message_id = state.last_committed_message_id;
                    }

                    state.queue.pop_front();

                    self.handler.on_commit(user_data);

                    self.check_flow(state);
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    fn check_flow(&self, state: &mut State) {
        if !state.flow_locked && state.queue.len() >= self.lock_queue_capacity {
            state.flow_locked = true;
            self.flow_controller.lock_flow(FlowType::Publish);
        }

        if state.flow_locked && state.queue.len() <= self.unlock_queue_capacity {
            state.flow_locked = false;
            self.flow_controller.unlock_flow(FlowType::Publish);
        }
    }

    fn update_configuration(&self, configuration: GroupConfiguration, state: &mut State) {
        if state.configuration != configuration {
            debug!(
                "[{}] Group configuration has been updated: {:?}.",
                self.endpoint, configuration
            );

            state.configuration = configuration.clone();
            state.client_store.save_group_configuration(configuration.clone());

            state.query_protocol.update_configuration(configuration);
        }
    }
}
