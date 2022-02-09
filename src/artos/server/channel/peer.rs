use crate::artos::common::Error;
use crate::artos::common::{MessageSender, ResponseHandler, ServerConfiguration};
use crate::artos::message::Request;
use crate::artos::server::channel::{Context, Event};
use crate::common::utils::Time;
use crate::compartment::{Sender, Timer};
use crate::track;
use crate::track_method;
use log::info;
use log::warn;
use std::cell::RefCell;
use std::cmp::min;
use std::rc::Rc;
use std::time::{Duration, Instant};
use uuid::Uuid;

include!("peer_tests.rs");

//////////////////////////////////////////////////////////
// ServerPeer
//////////////////////////////////////////////////////////

pub struct ServerPeer {
    endpoint: String,
    configuration: ServerConfiguration,
    context: Rc<Context>,
    sender: Sender<Event>,
    heartbeat_period: Duration,
    max_heartbeat_period: Duration,
    send_backoff_period: Duration,
    state: RefCell<State>,
}

struct State {
    heartbeat_enabled: bool,
    current_heartbeat_period: Duration,
    next_heartbeat_time: Instant,
    next_log_index: u64,
    matched_index: u64,
    rewind_step: u64,
    last_response_time: Instant,
    in_progress: bool,
    message_sender: Option<Box<dyn MessageSender>>,
    peer_timeout_timer: Timer,
    started: bool,
}

impl ServerPeer {
    pub fn new(
        endpoint: String,
        configuration: ServerConfiguration,
        context: Rc<Context>,
        sender: Sender<Event>,
    ) -> ServerPeer {
        let heartbeat_period = context.server_channel_factory_configuration().heartbeat_period();
        let max_heartbeat_period = context.server_channel_factory_configuration().max_heartbeat_period();
        let send_backoff_period = context.server_channel_factory_configuration().send_failure_backoff();
        let peer_timeout = context.server_channel_factory_configuration().peer_timeout();
        ServerPeer {
            endpoint,
            configuration,
            context,
            sender,
            heartbeat_period,
            max_heartbeat_period,
            send_backoff_period,
            state: RefCell::new(State {
                heartbeat_enabled: false,
                current_heartbeat_period: heartbeat_period,
                next_heartbeat_time: Time::now(),
                next_log_index: 1,
                matched_index: 0,
                rewind_step: 1,
                last_response_time: Time::now(),
                in_progress: false,
                message_sender: None,
                peer_timeout_timer: Timer::new(false, peer_timeout),
                started: false,
            }),
        }
    }

    pub fn configuration(&self) -> &ServerConfiguration {
        &self.configuration
    }

    pub fn is_connected(&self) -> bool {
        self.state.borrow().message_sender.is_some()
    }

    pub fn is_request_in_progress(&self) -> bool {
        self.state.borrow().in_progress
    }

    pub fn is_started(&self) -> bool {
        self.state.borrow().started
    }

    pub fn start(&self) {
        track_method!(t, "start");

        let state = &mut self.state.borrow_mut();
        state.heartbeat_enabled = true;

        self.resume_heartbeating(state);

        self.update_next_heartbeat_time(state);

        state.in_progress = false;
        state.last_response_time = Time::now();
        state.started = true;

        track!(t, "end");
    }

    pub fn stop(&self) {
        track_method!(t, "stop");

        let state = &mut self.state.borrow_mut();
        state.heartbeat_enabled = false;

        self.on_peer_disconnected(state);

        state.started = false;

        track!(t, "end");
    }

    pub fn can_heartbeat(&self, current_time: Instant) -> bool {
        track_method!(t, "can_heartbeat");

        let state = &mut self.state.borrow_mut();
        if state.peer_timeout_timer.is_fired(current_time) {
            self.handle_peer_timeout(state);
        }

        if !state.heartbeat_enabled {
            track!(t, "heartbeat_disabled");
            return false;
        }

        if current_time >= state.next_heartbeat_time {
            if self.ensure_message_sender(state) {
                track!(t, "end");
                return true;
            }
        }

        track!(t, "end_false");
        return false;
    }

    pub fn last_response_time(&self) -> Instant {
        self.state.borrow().last_response_time
    }

    pub fn next_log_index(&self) -> u64 {
        self.state.borrow().next_log_index
    }

    pub fn set_next_log_index(&self, value: u64) {
        self.state.borrow_mut().next_log_index = value;
    }

    pub fn rewind_step(&self) -> u64 {
        self.state.borrow().rewind_step
    }

    pub fn set_rewind_step(&self, value: u64) {
        self.state.borrow_mut().rewind_step = value;
    }

    pub fn matched_index(&self) -> u64 {
        self.state.borrow().matched_index
    }

    pub fn set_matched_index(&self, value: u64) {
        self.state.borrow_mut().matched_index = value;
    }

    pub fn send(&self, request: Request) {
        track_method!(t, "send");

        let state = &mut self.state.borrow_mut();
        self.update_next_heartbeat_time(state);

        if self.ensure_message_sender(state) {
            track!(t, "send");

            let serialized_request = bincode::serialize(&request).unwrap();
            state.in_progress = true;

            state.message_sender.as_ref().unwrap().send(serialized_request);
        }

        track!(t, "end");
    }

    pub fn handle_succeeded_response(&self) {
        track_method!(t, "handle_succeeded_response");

        let state = &mut self.state.borrow_mut();
        state.in_progress = false;
        self.resume_heartbeating(state);
        state.last_response_time = Time::now();
        state.peer_timeout_timer.restart();

        track!(t, "end");
    }

    pub fn handle_failed_response(&self) {
        track_method!(t, "handle_failed_response");

        let state = &mut self.state.borrow_mut();
        state.in_progress = false;
        self.slow_down_heartbeating(state);
        state.last_response_time = Time::now();
        self.on_peer_disconnected(state);

        track!(t, "end");
    }
}

impl ServerPeer {
    fn resume_heartbeating(&self, state: &mut State) {
        track_method!(t, "resume_heartbeating");

        if state.current_heartbeat_period > self.heartbeat_period {
            state.current_heartbeat_period = self.heartbeat_period;
            self.update_next_heartbeat_time(state);
        }

        track!(t, "end");
    }

    fn slow_down_heartbeating(&self, state: &mut State) {
        track_method!(t, "slow_down_heartbeating");

        state.current_heartbeat_period = min(
            self.max_heartbeat_period,
            state.current_heartbeat_period + self.send_backoff_period,
        );
        self.update_next_heartbeat_time(state);

        track!(t, "end");
    }

    fn update_next_heartbeat_time(&self, state: &mut State) {
        track_method!(t, "update_next_heartbeat_time");

        state.next_heartbeat_time = Time::now() + state.current_heartbeat_period;

        track!(t, "end");
    }

    fn on_peer_connected(&self, state: &mut State) {
        track_method!(t, "on_peer_connected");

        state.in_progress = false;
        state.peer_timeout_timer.start();

        info!(
            "[{}] Peer has been connected: {}.",
            self.endpoint,
            self.configuration.endpoint()
        );

        track!(t, "end");
    }

    fn on_peer_disconnected(&self, state: &mut State) {
        track_method!(t, "on_peer_disconnected");

        if state.message_sender.is_none() {
            track!(t, "already_disconnected");
            return;
        }

        state.message_sender = None;
        state.in_progress = false;
        state.peer_timeout_timer.stop();

        info!(
            "[{}] Peer has been disconnected: {}.",
            self.endpoint,
            self.configuration.endpoint()
        );

        track!(t, "end");
    }

    fn handle_peer_timeout(&self, state: &mut State) {
        track_method!(t, "handle_peer_timeout");

        warn!(
            "[{}] Timeout has occured for peer {}.",
            self.endpoint,
            self.configuration.endpoint()
        );

        self.on_peer_disconnected(state);

        track!(t, "end");
    }

    fn ensure_message_sender(&self, state: &mut State) -> bool {
        track_method!(t, "ensure_message_sender");

        if state.message_sender.is_none() {
            track!(t, "end");

            match self.context.message_sender_factory().create_sender(
                self.configuration.endpoint().clone(),
                PeerResponseHandler::new(self.configuration.id(), self.sender.clone()),
            ) {
                Result::Ok(message_sender) => {
                    state.message_sender = Some(message_sender);
                    self.on_peer_connected(state);
                    true
                }
                Result::Err(error) => {
                    warn!(
                        "[{}] Could not create message sender {}. Error: {:?}",
                        self.endpoint,
                        self.configuration.endpoint(),
                        error
                    );
                    self.on_peer_disconnected(state);
                    false
                }
            }
        } else {
            track!(t, "already_created");
            true
        }
    }
}

//////////////////////////////////////////////////////////
// PeerResponseHandler
//////////////////////////////////////////////////////////

struct PeerResponseHandler {
    server_id: Uuid,
    sender: Sender<Event>,
}

impl PeerResponseHandler {
    fn new(server_id: Uuid, sender: Sender<Event>) -> Box<dyn ResponseHandler> {
        Box::new(PeerResponseHandler { server_id, sender })
    }
}

impl ResponseHandler for PeerResponseHandler {
    fn on_succeeded(&self, response: Vec<u8>) {
        self.sender.send(Event::PeerResponse((self.server_id, response)))
    }

    fn on_failed(&self, error: Error) {
        self.sender.send(Event::PeerError((self.server_id, error)))
    }
}
