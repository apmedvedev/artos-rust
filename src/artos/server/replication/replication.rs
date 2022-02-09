use crate::artos::common::utils::VecExt;
use crate::artos::common::GroupConfiguration;
use crate::artos::message::client::{ConfigurationRequest, PublishRequest, PublishResponse, RestartSessionRequest};
use crate::artos::message::server::{
    AppendEntriesRequest, AppendEntriesResponse, ClientSessionTransientState, LogEntry, LogValueType,
    ServerTransientState, StateTransferConfiguration, StateTransferType, NULL_UUID,
};
use crate::artos::message::Request;
use crate::artos::server::api::LogStore;
use crate::artos::server::channel::{Context, Server, ServerPeer, ServerRole, ServerState};
use crate::artos::server::election::{ElectionProtocol, NullElectionProtocol};
use crate::artos::server::membership::{MembershipProtocol, NullMembershipProtocol};
use crate::artos::server::replication::query::{NullQueryProtocol, QueryProtocol};
use crate::artos::server::replication::session::{ClientSession, ClientSessionManager};
use crate::artos::server::state::state_transfer::{NullStateTransferProtocol, StateTransferProtocol};
use crate::artos::server::state::{StateTransferCompletionHandler, StateTransferCompletionType};
use crate::artos::server::utils::update_term;
use crate::track;
use crate::track_method;
use log::debug;
use std::cell::{RefCell, RefMut};
use std::cmp::{max, min};
use std::rc::Rc;
use std::time::Instant;
use std::u64::MAX;
use uuid::Uuid;

include!("replication_tests.rs");

//////////////////////////////////////////////////////////
// ReplicationProtocol API
//////////////////////////////////////////////////////////

#[cfg_attr(test, mockall::automock)]
pub trait ReplicationProtocol {
    fn start(&self, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn stop(&self) {
        unimplemented!()
    }

    fn on_timer(&self, _current_time: Instant, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn handle_publish_request(&self, _request: PublishRequest, _server_state: &mut ServerState) -> PublishResponse {
        unimplemented!()
    }

    fn handle_configuration_request(
        &self,
        _request: ConfigurationRequest,
        _server_state: &mut ServerState,
    ) -> PublishResponse {
        unimplemented!()
    }

    fn handle_restart_session_request(
        &self,
        _request: RestartSessionRequest,
        _server_state: &mut ServerState,
    ) -> PublishResponse {
        unimplemented!()
    }

    fn handle_append_entries_request(
        &self,
        _request: AppendEntriesRequest,
        _server_state: &mut ServerState,
    ) -> AppendEntriesResponse {
        unimplemented!()
    }

    fn handle_append_entries_response(&self, _response: AppendEntriesResponse, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn client_session_manager<'a>(&'a self) -> RefMut<'a, ClientSessionManager> {
        unimplemented!()
    }

    fn on_leader(&self, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn on_follower(&self, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn on_left(&self) {
        unimplemented!()
    }

    fn on_state_transfer_completed(&self) {
        unimplemented!()
    }

    fn publish(&self, _log_entry: LogEntry, _server_state: &mut ServerState) -> u64 {
        unimplemented!()
    }

    fn last_committed_message_id(&self, _server_state: &mut ServerState) -> u64 {
        unimplemented!()
    }

    fn last_received_message_id(&self, _server_state: &mut ServerState) -> u64 {
        unimplemented!()
    }

    fn request_append_entries(&self, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn lock_commits(&self) {
        unimplemented!()
    }

    fn unlock_commits(&self) {
        unimplemented!()
    }

    fn acquire_transient_state(&self, _server_state: &mut ServerState) -> ServerTransientState {
        unimplemented!()
    }

    fn apply_transient_state(
        &self,
        _transient_states: Vec<(Uuid, ServerTransientState)>,
        _server_state: &mut ServerState,
    ) {
        unimplemented!();
    }
}

pub struct NullReplicationProtocol {}

impl NullReplicationProtocol {
    pub fn new() -> Rc<dyn ReplicationProtocol> {
        Rc::new(NullReplicationProtocol {})
    }
}

impl ReplicationProtocol for NullReplicationProtocol {}

//////////////////////////////////////////////////////////
// ReplicationProtocolImpl
//////////////////////////////////////////////////////////

pub struct ReplicationProtocolImpl {
    endpoint: String,
    server: Rc<dyn Server>,
    context: Rc<Context>,
    state: RefCell<State>,
}

struct State {
    client_session_manager: ClientSessionManager,
    election_protocol: Rc<dyn ElectionProtocol>,
    membership_protocol: Rc<dyn MembershipProtocol>,
    state_transfer_protocol: Rc<dyn StateTransferProtocol>,
    query_protocol: Rc<dyn QueryProtocol>,
    commits_locked: bool,
    state_transfer_in_progress: bool,
}

impl ReplicationProtocolImpl {
    fn new(server: Rc<dyn Server>, context: Rc<Context>) -> ReplicationProtocolImpl {
        let client_session_timeout = context.server_channel_factory_configuration().client_session_timeout();

        ReplicationProtocolImpl {
            endpoint: context.local_server().endpoint().clone(),
            server,
            context,
            state: RefCell::new(State {
                client_session_manager: ClientSessionManager::new(client_session_timeout),
                election_protocol: NullElectionProtocol::new(),
                membership_protocol: NullMembershipProtocol::new(),
                state_transfer_protocol: NullStateTransferProtocol::new(),
                query_protocol: NullQueryProtocol::new(),
                commits_locked: false,
                state_transfer_in_progress: false,
            }),
        }
    }

    pub fn set_membership_protocol(&self, value: Rc<dyn MembershipProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.membership_protocol = value;
    }

    pub fn set_election_protocol(&self, value: Rc<dyn ElectionProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.election_protocol = value;
    }

    pub fn set_state_transfer_protocol(&self, value: Rc<dyn StateTransferProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.state_transfer_protocol = value;
    }

    pub fn set_query_protocol(&self, value: Rc<dyn QueryProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.query_protocol = value;
    }
}

impl ReplicationProtocol for ReplicationProtocolImpl {
    fn start(&self, server_state: &mut ServerState) {
        track_method!(t, "start");

        let transaction = self.context.state_machine().begin_transaction(true);
        if let Some(persistent_state) = transaction.read_state() {
            track!(t, "persistent_state_read");

            server_state.persistent = persistent_state;
        }

        if let Some(configuration) = transaction.read_configuration() {
            track!(t, "configuration_read");

            server_state.configuration = configuration;
        }

        transaction.commit();

        assert_eq!(server_state.configuration.id(), self.context.group_id());

        if server_state
            .configuration
            .servers()
            .iter()
            .find(|v| v.id() == self.context.local_server().id())
            .is_none()
        {
            track!(t, "catching_up");

            server_state.catching_up = true;
        }

        let mut peers = vec![];
        for server_configuration in server_state.configuration.servers() {
            if server_configuration.id() != self.context.local_server().id() {
                peers.push(self.server.create_peer(server_configuration.clone()));
            }
        }

        std::mem::swap(&mut server_state.peers, &mut peers);

        server_state.quick_commit_index = server_state.persistent.commit_index();

        if self.context.log_store().end_index() - 1 < server_state.persistent.commit_index() {
            track!(t, "clear_to_commit_index");

            self.context.log_store().clear(server_state.persistent.commit_index());
        }

        track!(t, "end");
    }

    fn stop(&self) {}

    fn on_timer(&self, current_time: Instant, server_state: &mut ServerState) {
        track_method!(t, "on_timer");

        let state = &mut self.state.borrow_mut();

        if server_state.role == ServerRole::Leader {
            track!(t, "leader");

            let mut disconnected_count = 0;
            for peer in &server_state.peers {
                if peer.can_heartbeat(current_time) {
                    track!(t, "heartbeat");

                    self.request_append_entries_peer(&peer, state, server_state);
                } else if !peer.is_connected() {
                    track!(t, "disconnected");

                    disconnected_count += 1;
                }
            }

            if disconnected_count > (server_state.peers.len() + 1) / 2 {
                track!(t, "become_follower");

                state.election_protocol.become_follower(server_state);
            }
        }

        if let Some(sessions) = state.client_session_manager.check_timed_out_sessions(current_time) {
            track!(t, "remove_timed_out_sessions");

            for session in sessions {
                self.on_session_removed(&session, state);
            }
        }

        track!(t, "end");
    }

    fn handle_publish_request(&self, request: PublishRequest, server_state: &mut ServerState) -> PublishResponse {
        track_method!(t, "handle_publish_request");

        let state = &mut self.state.borrow_mut();

        if server_state.role != ServerRole::Leader {
            track!(t, "rejected");

            return PublishResponse::new(
                self.context.local_server().id(),
                false,
                server_state.leader_id,
                Some(server_state.configuration.clone()),
                false,
                0,
                0,
            );
        }

        let term = server_state.persistent.term();
        let session = state.client_session_manager.ensure_session(request.client_id());

        let log_entries = request.entries();
        let mut appended = false;
        if !log_entries.is_empty() {
            track!(t, "with_log_entries");

            for log_entry in log_entries {
                if session.receive(log_entry.message_id()) {
                    track!(t, "log_received");

                    self.context
                        .log_store()
                        .append(LogEntry::from_entry(&log_entry, term, request.client_id()));
                    appended = true;
                }
            }
        }

        let response = PublishResponse::new(
            self.context.local_server().id(),
            true,
            server_state.leader_id,
            None,
            session.is_out_of_order_received(),
            session.last_received_message_id(),
            session.last_committed_message_id(),
        );

        if appended {
            track!(t, "appended");

            self.request_append_entries_state(state, server_state);
        }

        track!(t, "end");

        response
    }

    fn handle_configuration_request(
        &self,
        request: ConfigurationRequest,
        server_state: &mut ServerState,
    ) -> PublishResponse {
        track_method!(t, "handle_configuration_request");

        let state = &mut self.state.borrow_mut();

        if server_state.role != ServerRole::Leader {
            track!(t, "rejected");

            return PublishResponse::new(
                self.context.local_server().id(),
                false,
                server_state.leader_id,
                Some(server_state.configuration.clone()),
                false,
                0,
                0,
            );
        }

        let session = state.client_session_manager.ensure_session(request.client_id());

        track!(t, "end");

        PublishResponse::new(
            self.context.local_server().id(),
            true,
            server_state.leader_id,
            Some(server_state.configuration.clone()),
            session.is_out_of_order_received(),
            session.last_received_message_id(),
            session.last_committed_message_id(),
        )
    }

    fn handle_restart_session_request(
        &self,
        request: RestartSessionRequest,
        server_state: &mut ServerState,
    ) -> PublishResponse {
        track_method!(t, "handle_restart_session_request");

        let state = &mut self.state.borrow_mut();

        if server_state.role != ServerRole::Leader {
            track!(t, "rejected");

            return PublishResponse::new(
                self.context.local_server().id(),
                false,
                server_state.leader_id,
                Some(server_state.configuration.clone()),
                false,
                0,
                0,
            );
        }

        let session = state.client_session_manager.ensure_session(request.client_id());
        session.set_last_received_message_id(request.last_sent_message_id());
        session.set_out_of_order_received(false);

        track!(t, "end");

        PublishResponse::new(
            self.context.local_server().id(),
            true,
            server_state.leader_id,
            None,
            session.is_out_of_order_received(),
            session.last_received_message_id(),
            session.last_committed_message_id(),
        )
    }

    fn handle_append_entries_request(
        &self,
        request: AppendEntriesRequest,
        server_state: &mut ServerState,
    ) -> AppendEntriesResponse {
        track_method!(t, "handle_append_entries_request");

        let state = &mut self.state.borrow_mut();

        if !server_state
            .configuration
            .servers()
            .exists(|v| v.id() == request.server_id())
        {
            track!(t, "rejected_by_unknown_server");

            return AppendEntriesResponse::new(
                self.context.local_server().id(),
                self.context.group_id(),
                false,
                0,
                0,
                false,
            );
        }

        if update_term(&self.context, server_state, request.term()) {
            track!(t, "term_updated");

            state.election_protocol.become_follower(server_state);
        }

        if request.term() == server_state.persistent.term() {
            if server_state.role == ServerRole::Candidate {
                track!(t, "become_follower");

                state.election_protocol.become_follower(server_state);
            } else {
                track!(t, "delay_election");

                assert_ne!(server_state.role, ServerRole::Leader);
                state.election_protocol.delay_election(server_state);
            }
        }

        let log_ok = request.last_log_index() == 0
            || (request.last_log_index() < self.context.log_store().end_index()
                && request.last_log_term()
                    == Self::term_for_log_index(self.context.log_store(), request.last_log_index()));

        let mut state_transfer = false;
        if request.state_transfer_request().is_some()
            && (request.state_transfer_request().unwrap() == StateTransferType::Snapshot
                || (request.state_transfer_request().unwrap() == StateTransferType::Log && log_ok))
        {
            track!(t, "state_transfer_required");

            assert_eq!(request.entries().len(), 1);
            let log_entry = &request.entries()[0];
            let state_transfer_configuration: StateTransferConfiguration =
                bincode::deserialize(&log_entry.value()).unwrap();

            let mut start_log_index = 1;
            if request.state_transfer_request().unwrap() == StateTransferType::Log {
                start_log_index = request.last_log_index() + 1;
            }

            if !state.state_transfer_in_progress {
                track!(t, "request_state_transfer");

                state.state_transfer_in_progress = true;
                state.state_transfer_protocol.request_state_transfer(
                    state_transfer_configuration.host(),
                    state_transfer_configuration.port(),
                    start_log_index,
                    StateTransferCompletionHandler::new(
                        StateTransferCompletionType::Replication,
                        self.context.sender().clone(),
                    ),
                );
            }

            state_transfer = true;
        }

        if request.term() < server_state.persistent.term()
            || !log_ok
            || state_transfer
            || state.state_transfer_in_progress
        {
            track!(t, "rejected_by_stale_log");

            return AppendEntriesResponse::new(
                self.context.local_server().id(),
                self.context.group_id(),
                false,
                server_state.persistent.term(),
                self.context.log_store().end_index(),
                state_transfer,
            );
        }

        if !request.entries().is_empty() {
            track!(t, "append_entries");

            self.append_entries(request.entries(), request.last_log_index() + 1);
        }

        server_state.leader_id = Some(request.server_id());

        self.commit(request.commit_index(), state, server_state);

        track!(t, "end");

        AppendEntriesResponse::new(
            self.context.local_server().id(),
            self.context.group_id(),
            true,
            server_state.persistent.term(),
            request.last_log_index() + request.entries().len() as u64 + 1,
            state_transfer,
        )
    }

    fn handle_append_entries_response(&self, response: AppendEntriesResponse, server_state: &mut ServerState) {
        track_method!(t, "handle_append_entries_response");

        if response.group_id() != server_state.configuration.id() {
            track!(t, "rejected_invalid_group");
            return;
        }

        let state = &mut self.state.borrow_mut();

        if server_state.role != ServerRole::Leader {
            track!(t, "rejected_not_leader");
            return;
        }

        let index = server_state
            .peers
            .iter()
            .position(|v| v.configuration().id() == response.server_id());
        if index.is_none() {
            track!(t, "rejected_by_unknown_server");
            return;
        }

        let index = index.unwrap();
        let peer = &server_state.peers[index];

        let rewind_step = peer.rewind_step();
        peer.set_rewind_step(1);

        let need_to_catchup: bool;
        if response.is_accepted() {
            track!(t, "response_accepted");

            peer.set_next_log_index(response.next_index());
            peer.set_matched_index(response.next_index() - 1);

            let mut matched_indexes = vec![self.context.log_store().end_index() - 1];
            for server_peer in &server_state.peers {
                matched_indexes.push(server_peer.matched_index());
            }

            matched_indexes.sort();
            matched_indexes.reverse();
            self.commit(matched_indexes[matched_indexes.len() / 2], state, server_state);

            need_to_catchup = response.next_index() < self.context.log_store().end_index();
        } else {
            if response.state_transfer_required() {
                track!(t, "response_rejected_by_state_transfer");

                peer.set_next_log_index(response.next_index());
                need_to_catchup = false;
            } else {
                track!(t, "response_rejected_by_stale_log");

                if response.next_index() < peer.next_log_index() {
                    peer.set_next_log_index(response.next_index());
                } else if peer.next_log_index() > rewind_step {
                    peer.set_next_log_index(peer.next_log_index() - rewind_step);
                    peer.set_rewind_step(rewind_step * 2);
                } else {
                    peer.set_next_log_index(1);
                }

                need_to_catchup = true;
            }
        }

        if need_to_catchup {
            track!(t, "need_to_catchup");

            let peer = &server_state.peers[index];
            self.request_append_entries_peer(peer, state, server_state);
        }

        track!(t, "end");
    }

    fn client_session_manager(&self) -> RefMut<ClientSessionManager> {
        let state = self.state.borrow_mut();
        RefMut::map(state, |v| &mut v.client_session_manager)
    }

    fn on_leader(&self, server_state: &mut ServerState) {
        track_method!(t, "on_leader");

        let state = &mut self.state.borrow_mut();
        assert_eq!(server_state.role, ServerRole::Leader);

        if server_state.quick_commit_index == 0 {
            track!(t, "add_initial_configuration");

            self.add_initial_configuration(server_state);
        }

        for peer in &mut server_state.peers {
            peer.set_next_log_index(self.context.log_store().end_index());
            peer.start();
        }

        let mut log_index = server_state.quick_commit_index + 1;
        while log_index < self.context.log_store().end_index() {
            let log_entry = self.context.log_store().get_at(log_index);
            if *log_entry.value_type() == LogValueType::Configuration {
                track!(t, "uncommitted_configuration_change");

                debug!(
                    "[{}] Detected an uncommitted configuration change at log index {}.",
                    self.endpoint, log_index
                );
                let configuration = bincode::deserialize(log_entry.value()).unwrap();
                state.membership_protocol.reconfigure(configuration, server_state);
                server_state.configuration_changing = true;
            }

            log_index += 1;
        }

        self.request_append_entries_state(state, server_state);

        track!(t, "end");
    }

    fn on_follower(&self, server_state: &mut ServerState) {
        track_method!(t, "on_follower");

        assert_eq!(server_state.role, ServerRole::Follower);

        for peer in &server_state.peers {
            peer.stop();
        }

        track!(t, "end");
    }

    fn on_left(&self) {
        track_method!(t, "on_left");

        let state = &mut self.state.borrow_mut();
        for session in state.client_session_manager.clear() {
            self.on_session_removed(&session, state);
        }

        track!(t, "end");
    }

    fn on_state_transfer_completed(&self) {
        let state = &mut self.state.borrow_mut();
        state.state_transfer_in_progress = false;
    }

    fn publish(&self, log_entry: LogEntry, server_state: &mut ServerState) -> u64 {
        track_method!(t, "publish");

        let mut state = self.state.borrow_mut();
        if server_state.role != ServerRole::Leader {
            track!(t, "not_leader");

            return 0;
        }

        let term = server_state.persistent.term();

        let session = state
            .client_session_manager
            .ensure_session(self.context.local_server().id());
        let received = session.receive(log_entry.message_id());
        assert!(received);

        self.context
            .log_store()
            .append(log_entry.to_entry(term, self.context.local_server().id()));

        self.request_append_entries_state(&mut state, server_state);

        track!(t, "end");

        self.last_committed_message_id_state(&mut state, server_state)
    }

    fn last_committed_message_id(&self, server_state: &mut ServerState) -> u64 {
        let state = &mut self.state.borrow_mut();
        self.last_committed_message_id_state(state, server_state)
    }

    fn last_received_message_id(&self, server_state: &mut ServerState) -> u64 {
        track_method!(t, "last_received_message_id");

        let state = &mut self.state.borrow_mut();
        if server_state.role != ServerRole::Leader {
            track!(t, "not_leader");

            return 0;
        }

        let session = state
            .client_session_manager
            .ensure_session(self.context.local_server().id());

        track!(t, "end");

        session.last_received_message_id()
    }

    fn request_append_entries(&self, server_state: &mut ServerState) {
        track_method!(t, "request_append_entries");

        let state = &mut self.state.borrow_mut();
        self.request_append_entries_state(state, server_state);

        track!(t, "end");
    }

    fn lock_commits(&self) {
        track_method!(t, "lock_commits");

        let state = &mut self.state.borrow_mut();
        state.commits_locked = true;

        track!(t, "end");
    }

    fn unlock_commits(&self) {
        track_method!(t, "unlock_commits");

        let state = &mut self.state.borrow_mut();
        state.commits_locked = false;

        track!(t, "end");
    }

    fn acquire_transient_state(&self, server_state: &mut ServerState) -> ServerTransientState {
        track_method!(t, "acquire_transient_state");

        let state = &mut self.state.borrow_mut();

        let mut client_sessions = vec![];
        for session in state.client_session_manager.sessions() {
            client_sessions.push(ClientSessionTransientState::new(
                session.client_id(),
                session.last_committed_message_id(),
            ));
        }

        track!(t, "end");

        ServerTransientState::new(client_sessions, server_state.quick_commit_index)
    }

    fn apply_transient_state(
        &self,
        transient_states: Vec<(Uuid, ServerTransientState)>,
        server_state: &mut ServerState,
    ) {
        track_method!(t, "apply_transient_state");

        let state = &mut self.state.borrow_mut();
        assert_eq!(server_state.role, ServerRole::Leader);

        let mut min_commit_index = server_state.quick_commit_index;
        for (_, transient_state) in &transient_states {
            if min_commit_index > transient_state.commit_index() {
                min_commit_index = transient_state.commit_index();
            }

            for client_session_state in transient_state.client_sessions() {
                let session = state
                    .client_session_manager
                    .ensure_session(client_session_state.client_id());
                if session.last_committed_message_id() < client_session_state.last_committed_message_id() {
                    track!(t, "update_session");

                    session.commit(client_session_state.last_committed_message_id());
                    session.set_committed(false);
                }
            }
        }

        let mut log_index = max(self.context.log_store().start_index(), min_commit_index + 1);
        while log_index < self.context.log_store().end_index() {
            let log_entry = self.context.log_store().get_at(log_index);
            if log_entry.client_id() != NULL_UUID {
                let session = state.client_session_manager.ensure_session(log_entry.client_id());
                session.set_last_received_message_id(log_entry.message_id());
            }

            log_index += 1;
        }

        track!(t, "end");
    }
}

impl ReplicationProtocolImpl {
    fn last_committed_message_id_state(&self, state: &mut State, server_state: &mut ServerState) -> u64 {
        track_method!(t, "last_committed_message_id");

        if server_state.role != ServerRole::Leader {
            track!(t, "not_leader");

            return 0;
        }

        let session = state
            .client_session_manager
            .ensure_session(self.context.local_server().id());

        track!(t, "end");

        session.last_committed_message_id()
    }

    fn request_append_entries_peer(&self, peer: &ServerPeer, state: &State, server_state: &ServerState) {
        track_method!(t, "request_append_entries_peer");

        if peer.is_connected() && !peer.is_request_in_progress() {
            track!(t, "send");

            self.server
                .send(peer, self.create_append_entries_request(peer, state, server_state));
        }

        track!(t, "end");
    }

    fn request_append_entries_state(&self, state: &mut State, server_state: &mut ServerState) {
        track_method!(t, "request_append_entries_state");

        if server_state.peers.is_empty() {
            track!(t, "single_server");

            self.commit(self.context.log_store().end_index() - 1, state, server_state);
            return;
        }

        for peer in &server_state.peers {
            self.request_append_entries_peer(peer, state, server_state);
        }

        track!(t, "end");
    }

    fn append_entries(&self, entries: &Vec<LogEntry>, start_log_index: u64) {
        track_method!(t, "append_entries");

        let mut log_index = start_log_index;
        let mut index = 0;
        while log_index < self.context.log_store().end_index()
            && index < entries.len()
            && entries[index].term() == self.context.log_store().get_at(log_index).term()
        {
            index += 1;
            log_index += 1;
        }

        let mut mappings = vec![];
        while log_index < self.context.log_store().end_index() && index < entries.len() {
            mappings.push((&entries[index], log_index));

            log_index += 1;
            index += 1;
        }

        for (entry, log_index) in mappings {
            self.context.log_store().set_at(log_index, entry.clone());
        }

        while index < entries.len() {
            self.context.log_store().append(entries[index].clone());

            index += 1;
        }

        track!(t, "end");
    }

    fn on_session_removed(&self, session: &ClientSession, state: &mut State) {
        track_method!(t, "on_session_removed");

        debug!(
            "[{}] Client session has been removed: {}.",
            self.endpoint,
            session.client_id()
        );

        state.query_protocol.on_session_removed(session);

        track!(t, "end");
    }

    fn add_initial_configuration(&self, server_state: &mut ServerState) {
        track_method!(t, "add_initial_configuration");

        self.context.log_store().append(LogEntry::new(
            server_state.persistent.term(),
            bincode::serialize(&server_state.configuration).unwrap(),
            LogValueType::Configuration,
            NULL_UUID,
            0,
        ));

        server_state.configuration_changing = true;

        debug!(
            "[{}] Initial configuration is added to log store: {:?}.",
            self.endpoint, server_state.configuration
        );

        track!(t, "end");
    }

    fn commit(&self, commit_index: u64, state: &mut State, server_state: &mut ServerState) {
        track_method!(t, "commit");

        if commit_index > server_state.quick_commit_index {
            server_state.quick_commit_index = commit_index;

            if server_state.role == ServerRole::Leader {
                track!(t, "leader");

                for peer in &server_state.peers {
                    self.request_append_entries_peer(&peer, state, server_state);
                }
            }
        }

        self.local_commit(state, server_state);

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

    fn create_append_entries_request(&self, peer: &ServerPeer, state: &State, server_state: &ServerState) -> Request {
        track_method!(t, "create_append_entries_request");

        assert_eq!(server_state.role, ServerRole::Leader);

        let start_log_index = self.context.log_store().start_index();
        let current_next_index = self.context.log_store().end_index();
        let commit_index = server_state.quick_commit_index;
        let term = server_state.persistent.term();

        let last_log_index = peer.next_log_index() - 1;

        assert!(last_log_index < current_next_index);

        let log_entries_unavailable = start_log_index > 1 && last_log_index < start_log_index;
        let mut log_entries = vec![];
        let last_log_term = Self::term_for_log_index(self.context.log_store(), last_log_index);
        let mut state_transfer_request = None;

        if log_entries_unavailable
            || (current_next_index - 1 - last_log_index
                >= self
                    .context
                    .server_channel_factory_configuration()
                    .min_state_transfer_gap_log_entry_count())
        {
            track!(t, "state_transfer_request");

            if log_entries_unavailable {
                state_transfer_request = Some(StateTransferType::Snapshot);
            } else {
                state_transfer_request = Some(StateTransferType::Log);
            }

            let host = state.state_transfer_protocol.host().clone();
            let port = state.state_transfer_protocol.port();

            let state_transfer_configuration = StateTransferConfiguration::new(host, port, 1);
            log_entries.push(LogEntry::new(
                0,
                bincode::serialize(&state_transfer_configuration).unwrap(),
                LogValueType::Application,
                NULL_UUID,
                0,
            ));
        } else {
            track!(t, "append_log_entries");

            assert_ne!(last_log_term, MAX);

            let end_log_index = min(
                current_next_index,
                last_log_index
                    + 1
                    + self
                        .context
                        .server_channel_factory_configuration()
                        .max_publishing_log_entry_count(),
            );

            if last_log_index + 1 < end_log_index {
                log_entries = self.context.log_store().get(last_log_index + 1, end_log_index);
            }
        }

        track!(t, "end");

        Request::AppendEntries(AppendEntriesRequest::new(
            self.context.local_server().id(),
            self.context.group_id(),
            term,
            last_log_term,
            last_log_index,
            commit_index,
            log_entries,
            state_transfer_request,
        ))
    }

    fn local_commit(&self, state: &mut State, server_state: &mut ServerState) {
        track_method!(t, "local_commit");

        if state.commits_locked {
            track!(t, "commit_locked");

            return;
        }

        let mut current_commit_index = server_state.persistent.commit_index();
        if current_commit_index >= server_state.quick_commit_index
            || current_commit_index >= self.context.log_store().end_index() - 1
        {
            track!(t, "commit_not_required");

            return;
        }

        let transaction = self.context.state_machine().begin_transaction(false);

        while current_commit_index < server_state.quick_commit_index
            && current_commit_index < self.context.log_store().end_index() - 1
        {
            current_commit_index += 1;

            let log_entry = self.context.log_store().get_at(current_commit_index);
            if *log_entry.value_type() == LogValueType::Application {
                if log_entry.client_id() != NULL_UUID {
                    let session = state.client_session_manager.ensure_session(log_entry.client_id());
                    session.commit(log_entry.message_id());
                }

                transaction.publish(current_commit_index, &log_entry.value());
            } else if *log_entry.value_type() == LogValueType::Configuration {
                let new_configuration: GroupConfiguration = bincode::deserialize(log_entry.value()).unwrap();

                debug!(
                    "[{}] Configuration is committed at log index {}: {:?}.",
                    self.endpoint, current_commit_index, new_configuration
                );

                state
                    .membership_protocol
                    .commit_configuration(new_configuration.clone(), server_state);
                transaction.write_configuration(new_configuration);
            }

            server_state.persistent.set_commit_index(current_commit_index);
        }

        transaction.write_state(server_state.persistent);
        transaction.commit();

        self.context.log_store().commit(current_commit_index);

        for session in state.client_session_manager.sessions_mut() {
            if session.is_committed() {
                state.query_protocol.on_commit(session);
                session.set_committed(false);
            }
        }

        track!(t, "end");
    }
}
