#[cfg(test)]
pub mod tests {
    use crate::artos::common::{GroupConfiguration, ServerConfiguration};
    use crate::artos::message::client;
    use crate::artos::message::client::{
        ConfigurationRequest, PublishRequest, QueryRequest, QueryRequestItem, RestartSessionRequest,
        SubscriptionRequest, SubscriptionRequestItem,
    };
    use crate::artos::message::server::{
        AppendEntriesRequest, AppendEntriesResponse, JoinGroupRequest, JoinGroupResponse, LeaveGroupRequest,
        LeaveGroupResponse, LogEntry, LogValueType, ServerTransientState, StateTransferConfiguration,
        StateTransferType, VoteRequest, VoteResponse, NULL_UUID,
    };
    use crate::artos::server::channel::Event;
    use crate::compartment::{Receiver, Sender};
    use std::time::Duration;
    use uuid::Uuid;

    //////////////////////////////////////////////////////////
    // ServerConfigurationBuilder
    //////////////////////////////////////////////////////////

    pub struct ServerConfigurationBuilder {
        pub endpoint: String,
        pub id: Uuid,
    }

    impl ServerConfigurationBuilder {
        pub fn new() -> ServerConfigurationBuilder {
            ServerConfigurationBuilder {
                endpoint: String::from("test-server"),
                id: Uuid::new_v4(),
            }
        }

        pub fn set_endpoint(&mut self, value: String) -> &mut ServerConfigurationBuilder {
            self.endpoint = value;
            self
        }

        pub fn set_id(&mut self, value: Uuid) -> &mut ServerConfigurationBuilder {
            self.id = value;
            self
        }

        pub fn build(&self) -> ServerConfiguration {
            ServerConfiguration::new(self.id, self.endpoint.clone())
        }
    }

    //////////////////////////////////////////////////////////
    // GroupConfigurationBuilder
    //////////////////////////////////////////////////////////

    #[derive(Clone)]
    pub struct GroupConfigurationBuilder {
        pub name: String,
        pub id: Uuid,
        pub servers: Vec<ServerConfiguration>,
        pub single_server: bool,
    }

    impl GroupConfigurationBuilder {
        pub fn new() -> GroupConfigurationBuilder {
            GroupConfigurationBuilder {
                name: String::from("test-group"),
                id: Uuid::new_v4(),
                servers: vec![ServerConfigurationBuilder::new().build()],
                single_server: false,
            }
        }

        pub fn set_name(&mut self, value: String) -> &mut GroupConfigurationBuilder {
            self.name = value;
            self
        }

        pub fn set_id(&mut self, value: Uuid) -> &mut GroupConfigurationBuilder {
            self.id = value;
            self
        }

        pub fn set_servers(&mut self, value: Vec<ServerConfiguration>) -> &mut GroupConfigurationBuilder {
            self.servers = value;
            self
        }

        pub fn add_server(&mut self, value: ServerConfiguration) -> &mut GroupConfigurationBuilder {
            self.servers.push(value);
            self
        }

        pub fn remove_server(&mut self, value: usize) -> &mut GroupConfigurationBuilder {
            self.servers.remove(value);
            self
        }

        pub fn set_single_server(&mut self, value: bool) -> &mut GroupConfigurationBuilder {
            self.single_server = value;
            self
        }

        pub fn build(&self) -> GroupConfiguration {
            GroupConfiguration::new(self.name.clone(), self.id, self.servers.clone(), self.single_server)
        }
    }

    //////////////////////////////////////////////////////////
    // LogEntryBuilder
    //////////////////////////////////////////////////////////

    pub struct LogEntryBuilder {
        pub term: u64,
        pub value: Vec<u8>,
        pub value_type: LogValueType,
        pub client_id: Uuid,
        pub message_id: u64,
    }

    impl LogEntryBuilder {
        pub fn new() -> LogEntryBuilder {
            LogEntryBuilder {
                term: 1,
                value: vec![],
                value_type: LogValueType::Application,
                client_id: Uuid::new_v4(),
                message_id: 0,
            }
        }

        pub fn set_term(&mut self, value: u64) -> &mut LogEntryBuilder {
            self.term = value;
            self
        }

        pub fn set_value(&mut self, value: Vec<u8>) -> &mut LogEntryBuilder {
            self.value = value;
            self
        }

        pub fn set_value_type(&mut self, value: LogValueType) -> &mut LogEntryBuilder {
            self.value_type = value;
            self
        }

        pub fn set_client_id(&mut self, value: Uuid) -> &mut LogEntryBuilder {
            self.client_id = value;
            self
        }

        pub fn set_message_id(&mut self, value: u64) -> &mut LogEntryBuilder {
            self.message_id = value;
            self
        }

        pub fn build(&self) -> LogEntry {
            LogEntry::new(
                self.term,
                self.value.clone(),
                self.value_type,
                self.client_id,
                self.message_id,
            )
        }
    }

    //////////////////////////////////////////////////////////
    // VoteRequestBuilder
    //////////////////////////////////////////////////////////

    pub struct VoteRequestBuilder {
        pub server_id: Uuid,
        pub group_id: Uuid,
        pub term: u64,
        pub last_log_term: u64,
        pub last_log_index: u64,
        pub pre_vote: bool,
    }

    impl VoteRequestBuilder {
        pub fn new() -> VoteRequestBuilder {
            VoteRequestBuilder {
                server_id: Uuid::new_v4(),
                group_id: Uuid::new_v4(),
                term: 1,
                last_log_term: 1,
                last_log_index: 1,
                pre_vote: false,
            }
        }

        pub fn set_server_id(&mut self, value: Uuid) -> &mut VoteRequestBuilder {
            self.server_id = value;
            self
        }

        pub fn set_group_id(&mut self, value: Uuid) -> &mut VoteRequestBuilder {
            self.group_id = value;
            self
        }

        pub fn set_term(&mut self, value: u64) -> &mut VoteRequestBuilder {
            self.term = value;
            self
        }

        pub fn set_last_log_term(&mut self, value: u64) -> &mut VoteRequestBuilder {
            self.last_log_term = value;
            self
        }

        pub fn set_last_log_index(&mut self, value: u64) -> &mut VoteRequestBuilder {
            self.last_log_index = value;
            self
        }

        pub fn set_pre_vote(&mut self, value: bool) -> &mut VoteRequestBuilder {
            self.pre_vote = value;
            self
        }

        pub fn build(&self) -> VoteRequest {
            VoteRequest::new(
                self.server_id,
                self.group_id,
                self.term,
                self.last_log_term,
                self.last_log_index,
                self.pre_vote,
            )
        }
    }

    //////////////////////////////////////////////////////////
    // VoteResponseBuilder
    //////////////////////////////////////////////////////////

    pub struct VoteResponseBuilder {
        pub server_id: Uuid,
        pub group_id: Uuid,
        pub accepted: bool,
        pub term: u64,
        transient_state: Option<ServerTransientState>,
    }

    impl VoteResponseBuilder {
        pub fn new() -> VoteResponseBuilder {
            VoteResponseBuilder {
                server_id: Uuid::new_v4(),
                group_id: Uuid::new_v4(),
                accepted: true,
                term: 1,
                transient_state: None,
            }
        }

        pub fn set_server_id(&mut self, value: Uuid) -> &mut VoteResponseBuilder {
            self.server_id = value;
            self
        }

        pub fn set_group_id(&mut self, value: Uuid) -> &mut VoteResponseBuilder {
            self.group_id = value;
            self
        }

        pub fn set_accepted(&mut self, value: bool) -> &mut VoteResponseBuilder {
            self.accepted = value;
            self
        }

        pub fn set_term(&mut self, value: u64) -> &mut VoteResponseBuilder {
            self.term = value;
            self
        }

        pub fn set_transient_state(&mut self, value: Option<ServerTransientState>) -> &mut VoteResponseBuilder {
            self.transient_state = value;
            self
        }

        pub fn build(&self) -> VoteResponse {
            VoteResponse::new(
                self.server_id,
                self.group_id,
                self.accepted,
                self.term,
                self.transient_state.clone(),
            )
        }
    }

    //////////////////////////////////////////////////////////
    // SenderBuilder
    //////////////////////////////////////////////////////////

    pub struct SenderBuilder {
        pub queue_size: usize,
        pub dispatch_period: Duration,
        pub thread_count: usize,
    }

    impl SenderBuilder {
        pub fn new() -> SenderBuilder {
            SenderBuilder {
                queue_size: 10,
                dispatch_period: Duration::from_millis(1),
                thread_count: num_cpus::get() * 2,
            }
        }

        pub fn set_queue_size(&mut self, value: usize) -> &mut SenderBuilder {
            self.queue_size = value;
            self
        }

        pub fn set_dispatch_period(&mut self, value: Duration) -> &mut SenderBuilder {
            self.dispatch_period = value;
            self
        }

        pub fn set_thread_count(&mut self, value: usize) -> &mut SenderBuilder {
            self.thread_count = value;
            self
        }

        pub fn build(&self) -> (Sender<Event>, Receiver<Event>) {
            Sender::new(self.queue_size, self.dispatch_period, self.thread_count)
        }
    }

    //////////////////////////////////////////////////////////
    // ConfigurationRequestBuilder
    //////////////////////////////////////////////////////////

    pub struct ConfigurationRequestBuilder {
        client_id: Uuid,
        group_id: Uuid,
    }

    impl ConfigurationRequestBuilder {
        pub fn new() -> ConfigurationRequestBuilder {
            ConfigurationRequestBuilder {
                client_id: NULL_UUID,
                group_id: NULL_UUID,
            }
        }

        pub fn set_client_id(&mut self, value: Uuid) -> &mut ConfigurationRequestBuilder {
            self.client_id = value;
            self
        }

        pub fn set_group_id(&mut self, value: Uuid) -> &mut ConfigurationRequestBuilder {
            self.group_id = value;
            self
        }

        pub fn build(&self) -> ConfigurationRequest {
            ConfigurationRequest::new(self.client_id, self.group_id)
        }
    }

    //////////////////////////////////////////////////////////
    // RestartSessionRequestBuilder
    //////////////////////////////////////////////////////////

    pub struct RestartSessionRequestBuilder {
        client_id: Uuid,
        group_id: Uuid,
        last_sent_message_id: u64,
    }

    impl RestartSessionRequestBuilder {
        pub fn new() -> RestartSessionRequestBuilder {
            RestartSessionRequestBuilder {
                client_id: NULL_UUID,
                group_id: NULL_UUID,
                last_sent_message_id: 0,
            }
        }

        pub fn set_client_id(&mut self, value: Uuid) -> &mut RestartSessionRequestBuilder {
            self.client_id = value;
            self
        }

        pub fn set_group_id(&mut self, value: Uuid) -> &mut RestartSessionRequestBuilder {
            self.group_id = value;
            self
        }

        pub fn set_last_sent_message_id(&mut self, value: u64) -> &mut RestartSessionRequestBuilder {
            self.last_sent_message_id = value;
            self
        }

        pub fn build(&self) -> RestartSessionRequest {
            RestartSessionRequest::new(self.client_id, self.group_id, self.last_sent_message_id)
        }
    }

    //////////////////////////////////////////////////////////
    // PublishRequestBuilder
    //////////////////////////////////////////////////////////

    pub struct PublishRequestBuilder {
        client_id: Uuid,
        group_id: Uuid,
        entries: Vec<client::LogEntry>,
    }

    impl PublishRequestBuilder {
        pub fn new() -> PublishRequestBuilder {
            PublishRequestBuilder {
                client_id: NULL_UUID,
                group_id: NULL_UUID,
                entries: vec![],
            }
        }

        pub fn set_client_id(&mut self, value: Uuid) -> &mut PublishRequestBuilder {
            self.client_id = value;
            self
        }

        pub fn set_group_id(&mut self, value: Uuid) -> &mut PublishRequestBuilder {
            self.group_id = value;
            self
        }

        pub fn set_entries(&mut self, value: Vec<client::LogEntry>) -> &mut PublishRequestBuilder {
            self.entries = value;
            self
        }

        pub fn build(&self) -> PublishRequest {
            PublishRequest::new(self.client_id, self.group_id, self.entries.clone())
        }
    }

    //////////////////////////////////////////////////////////
    // QueryRequestBuilder
    //////////////////////////////////////////////////////////

    pub struct QueryRequestBuilder {
        client_id: Uuid,
        group_id: Uuid,
        queries: Vec<QueryRequestItem>,
    }

    impl QueryRequestBuilder {
        pub fn new() -> QueryRequestBuilder {
            QueryRequestBuilder {
                client_id: NULL_UUID,
                group_id: NULL_UUID,
                queries: vec![],
            }
        }

        pub fn set_client_id(&mut self, value: Uuid) -> &mut QueryRequestBuilder {
            self.client_id = value;
            self
        }

        pub fn set_group_id(&mut self, value: Uuid) -> &mut QueryRequestBuilder {
            self.group_id = value;
            self
        }

        pub fn set_queries(&mut self, value: Vec<QueryRequestItem>) -> &mut QueryRequestBuilder {
            self.queries = value;
            self
        }

        pub fn build(&self) -> QueryRequest {
            QueryRequest::new(self.client_id, self.group_id, self.queries.clone())
        }
    }

    //////////////////////////////////////////////////////////
    // SubscriptionRequestBuilder
    //////////////////////////////////////////////////////////

    pub struct SubscriptionRequestBuilder {
        client_id: Uuid,
        group_id: Uuid,
        subscriptions: Vec<SubscriptionRequestItem>,
    }

    impl SubscriptionRequestBuilder {
        pub fn new() -> SubscriptionRequestBuilder {
            SubscriptionRequestBuilder {
                client_id: NULL_UUID,
                group_id: NULL_UUID,
                subscriptions: vec![],
            }
        }

        pub fn set_client_id(&mut self, value: Uuid) -> &mut SubscriptionRequestBuilder {
            self.client_id = value;
            self
        }

        pub fn set_group_id(&mut self, value: Uuid) -> &mut SubscriptionRequestBuilder {
            self.group_id = value;
            self
        }

        pub fn set_subscriptions(&mut self, value: Vec<SubscriptionRequestItem>) -> &mut SubscriptionRequestBuilder {
            self.subscriptions = value;
            self
        }

        pub fn build(&self) -> SubscriptionRequest {
            SubscriptionRequest::new(self.client_id, self.group_id, self.subscriptions.clone())
        }
    }

    //////////////////////////////////////////////////////////
    // AppendEntriesResponseBuilder
    //////////////////////////////////////////////////////////

    pub struct AppendEntriesResponseBuilder {
        pub server_id: Uuid,
        pub group_id: Uuid,
        pub accepted: bool,
        pub term: u64,
        pub next_index: u64,
        pub state_transfer_required: bool,
    }

    impl AppendEntriesResponseBuilder {
        pub fn new() -> AppendEntriesResponseBuilder {
            AppendEntriesResponseBuilder {
                server_id: Uuid::new_v4(),
                group_id: Uuid::new_v4(),
                accepted: true,
                term: 1,
                next_index: 0,
                state_transfer_required: false,
            }
        }

        pub fn set_server_id(&mut self, value: Uuid) -> &mut AppendEntriesResponseBuilder {
            self.server_id = value;
            self
        }

        pub fn set_group_id(&mut self, value: Uuid) -> &mut AppendEntriesResponseBuilder {
            self.group_id = value;
            self
        }

        pub fn set_accepted(&mut self, value: bool) -> &mut AppendEntriesResponseBuilder {
            self.accepted = value;
            self
        }

        pub fn set_term(&mut self, value: u64) -> &mut AppendEntriesResponseBuilder {
            self.term = value;
            self
        }

        pub fn set_next_index(&mut self, value: u64) -> &mut AppendEntriesResponseBuilder {
            self.next_index = value;
            self
        }

        pub fn set_state_transfer_required(&mut self, value: bool) -> &mut AppendEntriesResponseBuilder {
            self.state_transfer_required = value;
            self
        }

        pub fn build(&self) -> AppendEntriesResponse {
            AppendEntriesResponse::new(
                self.server_id,
                self.group_id,
                self.accepted,
                self.term,
                self.next_index,
                self.state_transfer_required,
            )
        }
    }

    //////////////////////////////////////////////////////////
    // AppendEntriesRequestBuilder
    //////////////////////////////////////////////////////////

    pub struct AppendEntriesRequestBuilder {
        pub server_id: Uuid,
        pub group_id: Uuid,
        pub term: u64,
        pub last_log_term: u64,
        pub last_log_index: u64,
        pub commit_index: u64,
        pub entries: Vec<LogEntry>,
        pub state_transfer_request: Option<StateTransferType>,
    }

    impl AppendEntriesRequestBuilder {
        pub fn new() -> AppendEntriesRequestBuilder {
            AppendEntriesRequestBuilder {
                server_id: Uuid::new_v4(),
                group_id: Uuid::new_v4(),
                term: 1,
                last_log_term: 1,
                last_log_index: 0,
                commit_index: 0,
                entries: vec![],
                state_transfer_request: None,
            }
        }

        pub fn set_server_id(&mut self, value: Uuid) -> &mut AppendEntriesRequestBuilder {
            self.server_id = value;
            self
        }

        pub fn set_group_id(&mut self, value: Uuid) -> &mut AppendEntriesRequestBuilder {
            self.group_id = value;
            self
        }

        pub fn set_term(&mut self, value: u64) -> &mut AppendEntriesRequestBuilder {
            self.term = value;
            self
        }

        pub fn set_last_log_term(&mut self, value: u64) -> &mut AppendEntriesRequestBuilder {
            self.last_log_term = value;
            self
        }

        pub fn set_last_log_index(&mut self, value: u64) -> &mut AppendEntriesRequestBuilder {
            self.last_log_index = value;
            self
        }

        pub fn set_commit_index(&mut self, value: u64) -> &mut AppendEntriesRequestBuilder {
            self.commit_index = value;
            self
        }

        pub fn set_entries(&mut self, value: Vec<LogEntry>) -> &mut AppendEntriesRequestBuilder {
            self.entries = value;
            self
        }

        pub fn set_state_transfer_request(
            &mut self,
            value: Option<StateTransferType>,
        ) -> &mut AppendEntriesRequestBuilder {
            self.state_transfer_request = value;
            self
        }

        pub fn build(&self) -> AppendEntriesRequest {
            AppendEntriesRequest::new(
                self.server_id,
                self.group_id,
                self.term,
                self.last_log_term,
                self.last_log_index,
                self.commit_index,
                self.entries.clone(),
                self.state_transfer_request,
            )
        }
    }

    //////////////////////////////////////////////////////////
    // JoinGroupRequestBuilder
    //////////////////////////////////////////////////////////

    pub struct JoinGroupRequestBuilder {
        pub server_id: Uuid,
        pub group_id: Uuid,
        pub last_log_term: u64,
        pub last_log_index: u64,
        pub configuration: ServerConfiguration,
    }

    impl JoinGroupRequestBuilder {
        pub fn new() -> JoinGroupRequestBuilder {
            JoinGroupRequestBuilder {
                server_id: Uuid::new_v4(),
                group_id: Uuid::new_v4(),
                last_log_term: 1,
                last_log_index: 1,
                configuration: ServerConfiguration::new(Uuid::new_v4(), String::from("test")),
            }
        }

        pub fn set_server_id(&mut self, value: Uuid) -> &mut JoinGroupRequestBuilder {
            self.server_id = value;
            self
        }

        pub fn set_group_id(&mut self, value: Uuid) -> &mut JoinGroupRequestBuilder {
            self.group_id = value;
            self
        }

        pub fn set_last_log_term(&mut self, value: u64) -> &mut JoinGroupRequestBuilder {
            self.last_log_term = value;
            self
        }

        pub fn set_last_log_index(&mut self, value: u64) -> &mut JoinGroupRequestBuilder {
            self.last_log_index = value;
            self
        }

        pub fn set_configuration(&mut self, value: ServerConfiguration) -> &mut JoinGroupRequestBuilder {
            self.configuration = value;
            self
        }

        pub fn build(&self) -> JoinGroupRequest {
            JoinGroupRequest::new(
                self.server_id,
                self.group_id,
                self.last_log_term,
                self.last_log_index,
                self.configuration.clone(),
            )
        }
    }

    //////////////////////////////////////////////////////////
    // JoinGroupResponseBuilder
    //////////////////////////////////////////////////////////

    pub struct JoinGroupResponseBuilder {
        pub server_id: Uuid,
        pub group_id: Uuid,
        pub accepted: bool,
        pub leader_id: Option<Uuid>,
        pub state_transfer_configuration: Option<StateTransferConfiguration>,
        pub already_joined: bool,
        pub catching_up: bool,
        pub state_transfer_required: bool,
        pub rewind_repeat_join: bool,
    }

    impl JoinGroupResponseBuilder {
        pub fn new() -> JoinGroupResponseBuilder {
            JoinGroupResponseBuilder {
                server_id: Uuid::new_v4(),
                group_id: Uuid::new_v4(),
                accepted: true,
                leader_id: None,
                state_transfer_configuration: None,
                already_joined: false,
                catching_up: false,
                state_transfer_required: false,
                rewind_repeat_join: false,
            }
        }

        pub fn set_server_id(&mut self, value: Uuid) -> &mut JoinGroupResponseBuilder {
            self.server_id = value;
            self
        }

        pub fn set_group_id(&mut self, value: Uuid) -> &mut JoinGroupResponseBuilder {
            self.group_id = value;
            self
        }

        pub fn set_accepted(&mut self, value: bool) -> &mut JoinGroupResponseBuilder {
            self.accepted = value;
            self
        }

        pub fn set_leader_id(&mut self, value: Option<Uuid>) -> &mut JoinGroupResponseBuilder {
            self.leader_id = value;
            self
        }

        pub fn set_state_transfer_configuration(
            &mut self,
            value: Option<StateTransferConfiguration>,
        ) -> &mut JoinGroupResponseBuilder {
            self.state_transfer_configuration = value;
            self
        }

        pub fn set_already_joined(&mut self, value: bool) -> &mut JoinGroupResponseBuilder {
            self.already_joined = value;
            self
        }

        pub fn set_catching_up(&mut self, value: bool) -> &mut JoinGroupResponseBuilder {
            self.catching_up = value;
            self
        }

        pub fn set_state_transfer_required(&mut self, value: bool) -> &mut JoinGroupResponseBuilder {
            self.state_transfer_required = value;
            self
        }

        pub fn set_rewind_repeat_join(&mut self, value: bool) -> &mut JoinGroupResponseBuilder {
            self.rewind_repeat_join = value;
            self
        }

        pub fn build(&self) -> JoinGroupResponse {
            JoinGroupResponse::new(
                self.server_id,
                self.group_id,
                self.accepted,
                self.leader_id,
                self.state_transfer_configuration.clone(),
                self.already_joined,
                self.catching_up,
                self.state_transfer_required,
                self.rewind_repeat_join,
            )
        }
    }

    //////////////////////////////////////////////////////////
    // LeaveGroupRequestBuilder
    //////////////////////////////////////////////////////////

    pub struct LeaveGroupRequestBuilder {
        pub server_id: Uuid,
        pub group_id: Uuid,
    }

    impl LeaveGroupRequestBuilder {
        pub fn new() -> LeaveGroupRequestBuilder {
            LeaveGroupRequestBuilder {
                server_id: Uuid::new_v4(),
                group_id: Uuid::new_v4(),
            }
        }

        pub fn set_server_id(&mut self, value: Uuid) -> &mut LeaveGroupRequestBuilder {
            self.server_id = value;
            self
        }

        pub fn set_group_id(&mut self, value: Uuid) -> &mut LeaveGroupRequestBuilder {
            self.group_id = value;
            self
        }

        pub fn build(&self) -> LeaveGroupRequest {
            LeaveGroupRequest::new(self.server_id, self.group_id)
        }
    }

    //////////////////////////////////////////////////////////
    // LeaveGroupResponseBuilder
    //////////////////////////////////////////////////////////

    pub struct LeaveGroupResponseBuilder {
        pub server_id: Uuid,
        pub group_id: Uuid,
        pub accepted: bool,
        pub leader_id: Option<Uuid>,
        pub configuration: Option<GroupConfiguration>,
        pub already_left: bool,
    }

    impl LeaveGroupResponseBuilder {
        pub fn new() -> LeaveGroupResponseBuilder {
            LeaveGroupResponseBuilder {
                server_id: Uuid::new_v4(),
                group_id: Uuid::new_v4(),
                accepted: true,
                leader_id: None,
                configuration: None,
                already_left: false,
            }
        }

        pub fn set_server_id(&mut self, value: Uuid) -> &mut LeaveGroupResponseBuilder {
            self.server_id = value;
            self
        }

        pub fn set_group_id(&mut self, value: Uuid) -> &mut LeaveGroupResponseBuilder {
            self.group_id = value;
            self
        }

        pub fn set_accepted(&mut self, value: bool) -> &mut LeaveGroupResponseBuilder {
            self.accepted = value;
            self
        }

        pub fn set_leader_id(&mut self, value: Option<Uuid>) -> &mut LeaveGroupResponseBuilder {
            self.leader_id = value;
            self
        }

        pub fn set_configuration(&mut self, value: Option<GroupConfiguration>) -> &mut LeaveGroupResponseBuilder {
            self.configuration = value;
            self
        }

        pub fn set_already_left(&mut self, value: bool) -> &mut LeaveGroupResponseBuilder {
            self.already_left = value;
            self
        }

        pub fn build(&self) -> LeaveGroupResponse {
            LeaveGroupResponse::new(
                self.server_id,
                self.group_id,
                self.accepted,
                self.leader_id,
                self.configuration.clone(),
                self.already_left,
            )
        }
    }
}
