use crate::artos::common::{Error, ErrorKind};
use crate::artos::common::{GroupConfiguration, ServerConfiguration};
use crate::artos::message::server::{LeaveGroupRequest, LeaveGroupResponse, LogEntry, LogValueType, NULL_UUID};
use crate::artos::message::Request;
use crate::artos::server::channel::{Context, ServerRole, ServerState};
use crate::artos::server::client::{FollowerMessageSender, NullFollowerMessageSender};
use crate::artos::server::election::{ElectionProtocol, NullElectionProtocol};
use crate::artos::server::membership::{MembershipProtocol, NullMembershipProtocol};
use crate::artos::server::replication::{NullReplicationProtocol, ReplicationProtocol};
use crate::artos::server::utils::CompletionHandler;
use crate::common::utils::Time;
use crate::compartment::Timer;
use crate::track;
use crate::track_method;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use uuid::Uuid;

include!("leave_tests.rs");

//////////////////////////////////////////////////////////
// LeaveGroupProtocol API
//////////////////////////////////////////////////////////

#[cfg_attr(test, mockall::automock)]
pub trait LeaveGroupProtocol {
    fn start(&self) {
        unimplemented!()
    }

    fn stop(&self) {
        unimplemented!()
    }

    fn on_timer(&self, _current_time: Instant, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn on_leader(&self) {
        unimplemented!()
    }

    fn on_follower(&self) {
        unimplemented!()
    }

    fn graceful_close(&self, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn leave_group(&self, _completion_handler: Box<dyn CompletionHandler<(), Error>>, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn on_left(&self) {
        unimplemented!()
    }

    fn handle_leave_group_request(
        &self,
        _request: LeaveGroupRequest,
        _server_state: &mut ServerState,
    ) -> LeaveGroupResponse {
        unimplemented!()
    }

    fn handle_leave_group_response(
        &self,
        _leader_configuration: ServerConfiguration,
        _response: LeaveGroupResponse,
        _server_state: &mut ServerState,
    ) {
        unimplemented!()
    }

    fn handle_leave_group_response_error(&self, _leader_configuration: ServerConfiguration, _error: Error) {
        unimplemented!()
    }
}

pub struct NullLeaveGroupProtocol {}

impl NullLeaveGroupProtocol {
    pub fn new() -> Rc<dyn LeaveGroupProtocol> {
        Rc::new(NullLeaveGroupProtocol {})
    }
}

impl LeaveGroupProtocol for NullLeaveGroupProtocol {}

//////////////////////////////////////////////////////////
// SingleServerLeaveGroupProtocolImpl
//////////////////////////////////////////////////////////

struct SingleServerLeaveGroupProtocolImpl {}

impl SingleServerLeaveGroupProtocolImpl {
    fn new() -> SingleServerLeaveGroupProtocolImpl {
        SingleServerLeaveGroupProtocolImpl {}
    }
}

impl LeaveGroupProtocol for SingleServerLeaveGroupProtocolImpl {
    fn start(&self) {}

    fn stop(&self) {}

    fn on_timer(&self, _current_time: Instant, _server_state: &mut ServerState) {}

    fn on_leader(&self) {}

    fn on_follower(&self) {}

    fn graceful_close(&self, _server_state: &mut ServerState) {}

    fn leave_group(&self, _completion_handler: Box<dyn CompletionHandler<(), Error>>, _server_state: &mut ServerState) {
    }

    fn on_left(&self) {}

    fn handle_leave_group_request(
        &self,
        _request: LeaveGroupRequest,
        _server_state: &mut ServerState,
    ) -> LeaveGroupResponse {
        unimplemented!()
    }

    fn handle_leave_group_response(
        &self,
        _leader_configuration: ServerConfiguration,
        _response: LeaveGroupResponse,
        _server_state: &mut ServerState,
    ) {
        unimplemented!()
    }

    fn handle_leave_group_response_error(&self, _leader_configuration: ServerConfiguration, _error: Error) {
        unimplemented!()
    }
}

//////////////////////////////////////////////////////////
// LeaveGroupProtocolImpl
//////////////////////////////////////////////////////////

pub struct LeaveGroupProtocolImpl {
    endpoint: String,
    context: Rc<Context>,
    state: RefCell<State>,
}

struct State {
    membership_protocol: Rc<dyn MembershipProtocol>,
    replication_protocol: Rc<dyn ReplicationProtocol>,
    election_protocol: Rc<dyn ElectionProtocol>,
    message_sender: Rc<dyn FollowerMessageSender>,
    leave_group_timer: Timer,
    auto_leave_group_timeout: Timer,
    leave_group_completion_handler: Option<Box<dyn CompletionHandler<(), Error>>>,
}

impl LeaveGroupProtocolImpl {
    pub fn new(context: Rc<Context>) -> LeaveGroupProtocolImpl {
        LeaveGroupProtocolImpl {
            endpoint: context.local_server().endpoint().clone(),
            context,
            state: RefCell::new(State {
                membership_protocol: NullMembershipProtocol::new(),
                replication_protocol: NullReplicationProtocol::new(),
                election_protocol: NullElectionProtocol::new(),
                message_sender: NullFollowerMessageSender::new(),
                leave_group_timer: Timer::new(false, Duration::from_secs(0)),
                auto_leave_group_timeout: Timer::new(false, Duration::from_secs(0)),
                leave_group_completion_handler: None,
            }),
        }
    }

    pub fn set_membership_protocol(&self, value: Rc<dyn MembershipProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.membership_protocol = value;
    }

    pub fn set_replication_protocol(&self, value: Rc<dyn ReplicationProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.replication_protocol = value;
    }

    pub fn set_election_protocol(&self, value: Rc<dyn ElectionProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.election_protocol = value;
    }

    pub fn set_message_sender(&self, value: Rc<dyn FollowerMessageSender>) {
        let state = &mut self.state.borrow_mut();
        state.message_sender = value;
    }
}

impl LeaveGroupProtocol for LeaveGroupProtocolImpl {
    fn start(&self) {
        track_method!(t, "start");

        let state = &mut self.state.borrow_mut();

        state.message_sender.start();

        state
            .leave_group_timer
            .set_period(self.context.server_channel_factory_configuration().leave_group_period());

        if self
            .context
            .server_channel_factory_configuration()
            .auto_leave_group_timeout()
            .as_millis()
            != 0
        {
            track!(t, "auto_leave");

            state.auto_leave_group_timeout.set_period(
                self.context
                    .server_channel_factory_configuration()
                    .auto_leave_group_timeout(),
            );
        }

        track!(t, "end");
    }

    fn stop(&self) {
        track_method!(t, "stop");

        let state = &mut self.state.borrow_mut();
        state.message_sender.stop();

        track!(t, "end");
    }

    fn on_timer(&self, current_time: Instant, server_state: &mut ServerState) {
        track_method!(t, "on_timer");

        let state = &mut self.state.borrow_mut();
        if state.leave_group_timer.is_fired(current_time) {
            track!(t, "leave_timer");

            self.request_leave_group(state, server_state);
        }

        if state.auto_leave_group_timeout.is_fired(current_time) {
            track!(t, "auto_leave_timeout");

            self.handle_auto_leave_group_timeout(state, server_state);
        }

        track!(t, "end");
    }

    fn on_leader(&self) {
        track_method!(t, "on_leader");

        let state = &mut self.state.borrow_mut();
        if self
            .context
            .server_channel_factory_configuration()
            .auto_leave_group_timeout()
            .as_millis()
            != 0
        {
            track!(t, "auto_leave_start");

            state.auto_leave_group_timeout.start();
        }

        track!(t, "end");
    }

    fn on_follower(&self) {
        track_method!(t, "on_follower");

        let state = &mut self.state.borrow_mut();
        if self
            .context
            .server_channel_factory_configuration()
            .auto_leave_group_timeout()
            .as_millis()
            != 0
        {
            track!(t, "auto_leave_stop");

            state.auto_leave_group_timeout.stop();
        }

        track!(t, "end");
    }

    fn graceful_close(&self, server_state: &mut ServerState) {
        track_method!(t, "graceful_close");

        let state = &mut self.state.borrow_mut();
        if server_state.role == ServerRole::Leader && !server_state.peers.is_empty() {
            track!(t, "leader");

            state.election_protocol.become_follower(server_state);
            state.election_protocol.lock_election();
        }

        track!(t, "end");
    }

    fn leave_group(&self, completion_handler: Box<dyn CompletionHandler<(), Error>>, server_state: &mut ServerState) {
        track_method!(t, "leave_group");

        let state = &mut self.state.borrow_mut();
        if server_state.peers.is_empty() {
            track!(t, "single_server");

            let error = Box::new(ErrorKind::SingleServerGroup);
            completion_handler.on_failed(error);
            return;
        }

        if server_state.role == ServerRole::Leader {
            track!(t, "leader");

            state.election_protocol.become_follower(server_state);
            state.election_protocol.lock_election();
        }

        self.leave_group_follower(completion_handler, state);

        track!(t, "end");
    }

    fn on_left(&self) {
        track_method!(t, "on_left");

        let state = &mut self.state.borrow_mut();
        state.leave_group_timer.stop();
        if let Some(completion_handler) = state.leave_group_completion_handler.as_ref() {
            completion_handler.on_succeeded(());
            state.leave_group_completion_handler = None;
        }

        track!(t, "end");
    }

    fn handle_leave_group_request(
        &self,
        request: LeaveGroupRequest,
        server_state: &mut ServerState,
    ) -> LeaveGroupResponse {
        track_method!(t, "handle_leave_group_request");

        let state = &mut self.state.borrow_mut();

        if server_state.role != ServerRole::Leader || server_state.configuration_changing {
            track!(t, "rejected");

            return LeaveGroupResponse::new(
                self.context.local_server().id(),
                self.context.group_id(),
                false,
                server_state.leader_id,
                None,
                false,
            );
        }

        let mut already_left = false;
        let mut configuration = None;
        if server_state
            .configuration
            .servers()
            .iter()
            .find(|x| x.id() == request.server_id())
            .is_some()
        {
            track!(t, "leave_from_group");

            self.remove_servers_from_group(vec![request.server_id()], state, server_state);
        } else {
            track!(t, "already_left");

            already_left = true;
            configuration = Some(server_state.configuration.clone());
        }

        track!(t, "end");

        LeaveGroupResponse::new(
            self.context.local_server().id(),
            self.context.group_id(),
            true,
            server_state.leader_id,
            configuration,
            already_left,
        )
    }

    fn handle_leave_group_response(
        &self,
        leader_configuration: ServerConfiguration,
        response: LeaveGroupResponse,
        server_state: &mut ServerState,
    ) {
        track_method!(t, "handle_leave_group_response");

        let state = &mut self.state.borrow_mut();
        if !state.message_sender.handle_succeeded_response(
            leader_configuration,
            response.is_accepted(),
            response.leader_id(),
        ) {
            track!(t, "rejected_by_message_sender");

            return;
        }

        if !response.is_already_left()
            || response.configuration().is_none()
            || response
                .configuration()
                .as_ref()
                .unwrap()
                .servers()
                .iter()
                .find(|x| x.id() == self.context.local_server().id())
                .is_some()
        {
            track!(t, "leaving");

            return;
        }

        if server_state
            .configuration
            .servers()
            .iter()
            .find(|x| x.id() == self.context.local_server().id())
            .is_none()
        {
            track!(t, "already_left");

            return;
        }

        let configuration = response.configuration().as_ref().unwrap();

        let transaction = self.context.state_machine().begin_transaction(false);
        transaction.write_configuration(configuration.clone());
        transaction.commit();

        state
            .membership_protocol
            .reconfigure(configuration.clone(), server_state);

        track!(t, "end");
    }

    fn handle_leave_group_response_error(&self, leader_configuration: ServerConfiguration, error: Error) {
        track_method!(t, "handle_leave_group_response_error");

        let state = &mut self.state.borrow_mut();
        state.message_sender.handle_failed_response(leader_configuration, error);

        track!(t, "end");
    }
}

impl LeaveGroupProtocolImpl {
    fn request_leave_group(&self, state: &mut State, server_state: &mut ServerState) {
        track_method!(t, "request_leave_group");

        if state.leave_group_timer.attempt()
            > self
                .context
                .server_channel_factory_configuration()
                .leave_group_attempt_count()
        {
            track!(t, "fail_to_leave");

            state.leave_group_timer.stop();
            if let Some(completion_handler) = state.leave_group_completion_handler.as_ref() {
                let error = Box::new(ErrorKind::TimeoutOccurred);
                completion_handler.on_failed(error);
                state.leave_group_completion_handler = None;
            }

            return;
        }

        let request = LeaveGroupRequest::new(self.context.local_server().id(), self.context.group_id());

        state.message_sender.send(Request::LeaveGroup(request), server_state);

        track!(t, "end");
    }

    fn handle_auto_leave_group_timeout(&self, state: &mut State, server_state: &mut ServerState) {
        track_method!(t, "handle_auto_leave_group_timeout");

        if server_state.configuration_changing {
            track!(t, "configuration_changing");
            return;
        }

        let mut leaving_servers = vec![];
        let current_time = Time::now();
        for peer in &server_state.peers {
            if current_time
                >= peer.last_response_time()
                    + self
                        .context
                        .server_channel_factory_configuration()
                        .auto_leave_group_timeout()
            {
                leaving_servers.push(peer.configuration().id());
            }
        }

        if !leaving_servers.is_empty() {
            track!(t, "auto_leave");
            self.remove_servers_from_group(leaving_servers, state, server_state);
        }

        track!(t, "end");
    }

    fn remove_servers_from_group(&self, server_ids: Vec<Uuid>, state: &mut State, server_state: &mut ServerState) {
        track_method!(t, "remove_servers_from_group");

        let mut changed = false;
        let mut servers = server_state.configuration.servers().clone();
        for id in server_ids {
            let index = servers.iter().position(|x| x.id() == id);
            if let Some(index) = index {
                servers.remove(index);
                changed = true;
            }
        }

        if changed {
            let new_configuration = GroupConfiguration::new(
                server_state.configuration.name().clone(),
                server_state.configuration.id(),
                servers,
                server_state.configuration.is_single_server(),
            );

            self.context.log_store().append(LogEntry::new(
                server_state.persistent.term(),
                bincode::serialize(&new_configuration).unwrap(),
                LogValueType::Configuration,
                NULL_UUID,
                0,
            ));

            state.membership_protocol.reconfigure(new_configuration, server_state);
            server_state.configuration_changing = true;
            state.replication_protocol.request_append_entries(server_state);
        }

        track!(t, "end");
    }

    fn leave_group_follower(&self, completion_handler: Box<dyn CompletionHandler<(), Error>>, state: &mut State) {
        track_method!(t, "leave_group_follower");

        state.leave_group_completion_handler = Some(completion_handler);
        state.leave_group_timer.start();

        track!(t, "end");
    }
}
