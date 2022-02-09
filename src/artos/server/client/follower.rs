//////////////////////////////////////////////////////////
// FollowerMessageSender API
//////////////////////////////////////////////////////////

use crate::artos::common::{Error, ResponseHandler};
use crate::artos::common::{MessageSender, ServerConfiguration};
use crate::artos::message::Request;
use crate::artos::server::channel::{Context, Event, ServerState};
use crate::compartment::Sender;
use crate::track;
use crate::track_method;
use log::debug;
use log::info;
use log::trace;
use log::warn;
use rand::Rng;
use std::cell::RefCell;
use std::rc::Rc;
use uuid::Uuid;

include!("follower_tests.rs");

#[derive(Copy, Clone)]
pub enum FollowerType {
    Join,
    Leave,
}

#[cfg_attr(test, mockall::automock)]
pub trait FollowerMessageSender {
    fn start(&self) {
        unimplemented!()
    }

    fn stop(&self) {
        unimplemented!()
    }

    fn send(&self, _request: Request, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn handle_succeeded_response(
        &self,
        _leader_configuration: ServerConfiguration,
        _accepted: bool,
        _leader_id: Option<Uuid>,
    ) -> bool {
        unimplemented!()
    }

    fn handle_failed_response(&self, _leader_configuration: ServerConfiguration, _error: Error) {
        unimplemented!()
    }
}

pub struct NullFollowerMessageSender {}

impl NullFollowerMessageSender {
    pub fn new() -> Rc<dyn FollowerMessageSender> {
        Rc::new(NullFollowerMessageSender {})
    }
}

impl FollowerMessageSender for NullFollowerMessageSender {}

//////////////////////////////////////////////////////////
// FollowerMessageSenderImpl
//////////////////////////////////////////////////////////

pub struct FollowerMessageSenderImpl {
    endpoint: String,
    context: Rc<Context>,
    follower_type: FollowerType,
    sender: Sender<Event>,
    state: RefCell<State>,
}

struct State {
    leader_id: Option<Uuid>,
    leader_endpoint: Option<String>,
    message_sender: Option<Box<dyn MessageSender>>,
}

impl FollowerMessageSenderImpl {
    pub fn new(context: Rc<Context>, follower_type: FollowerType, sender: Sender<Event>) -> FollowerMessageSenderImpl {
        FollowerMessageSenderImpl {
            endpoint: context.local_server().endpoint().clone(),
            context,
            follower_type,
            sender,
            state: RefCell::new(State {
                leader_id: None,
                leader_endpoint: None,
                message_sender: None,
            }),
        }
    }
}

impl FollowerMessageSender for FollowerMessageSenderImpl {
    fn start(&self) {}

    fn stop(&self) {
        track_method!(t, "stop");

        let state = &mut self.state.borrow_mut();
        self.on_leader_disconnected(state);

        track!(t, "end");
    }

    fn send(&self, request: Request, server_state: &mut ServerState) {
        track_method!(t, "send");

        let state = &mut self.state.borrow_mut();
        if self.ensure_message_sender(state, server_state) {
            track!(t, "send");

            trace!("[{}] Request sent: {:?}.", self.endpoint, request);

            let serialized_request = bincode::serialize(&request).unwrap();

            state.message_sender.as_ref().unwrap().send(serialized_request);
        }

        track!(t, "end");
    }

    fn handle_succeeded_response(
        &self,
        leader_configuration: ServerConfiguration,
        accepted: bool,
        leader_id: Option<Uuid>,
    ) -> bool {
        track_method!(t, "handle_succeeded_response");

        let state = &mut self.state.borrow_mut();

        if accepted {
            track!(t, "end");

            true
        } else {
            track!(t, "rejected");

            if leader_id.is_none() || leader_id.as_ref().unwrap() != &leader_configuration.id() {
                debug!(
                    "[{}] Group endpoint is not a leader: {:?}.",
                    self.endpoint, leader_configuration
                );

                self.on_leader_disconnected(state);

                state.leader_id = leader_id;
            }

            false
        }
    }

    fn handle_failed_response(&self, leader_configuration: ServerConfiguration, error: Error) {
        track_method!(t, "handle_failed_response");

        let state = &mut self.state.borrow_mut();

        warn!(
            "[{}] Response error received from server {}: {:?}.",
            self.endpoint,
            leader_configuration.endpoint(),
            error
        );

        self.on_leader_disconnected(state);

        track!(t, "end");
    }
}

impl FollowerMessageSenderImpl {
    fn on_leader_disconnected(&self, state: &mut State) {
        track_method!(t, "on_leader_disconnected");

        if state.leader_id.is_some() {
            info!(
                "[{}] Group endpoint has been disconnected: {:?}:{:?}.",
                self.endpoint, state.leader_endpoint, state.leader_id
            );

            state.leader_id = None;
            state.leader_endpoint = None;
            state.message_sender = None;
        }

        track!(t, "end");
    }

    fn ensure_message_sender(&self, state: &mut State, server_state: &ServerState) -> bool {
        track_method!(t, "ensure_message_sender");

        if state.message_sender.is_none() {
            let (leader_id, leader_endpoint) = self.ensure_leader_configuration(state, server_state);
            match self.context.message_sender_factory().create_sender(
                leader_endpoint.clone(),
                FollowerResponseHandler::new(
                    self.follower_type,
                    ServerConfiguration::new(leader_id, leader_endpoint.clone()),
                    self.sender.clone(),
                ),
            ) {
                Result::Ok(message_sender) => {
                    track!(t, "end");

                    state.message_sender = Some(message_sender);
                    state.leader_id = Some(leader_id);
                    state.leader_endpoint = Some(leader_endpoint);
                    true
                }
                Result::Err(error) => {
                    track!(t, "error");

                    warn!(
                        "[{}] Could not create message sender {}. Error: {:?}",
                        self.endpoint, leader_endpoint, error
                    );
                    self.on_leader_disconnected(state);
                    false
                }
            }
        } else {
            true
        }
    }

    fn ensure_leader_configuration(&self, state: &mut State, server_state: &ServerState) -> (Uuid, String) {
        track_method!(t, "ensure_leader_configuration");

        if state.leader_id.is_none() {
            state.leader_id = server_state.leader_id;
        }

        let (leader_id, leader_endpoint) = self.select_leader_configuration(state.leader_id, server_state);
        state.leader_id = Some(leader_id);
        state.leader_endpoint = Some(leader_endpoint.clone());

        track!(t, "end");

        (leader_id, leader_endpoint)
    }

    fn select_leader_configuration(&self, leader_id: Option<Uuid>, server_state: &ServerState) -> (Uuid, String) {
        track_method!(t, "select_leader_configuration");

        assert!(server_state.configuration.servers().len() > 1);

        let servers: Vec<&ServerConfiguration> = server_state
            .configuration
            .servers()
            .iter()
            .filter(|x| x.id() != self.context.local_server().id())
            .collect();

        if let Some(leader_id) = leader_id {
            let server_configuration = servers.iter().find(|v| v.id() == leader_id);
            if let Some(server_configuration) = server_configuration {
                track!(t, "leader_found");
                return (server_configuration.id(), server_configuration.endpoint().clone());
            }
        }

        let mut random = rand::thread_rng();
        let index = random.gen_range(0, servers.len());
        let server_configuration = servers[index].clone();

        debug!(
            "[{}] Random server has been selected as primary group endpoint: {:?}.",
            self.endpoint, server_configuration
        );

        track!(t, "random_leader");

        (server_configuration.id(), server_configuration.endpoint().clone())
    }
}

//////////////////////////////////////////////////////////
// FollowerResponseHandler
//////////////////////////////////////////////////////////

struct FollowerResponseHandler {
    follower_type: FollowerType,
    leader_configuration: ServerConfiguration,
    sender: Sender<Event>,
}

impl FollowerResponseHandler {
    fn new(
        follower_type: FollowerType,
        leader_configuration: ServerConfiguration,
        sender: Sender<Event>,
    ) -> Box<dyn ResponseHandler> {
        Box::new(FollowerResponseHandler {
            follower_type,
            leader_configuration,
            sender,
        })
    }
}

impl ResponseHandler for FollowerResponseHandler {
    fn on_succeeded(&self, response: Vec<u8>) {
        self.sender.send(Event::FollowerResponse((
            self.follower_type,
            self.leader_configuration.clone(),
            response,
        )))
    }

    fn on_failed(&self, error: Error) {
        self.sender.send(Event::FollowerError((
            self.follower_type,
            self.leader_configuration.clone(),
            error,
        )))
    }
}
