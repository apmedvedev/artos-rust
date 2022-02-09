use crate::artos::common::{Error, GroupConfiguration, ServerConfiguration};
use crate::artos::message::server::{
    JoinGroupRequest, JoinGroupResponse, LogEntry, LogValueType, StateTransferConfiguration, NULL_UUID,
};
use crate::artos::message::Request;
use crate::artos::server::api::LogStore;
use crate::artos::server::channel::{Context, ServerRole, ServerState};
use crate::artos::server::client::{FollowerMessageSender, NullFollowerMessageSender};
use crate::artos::server::membership::{MembershipProtocol, NullMembershipProtocol};
use crate::artos::server::replication::{NullReplicationProtocol, ReplicationProtocol};
use crate::artos::server::state::{
    NullStateTransferProtocol, StateTransferCompletionHandler, StateTransferCompletionType, StateTransferProtocol,
};
use crate::compartment::Timer;
use crate::track;
use crate::track_method;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use std::u64::MAX;

include!("join_tests.rs");

//////////////////////////////////////////////////////////
// JoinGroupProtocol API
//////////////////////////////////////////////////////////

#[cfg_attr(test, mockall::automock)]
pub trait JoinGroupProtocol {
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

    fn on_joined(&self) {
        unimplemented!()
    }

    fn on_state_transfer_completed(&self) {
        unimplemented!()
    }

    fn handle_join_group_request(
        &self,
        _request: JoinGroupRequest,
        _server_state: &mut ServerState,
    ) -> JoinGroupResponse {
        unimplemented!()
    }

    fn handle_join_group_response(
        &self,
        _leader_configuration: ServerConfiguration,
        _response: JoinGroupResponse,
        _server_state: &mut ServerState,
    ) {
        unimplemented!()
    }

    fn handle_join_group_response_error(&self, _leader_configuration: ServerConfiguration, _error: Error) {
        unimplemented!()
    }
}

pub struct NullJoinGroupProtocol {}

impl NullJoinGroupProtocol {
    pub fn new() -> Rc<dyn JoinGroupProtocol> {
        Rc::new(NullJoinGroupProtocol {})
    }
}

impl JoinGroupProtocol for NullJoinGroupProtocol {}

//////////////////////////////////////////////////////////
// SingleServerJoinGroupProtocolImpl
//////////////////////////////////////////////////////////

struct SingleServerJoinGroupProtocolImpl {}

impl SingleServerJoinGroupProtocolImpl {
    fn new() -> SingleServerJoinGroupProtocolImpl {
        SingleServerJoinGroupProtocolImpl {}
    }
}

impl JoinGroupProtocol for SingleServerJoinGroupProtocolImpl {
    fn start(&self) {}

    fn stop(&self) {}

    fn on_timer(&self, _current_time: Instant, _server_state: &mut ServerState) {}

    fn on_leader(&self) {}

    fn on_follower(&self) {}

    fn on_joined(&self) {}

    fn on_state_transfer_completed(&self) {}

    fn handle_join_group_request(
        &self,
        _request: JoinGroupRequest,
        _server_state: &mut ServerState,
    ) -> JoinGroupResponse {
        unimplemented!()
    }

    fn handle_join_group_response(
        &self,
        _leader_configuration: ServerConfiguration,
        _response: JoinGroupResponse,
        _server_state: &mut ServerState,
    ) {
        unimplemented!()
    }

    fn handle_join_group_response_error(&self, _leader_configuration: ServerConfiguration, _error: Error) {
        unimplemented!()
    }
}

//////////////////////////////////////////////////////////
// JoinGroupProtocolImpl
//////////////////////////////////////////////////////////

pub struct JoinGroupProtocolImpl {
    endpoint: String,
    context: Rc<Context>,
    state: RefCell<State>,
}

struct State {
    membership_protocol: Rc<dyn MembershipProtocol>,
    replication_protocol: Rc<dyn ReplicationProtocol>,
    state_transfer_protocol: Rc<dyn StateTransferProtocol>,
    message_sender: Rc<dyn FollowerMessageSender>,
    join_group_timer: Timer,
    rewind_step: u64,
}

impl JoinGroupProtocolImpl {
    pub fn new(context: Rc<Context>) -> JoinGroupProtocolImpl {
        JoinGroupProtocolImpl {
            endpoint: context.local_server().endpoint().clone(),
            context,
            state: RefCell::new(State {
                membership_protocol: NullMembershipProtocol::new(),
                replication_protocol: NullReplicationProtocol::new(),
                state_transfer_protocol: NullStateTransferProtocol::new(),
                message_sender: NullFollowerMessageSender::new(),
                join_group_timer: Timer::new(false, Duration::from_secs(0)),
                rewind_step: 1,
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

    pub fn set_state_transfer_protocol(&self, value: Rc<dyn StateTransferProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.state_transfer_protocol = value;
    }

    pub fn set_message_sender(&self, value: Rc<dyn FollowerMessageSender>) {
        let state = &mut self.state.borrow_mut();
        state.message_sender = value;
    }
}

impl JoinGroupProtocol for JoinGroupProtocolImpl {
    fn start(&self) {
        track_method!(t, "start");

        let state = &mut self.state.borrow_mut();
        state.message_sender.start();
        state
            .join_group_timer
            .set_period(self.context.server_channel_factory_configuration().join_group_period());
        state.join_group_timer.start();

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
        if state.join_group_timer.is_fired(current_time) {
            track!(t, "join_timer");

            self.request_join_group(state, server_state);
        }

        track!(t, "end");
    }

    fn on_leader(&self) {
        track_method!(t, "on_leader");

        let state = &mut self.state.borrow_mut();
        state.join_group_timer.stop();

        track!(t, "end");
    }

    fn on_follower(&self) {}

    fn on_joined(&self) {
        track_method!(t, "on_joined");

        let state = &mut self.state.borrow_mut();
        state.join_group_timer.stop();

        track!(t, "end");
    }

    fn on_state_transfer_completed(&self) {
        track_method!(t, "on_state_transfer_completed");

        let state = &mut self.state.borrow_mut();
        state.join_group_timer.start();

        track!(t, "end");
    }

    fn handle_join_group_request(
        &self,
        request: JoinGroupRequest,
        server_state: &mut ServerState,
    ) -> JoinGroupResponse {
        track_method!(t, "handle_join_group_request");

        let state = &mut self.state.borrow_mut();

        if server_state.role != ServerRole::Leader || server_state.configuration_changing {
            track!(t, "rejected");
            return JoinGroupResponse::new(
                self.context.local_server().id(),
                self.context.group_id(),
                false,
                server_state.leader_id,
                None,
                false,
                false,
                false,
                false,
            );
        }

        let mut already_joined = false;
        let mut catching_up = false;
        let mut state_transfer_required = false;
        let mut state_transfer_configuration = None;
        let mut rewind_repeat_join = false;
        if server_state
            .configuration
            .servers()
            .iter()
            .find(|x| x.id() == request.server_id())
            .is_some()
        {
            track!(t, "already_joined");

            already_joined = true;
        } else {
            track!(t, "new_server");

            let start_index = self.context.log_store().start_index();
            let current_next_index = self.context.log_store().end_index();
            let last_log_index = request.last_log_index();

            if start_index > 1 && last_log_index < start_index {
                track!(t, "log_not_available");

                catching_up = true;
                state_transfer_required = true;

                let host = state.state_transfer_protocol.host().clone();
                let port = state.state_transfer_protocol.port();
                let start_log_index = 1;

                state_transfer_configuration = Some(StateTransferConfiguration::new(host, port, start_log_index));
            } else {
                track!(t, "log_available");

                let log_ok = request.last_log_index() == 0
                    || (request.last_log_index() < self.context.log_store().end_index()
                        && request.last_log_term()
                            == Self::term_for_log_index(self.context.log_store(), request.last_log_index()));

                if log_ok {
                    track!(t, "log_ok");

                    if current_next_index - 1 - last_log_index
                        >= self
                            .context
                            .server_channel_factory_configuration()
                            .min_state_transfer_gap_log_entry_count()
                    {
                        track!(t, "log_outdated");

                        catching_up = true;
                        state_transfer_required = true;

                        let host = state.state_transfer_protocol.host().clone();
                        let port = state.state_transfer_protocol.port();
                        let start_log_index = last_log_index + 1;

                        state_transfer_configuration =
                            Some(StateTransferConfiguration::new(host, port, start_log_index));
                    } else {
                        track!(t, "accepted");

                        let mut servers = server_state.configuration.servers().clone();
                        servers.push(request.configuration().clone());

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

                        catching_up = true;
                    }
                } else {
                    track!(t, "log_stale");

                    catching_up = true;
                    rewind_repeat_join = true;
                }
            }
        }

        track!(t, "end");

        JoinGroupResponse::new(
            self.context.local_server().id(),
            self.context.group_id(),
            true,
            server_state.leader_id,
            state_transfer_configuration,
            already_joined,
            catching_up,
            state_transfer_required,
            rewind_repeat_join,
        )
    }

    fn handle_join_group_response(
        &self,
        leader_configuration: ServerConfiguration,
        response: JoinGroupResponse,
        server_state: &mut ServerState,
    ) {
        track_method!(t, "handle_join_group_response");

        let state = &mut self.state.borrow_mut();
        if !state.message_sender.handle_succeeded_response(
            leader_configuration,
            response.is_accepted(),
            response.leader_id(),
        ) {
            track!(t, "rejected_by_sender");
            return;
        }

        assert!(response.is_accepted());

        let rewind_step = state.rewind_step;
        state.rewind_step = 1;

        if response.is_already_joined() {
            track!(t, "already_joined");

            server_state.catching_up = false;
            state.join_group_timer.stop();
        } else {
            if response.is_catching_up() {
                track!(t, "catching_up");

                server_state.catching_up = true;
            }

            if response.is_state_transfer_required() && response.state_transfer_configuration().is_some() {
                track!(t, "state_transfer_required");

                state.join_group_timer.stop();
                let state_transfer_configuration = response.state_transfer_configuration().as_ref().unwrap();

                state.state_transfer_protocol.request_state_transfer(
                    state_transfer_configuration.host(),
                    state_transfer_configuration.port(),
                    state_transfer_configuration.start_log_index(),
                    StateTransferCompletionHandler::new(
                        StateTransferCompletionType::Join,
                        self.context.sender().clone(),
                    ),
                );
            }

            if response.is_rewind_repeat_join() {
                track!(t, "rewind_join");

                state.rewind_step = rewind_step * 2;
            }
        }

        track!(t, "end");
    }

    fn handle_join_group_response_error(&self, leader_configuration: ServerConfiguration, error: Error) {
        track_method!(t, "handle_join_group_response_error");

        let state = &mut self.state.borrow_mut();
        state.message_sender.handle_failed_response(leader_configuration, error);

        track!(t, "end");
    }
}

impl JoinGroupProtocolImpl {
    fn request_join_group(&self, state: &mut State, server_state: &mut ServerState) {
        track_method!(t, "request_join_group");

        let mut last_log_term = 0u64;
        let mut last_log_index = 0u64;

        let log_index = self.context.log_store().end_index() - state.rewind_step;
        if log_index >= self.context.log_store().start_index() {
            let log_entry = self.context.log_store().get_at(log_index);
            last_log_term = log_entry.term();
            last_log_index = log_index;
        }

        let request = JoinGroupRequest::new(
            self.context.local_server().id(),
            self.context.group_id(),
            last_log_term,
            last_log_index,
            self.context.local_server().clone(),
        );

        state.message_sender.send(Request::JoinGroup(request), server_state);

        track!(t, "end");
    }

    fn term_for_log_index(log_store: &Box<dyn LogStore>, log_index: u64) -> u64 {
        track_method!(t, "term_for_log_index");

        if log_index == 0 {
            track!(t, "log_index_not_set");

            return 0;
        }

        if log_index >= log_store.start_index() {
            track!(t, "end");

            return log_store.get_at(log_index).term();
        }

        track!(t, "log_index_max");

        MAX
    }
}
