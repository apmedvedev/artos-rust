use crate::artos::common::{GroupConfiguration, ServerConfiguration};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct StateTransferConfiguration {
    host: String,
    port: u16,
    start_log_index: u64,
}

impl StateTransferConfiguration {
    pub fn new(host: String, port: u16, start_log_index: u64) -> StateTransferConfiguration {
        StateTransferConfiguration {
            host,
            port,
            start_log_index,
        }
    }

    pub fn host(&self) -> &String {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn start_log_index(&self) -> u64 {
        self.start_log_index
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, Copy)]
pub enum LogValueType {
    Application,
    Configuration,
}

pub const NULL_UUID: Uuid = Uuid::nil();

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LogEntry {
    term: u64,
    value: Arc<Vec<u8>>,
    value_type: LogValueType,
    client_id: Uuid,
    message_id: u64,
}

impl LogEntry {
    pub fn new(term: u64, value: Vec<u8>, value_type: LogValueType, client_id: Uuid, message_id: u64) -> LogEntry {
        LogEntry {
            term,
            value: Arc::new(value),
            value_type,
            client_id,
            message_id,
        }
    }

    pub fn term(&self) -> u64 {
        self.term
    }

    pub fn value(&self) -> &Arc<Vec<u8>> {
        &self.value
    }

    pub fn value_type(&self) -> &LogValueType {
        &self.value_type
    }

    pub fn client_id(&self) -> Uuid {
        self.client_id
    }

    pub fn message_id(&self) -> u64 {
        self.message_id
    }

    pub fn to_entry(self, term: u64, client_id: Uuid) -> LogEntry {
        LogEntry {
            term,
            client_id,
            value_type: self.value_type,
            value: self.value,
            message_id: self.message_id,
        }
    }

    pub fn from_entry(entry: &super::client::LogEntry, term: u64, client_id: Uuid) -> LogEntry {
        LogEntry {
            term,
            client_id,
            value_type: LogValueType::Application,
            value: entry.value().clone(),
            message_id: entry.message_id(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub struct ClientSessionTransientState {
    client_id: Uuid,
    last_committed_message_id: u64,
}

impl ClientSessionTransientState {
    pub fn new(client_id: Uuid, last_committed_message_id: u64) -> ClientSessionTransientState {
        ClientSessionTransientState {
            client_id,
            last_committed_message_id,
        }
    }

    pub fn client_id(&self) -> Uuid {
        self.client_id
    }

    pub fn last_committed_message_id(&self) -> u64 {
        self.last_committed_message_id
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerTransientState {
    client_sessions: Vec<ClientSessionTransientState>,
    commit_index: u64,
}

impl ServerTransientState {
    pub fn new(client_sessions: Vec<ClientSessionTransientState>, commit_index: u64) -> ServerTransientState {
        ServerTransientState {
            client_sessions,
            commit_index,
        }
    }

    pub fn client_sessions(&self) -> &Vec<ClientSessionTransientState> {
        &self.client_sessions
    }

    pub fn commit_index(&self) -> u64 {
        self.commit_index
    }
}

//////////////////////////////////////////////////////////
// VoteRequest
//////////////////////////////////////////////////////////

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub struct VoteRequest {
    server_id: Uuid,
    group_id: Uuid,
    term: u64,
    last_log_term: u64,
    last_log_index: u64,
    pre_vote: bool,
}

impl VoteRequest {
    pub fn new(
        server_id: Uuid,
        group_id: Uuid,
        term: u64,
        last_log_term: u64,
        last_log_index: u64,
        pre_vote: bool,
    ) -> VoteRequest {
        VoteRequest {
            server_id,
            group_id,
            term,
            last_log_term,
            last_log_index,
            pre_vote,
        }
    }

    pub fn server_id(&self) -> Uuid {
        self.server_id
    }

    pub fn group_id(&self) -> Uuid {
        self.group_id
    }

    pub fn term(&self) -> u64 {
        self.term
    }

    pub fn last_log_term(&self) -> u64 {
        self.last_log_term
    }

    pub fn last_log_index(&self) -> u64 {
        self.last_log_index
    }

    pub fn is_pre_vote(&self) -> bool {
        self.pre_vote
    }
}

//////////////////////////////////////////////////////////
// VoteResponse
//////////////////////////////////////////////////////////

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VoteResponse {
    server_id: Uuid,
    group_id: Uuid,
    accepted: bool,
    term: u64,
    transient_state: Option<ServerTransientState>,
}

impl VoteResponse {
    pub fn new(
        server_id: Uuid,
        group_id: Uuid,
        accepted: bool,
        term: u64,
        transient_state: Option<ServerTransientState>,
    ) -> VoteResponse {
        VoteResponse {
            server_id,
            group_id,
            accepted,
            term,
            transient_state,
        }
    }

    pub fn server_id(&self) -> Uuid {
        self.server_id
    }

    pub fn group_id(&self) -> Uuid {
        self.group_id
    }

    pub fn is_accepted(&self) -> bool {
        self.accepted
    }

    pub fn term(&self) -> u64 {
        self.term
    }

    pub fn transient_state(&self) -> &Option<ServerTransientState> {
        &self.transient_state
    }
}

//////////////////////////////////////////////////////////
// AppendEntriesRequest
//////////////////////////////////////////////////////////

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum StateTransferType {
    Snapshot,
    Log,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AppendEntriesRequest {
    server_id: Uuid,
    group_id: Uuid,
    term: u64,
    last_log_term: u64,
    last_log_index: u64,
    commit_index: u64,
    entries: Vec<LogEntry>,
    state_transfer_request: Option<StateTransferType>,
}

impl AppendEntriesRequest {
    pub fn new(
        server_id: Uuid,
        group_id: Uuid,
        term: u64,
        last_log_term: u64,
        last_log_index: u64,
        commit_index: u64,
        entries: Vec<LogEntry>,
        state_transfer_request: Option<StateTransferType>,
    ) -> AppendEntriesRequest {
        AppendEntriesRequest {
            server_id,
            group_id,
            term,
            last_log_term,
            last_log_index,
            commit_index,
            entries,
            state_transfer_request,
        }
    }

    pub fn server_id(&self) -> Uuid {
        self.server_id
    }

    pub fn group_id(&self) -> Uuid {
        self.group_id
    }

    pub fn term(&self) -> u64 {
        self.term
    }

    pub fn last_log_term(&self) -> u64 {
        self.last_log_term
    }

    pub fn last_log_index(&self) -> u64 {
        self.last_log_index
    }

    pub fn commit_index(&self) -> u64 {
        self.commit_index
    }

    pub fn entries(&self) -> &Vec<LogEntry> {
        &self.entries
    }

    pub fn state_transfer_request(&self) -> Option<StateTransferType> {
        self.state_transfer_request
    }
}

//////////////////////////////////////////////////////////
// AppendEntriesResponse
//////////////////////////////////////////////////////////

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct AppendEntriesResponse {
    server_id: Uuid,
    group_id: Uuid,
    accepted: bool,
    term: u64,
    next_index: u64,
    state_transfer_required: bool,
}

impl AppendEntriesResponse {
    pub fn new(
        server_id: Uuid,
        group_id: Uuid,
        accepted: bool,
        term: u64,
        next_index: u64,
        state_transfer_required: bool,
    ) -> AppendEntriesResponse {
        AppendEntriesResponse {
            server_id,
            group_id,
            accepted,
            term,
            next_index,
            state_transfer_required,
        }
    }

    pub fn server_id(&self) -> Uuid {
        self.server_id
    }

    pub fn group_id(&self) -> Uuid {
        self.group_id
    }

    pub fn is_accepted(&self) -> bool {
        self.accepted
    }

    pub fn term(&self) -> u64 {
        self.term
    }

    pub fn next_index(&self) -> u64 {
        self.next_index
    }

    pub fn state_transfer_required(&self) -> bool {
        self.state_transfer_required
    }
}

//////////////////////////////////////////////////////////
// JoinGroupRequest
//////////////////////////////////////////////////////////

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct JoinGroupRequest {
    server_id: Uuid,
    group_id: Uuid,
    last_log_term: u64,
    last_log_index: u64,
    configuration: ServerConfiguration,
}

impl JoinGroupRequest {
    pub fn new(
        server_id: Uuid,
        group_id: Uuid,
        last_log_term: u64,
        last_log_index: u64,
        configuration: ServerConfiguration,
    ) -> JoinGroupRequest {
        JoinGroupRequest {
            server_id,
            group_id,
            last_log_term,
            last_log_index,
            configuration,
        }
    }

    pub fn server_id(&self) -> Uuid {
        self.server_id
    }

    pub fn group_id(&self) -> Uuid {
        self.group_id
    }

    pub fn last_log_term(&self) -> u64 {
        self.last_log_term
    }

    pub fn last_log_index(&self) -> u64 {
        self.last_log_index
    }

    pub fn configuration(&self) -> &ServerConfiguration {
        &self.configuration
    }
}

//////////////////////////////////////////////////////////
// JoinGroupResponse
//////////////////////////////////////////////////////////

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JoinGroupResponse {
    server_id: Uuid,
    group_id: Uuid,
    accepted: bool,
    leader_id: Option<Uuid>,
    state_transfer_configuration: Option<StateTransferConfiguration>,
    already_joined: bool,
    catching_up: bool,
    state_transfer_required: bool,
    rewind_repeat_join: bool,
}

impl JoinGroupResponse {
    pub fn new(
        server_id: Uuid,
        group_id: Uuid,
        accepted: bool,
        leader_id: Option<Uuid>,
        state_transfer_configuration: Option<StateTransferConfiguration>,
        already_joined: bool,
        catching_up: bool,
        state_transfer_required: bool,
        rewind_repeat_join: bool,
    ) -> JoinGroupResponse {
        JoinGroupResponse {
            server_id,
            group_id,
            accepted,
            leader_id,
            state_transfer_configuration,
            already_joined,
            catching_up,
            state_transfer_required,
            rewind_repeat_join,
        }
    }

    pub fn server_id(&self) -> Uuid {
        self.server_id
    }

    pub fn group_id(&self) -> Uuid {
        self.group_id
    }

    pub fn is_accepted(&self) -> bool {
        self.accepted
    }

    pub fn leader_id(&self) -> Option<Uuid> {
        self.leader_id
    }

    pub fn state_transfer_configuration(&self) -> &Option<StateTransferConfiguration> {
        &self.state_transfer_configuration
    }

    pub fn is_already_joined(&self) -> bool {
        self.already_joined
    }

    pub fn is_catching_up(&self) -> bool {
        self.catching_up
    }

    pub fn is_state_transfer_required(&self) -> bool {
        self.state_transfer_required
    }

    pub fn is_rewind_repeat_join(&self) -> bool {
        self.rewind_repeat_join
    }
}

//////////////////////////////////////////////////////////
// LeaveGroupRequest
//////////////////////////////////////////////////////////

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub struct LeaveGroupRequest {
    server_id: Uuid,
    group_id: Uuid,
}

impl LeaveGroupRequest {
    pub fn new(server_id: Uuid, group_id: Uuid) -> LeaveGroupRequest {
        LeaveGroupRequest { server_id, group_id }
    }

    pub fn server_id(&self) -> Uuid {
        self.server_id
    }

    pub fn group_id(&self) -> Uuid {
        self.group_id
    }
}

//////////////////////////////////////////////////////////
// LeaveGroupResponse
//////////////////////////////////////////////////////////

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LeaveGroupResponse {
    server_id: Uuid,
    group_id: Uuid,
    accepted: bool,
    leader_id: Option<Uuid>,
    configuration: Option<GroupConfiguration>,
    already_left: bool,
}

impl LeaveGroupResponse {
    pub fn new(
        server_id: Uuid,
        group_id: Uuid,
        accepted: bool,
        leader_id: Option<Uuid>,
        configuration: Option<GroupConfiguration>,
        already_left: bool,
    ) -> LeaveGroupResponse {
        LeaveGroupResponse {
            server_id,
            group_id,
            accepted,
            leader_id,
            configuration,
            already_left,
        }
    }

    pub fn server_id(&self) -> Uuid {
        self.server_id
    }

    pub fn group_id(&self) -> Uuid {
        self.group_id
    }

    pub fn is_accepted(&self) -> bool {
        self.accepted
    }

    pub fn leader_id(&self) -> Option<Uuid> {
        self.leader_id
    }

    pub fn configuration(&self) -> &Option<GroupConfiguration> {
        &self.configuration
    }

    pub fn is_already_left(&self) -> bool {
        self.already_left
    }
}
