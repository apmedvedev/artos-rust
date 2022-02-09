use uuid::Uuid;

use crate::artos::common::{ResponseHandler, ServerConfiguration};
use crate::artos::message::{Request, Response};
use crate::artos::server::api::{MessageListener, MessageReceiver};
use crate::artos::server::channel::{Context, ServerPeer, ServerState};
use crate::artos::server::client::{FollowerType, LeaderClientProtocol};
use crate::artos::server::election::ElectionProtocol;
use crate::artos::server::membership::{JoinGroupProtocol, LeaveGroupProtocol};
use crate::artos::server::replication::{QueryProtocol, ReplicationProtocol};
use crate::artos::server::state::{StateTransferCompletionType, StateTransferProtocol};
use crate::artos::server::utils;
use crate::compartment::{Handler, Sender};
use log::debug;
use log::trace;
use log::warn;
use std::cell::RefCell;

use crate::artos::common::{Error, ErrorKind};
use crate::artos::server::utils::update_term;
use std::fmt::Debug;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

//////////////////////////////////////////////////////////
// Event
//////////////////////////////////////////////////////////

type Job = Box<dyn FnOnce(&ServerImpl) + Send + 'static>;

pub enum Event {
    Job(Job),
    Request((Vec<u8>, Arc<dyn ResponseHandler>)),
    PeerResponse((Uuid, Vec<u8>)),
    PeerError((Uuid, Error)),
    FollowerResponse((FollowerType, ServerConfiguration, Vec<u8>)),
    FollowerError((FollowerType, ServerConfiguration, Error)),
    Publish((Vec<u8>, usize)),
    StateTransferCompleted(StateTransferCompletionType),
}

//////////////////////////////////////////////////////////
// Server API
//////////////////////////////////////////////////////////

#[cfg_attr(test, mockall::automock)]
pub trait Server {
    fn create_peer(&self, configuration: ServerConfiguration) -> ServerPeer;

    fn send(&self, peer: &ServerPeer, request: Request);
}

//////////////////////////////////////////////////////////
// ServerImpl
//////////////////////////////////////////////////////////

pub struct ServerImpl {
    endpoint: String,
    context: Rc<Context>,
    server_state: Rc<RefCell<ServerState>>,
    election_protocol: Rc<dyn ElectionProtocol>,
    replication_protocol: Rc<dyn ReplicationProtocol>,
    state_transfer_protocol: Rc<dyn StateTransferProtocol>,
    join_group_protocol: Rc<dyn JoinGroupProtocol>,
    leave_group_protocol: Rc<dyn LeaveGroupProtocol>,
    leader_client_protocol: Rc<dyn LeaderClientProtocol>,
    query_protocol: Rc<dyn QueryProtocol>,
    message_listener: Rc<dyn MessageListener>,
}

impl Handler<Event> for ServerImpl {
    fn process(&self) {}

    fn on_timer(&self, current_time: Instant) {
        let server_state = &mut self.server_state.borrow_mut();
        self.election_protocol.on_timer(current_time, server_state);
        self.replication_protocol.on_timer(current_time, server_state);
        self.join_group_protocol.on_timer(current_time, server_state);
        self.leave_group_protocol.on_timer(current_time, server_state);
        self.leader_client_protocol.on_timer(current_time, server_state);
        self.query_protocol.on_timer(current_time);
    }

    fn handle(&self, event: Event) {
        let server_state = &mut self.server_state.borrow_mut();
        match event {
            Event::Job(job) => job(self),
            Event::Request((request, response_handler)) => {
                if let Result::Ok(request) = bincode::deserialize::<Request>(&request) {
                    if self.is_async_request(&request) {
                        self.handle_async(request, response_handler, server_state)
                    } else {
                        self.handle_sync(request, response_handler, server_state);
                    }
                }
            }

            Event::PeerResponse((server_id, response)) => {
                if let Result::Ok(response) = bincode::deserialize(&response) {
                    self.handle_peer_response(server_id, response, server_state);
                }
            }

            Event::PeerError((server_id, error)) => {
                self.handle_peer_error(server_id, error, server_state);
            }

            Event::FollowerResponse((follower_type, leader_configuration, response)) => {
                if let Result::Ok(response) = bincode::deserialize::<Response>(&response) {
                    if response.server_id() == leader_configuration.id() {
                        self.handle_follower_response(follower_type, leader_configuration, response, server_state);
                    }
                }
            }

            Event::FollowerError((follower_type, leader_configuration, error)) => {
                self.handle_follower_error(follower_type, leader_configuration, error);
            }

            Event::Publish((value, user_data)) => {
                self.handle_publish(value, user_data, server_state);
            }

            Event::StateTransferCompleted(completion_type) => {
                self.on_state_transfer_completed(completion_type);
            }
        }
    }
}

impl Drop for ServerImpl {
    fn drop(&mut self) {
        self.stop();
    }
}

impl ServerImpl {
    fn start(&mut self) {
        let server_state = &mut self.server_state.borrow_mut();
        self.replication_protocol.start(server_state);
        self.election_protocol.start(server_state);
        self.join_group_protocol.start();
        self.leave_group_protocol.start();
        self.state_transfer_protocol.start();
        self.query_protocol.start();
    }

    fn stop(&mut self) {
        let server_state = &mut self.server_state.borrow_mut();
        for peer in &server_state.peers {
            peer.stop();
        }

        self.replication_protocol.stop();
        self.election_protocol.stop();
        self.join_group_protocol.stop();
        self.leave_group_protocol.stop();
        self.state_transfer_protocol.stop();
        self.query_protocol.stop();
    }

    fn is_async_request(&self, request: &Request) -> bool {
        if let Request::Query(_) = request {
            true
        } else if let Request::Subscribe(_) = request {
            true
        } else {
            false
        }
    }

    fn handle_sync(
        &self,
        request: Request,
        response_handler: Arc<dyn ResponseHandler>,
        server_state: &mut ServerState,
    ) {
        if request.group_id() == server_state.configuration.id() {
            trace!(
                "[{}] Request received: {:?}.",
                self.endpoint,
                utils::to_string(&request, request.id(), &server_state, true)
            );

            let request_id = request.id();

            let response = self.handle_sync_request(request, server_state);
            let serialized_response = bincode::serialize(&response).unwrap();

            trace!(
                "[{}] Response {:?} sent to {}.",
                self.endpoint,
                utils::to_string(&response, response.server_id(), &server_state, true),
                utils::to_string_id(request_id, &server_state)
            );

            response_handler.on_succeeded(serialized_response);
        } else {
            let error = Box::new(ErrorKind::InvalidGroupId(request.group_id()));
            response_handler.on_failed(error);
        }
    }

    fn handle_async(
        &self,
        request: Request,
        response_handler: Arc<dyn ResponseHandler>,
        server_state: &mut ServerState,
    ) {
        if request.group_id() == server_state.configuration.id() {
            trace!(
                "[{}] Request received: {:?}.",
                self.endpoint,
                utils::to_string(&request, request.id(), &server_state, true)
            );

            self.handle_async_request(request, response_handler);
        } else {
            let error = Box::new(ErrorKind::InvalidGroupId(request.group_id()));
            response_handler.on_failed(error);
        }
    }

    fn handle_sync_request(&self, request: Request, server_state: &mut ServerState) -> Response {
        match request {
            Request::AppendEntries(request) => Response::AppendEntries(
                self.replication_protocol
                    .handle_append_entries_request(request, server_state),
            ),
            Request::Vote(request) => Response::Vote(self.election_protocol.handle_vote_request(request, server_state)),
            Request::Publish(request) => {
                Response::Publish(self.replication_protocol.handle_publish_request(request, server_state))
            }
            Request::Configuration(request) => Response::Publish(
                self.replication_protocol
                    .handle_configuration_request(request, server_state),
            ),
            Request::RestartSession(request) => Response::Publish(
                self.replication_protocol
                    .handle_restart_session_request(request, server_state),
            ),
            Request::JoinGroup(request) => Response::JoinGroup(
                self.join_group_protocol
                    .handle_join_group_request(request, server_state),
            ),
            Request::LeaveGroup(request) => Response::LeaveGroup(
                self.leave_group_protocol
                    .handle_leave_group_request(request, server_state),
            ),
            _ => panic!(),
        }
    }

    fn handle_async_request(&self, request: Request, response_handler: Arc<dyn ResponseHandler>) {
        match request {
            Request::Query(request) => self.query_protocol.handle_query_request(request, response_handler),
            Request::Subscribe(request) => self
                .query_protocol
                .handle_subscription_request(request, response_handler),
            _ => panic!(),
        }
    }

    fn handle_peer_response(&self, server_id: Uuid, response: Response, server_state: &mut ServerState) {
        if let Some(peer) = server_state.peers.iter().find(|x| x.configuration().id() == server_id) {
            peer.handle_succeeded_response();

            trace!("[{}] Response received: {:?}.", self.endpoint, response);

            match response {
                Response::Vote(response) => {
                    if self.check_term(&response, response.term(), server_state) {
                        self.election_protocol.handle_vote_response(response, server_state);
                    }
                }

                Response::AppendEntries(response) => {
                    if self.check_term(&response, response.term(), server_state) {
                        self.replication_protocol
                            .handle_append_entries_response(response, server_state);
                    }
                }

                _ => assert!(false),
            }
        }
    }

    fn handle_peer_error(&self, server_id: Uuid, error: Error, server_state: &mut ServerState) {
        if let Some(peer) = server_state.peers.iter().find(|x| x.configuration().id() == server_id) {
            peer.handle_failed_response();

            warn!(
                "[{}] Response error received from server {}: {}",
                self.endpoint,
                peer.configuration().endpoint(),
                error
            );
        }
    }

    fn check_term<R: Debug>(&self, response: &R, term: u64, server_state: &mut ServerState) -> bool {
        if update_term(&self.context, server_state, term) {
            self.election_protocol.become_follower(server_state);
            return false;
        }
        if term < server_state.persistent.term() {
            debug!(
                "[{}] Received response {:?} has lower term than term of current server {}.",
                self.endpoint,
                response,
                server_state.persistent.term()
            );
            return false;
        }

        return true;
    }

    fn handle_follower_response(
        &self,
        follower_type: FollowerType,
        leader_configuration: ServerConfiguration,
        response: Response,
        server_state: &mut ServerState,
    ) {
        trace!("[{}] Response received: {:?}.", self.endpoint, response);

        match follower_type {
            FollowerType::Join => {
                if let Response::JoinGroup(response) = response {
                    self.join_group_protocol
                        .handle_join_group_response(leader_configuration, response, server_state)
                }
            }

            FollowerType::Leave => {
                if let Response::LeaveGroup(response) = response {
                    self.leave_group_protocol
                        .handle_leave_group_response(leader_configuration, response, server_state)
                }
            }
        }
    }

    fn handle_follower_error(
        &self,
        follower_type: FollowerType,
        leader_configuration: ServerConfiguration,
        error: Error,
    ) {
        warn!(
            "[{}] Response error received from server {}: {}",
            self.endpoint,
            leader_configuration.endpoint(),
            error
        );

        match follower_type {
            FollowerType::Join => self
                .join_group_protocol
                .handle_join_group_response_error(leader_configuration, error),

            FollowerType::Leave => self
                .leave_group_protocol
                .handle_leave_group_response_error(leader_configuration, error),
        }
    }

    fn handle_publish(&self, value: Vec<u8>, user_data: usize, server_state: &mut ServerState) {
        self.leader_client_protocol.publish(value, user_data, server_state);
    }

    fn on_state_transfer_completed(&self, completion_type: StateTransferCompletionType) {
        match completion_type {
            StateTransferCompletionType::Join => self.join_group_protocol.on_state_transfer_completed(),
            StateTransferCompletionType::Replication => self.replication_protocol.on_state_transfer_completed(),
        }
    }
}

//////////////////////////////////////////////////////////
// ServerImplFactory
//////////////////////////////////////////////////////////

struct ServerImplFactory {
    // TODO:
}

//////////////////////////////////////////////////////////
// MessageReceiverImpl
//////////////////////////////////////////////////////////

struct MessageReceiverImpl {
    sender: Sender<Event>,
}

impl MessageReceiverImpl {
    pub fn new(sender: Sender<Event>) -> Box<dyn MessageReceiver> {
        Box::new(MessageReceiverImpl { sender })
    }
}
impl MessageReceiver for MessageReceiverImpl {
    fn receive(&self, request: Vec<u8>, response: Arc<dyn ResponseHandler>) {
        self.sender.send(Event::Request((request, response)));
    }
}
