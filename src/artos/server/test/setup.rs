#[cfg(test)]
pub mod tests {
    use crate::artos::common::{FlowController, MockMessageSenderConfiguration};
    use crate::artos::server::api::{
        MockLogStoreConfiguration, MockMessageListenerConfiguration, MockMessageListenerFactory, PersistentServerState,
        ServerChannelConfigurationBuilder, ServerChannelFactoryConfiguration,
    };

    use crate::artos::server::channel::{Context, MockServer, ServerPeer, ServerState};
    use crate::artos::server::test::{
        AppendEntriesRequestBuilder, AppendEntriesResponseBuilder, ConfigurationRequestBuilder,
        FlowControllerMockBuilder, GroupConfigurationBuilder, JoinGroupRequestBuilder, JoinGroupResponseBuilder,
        LeaveGroupRequestBuilder, LeaveGroupResponseBuilder, LogEntryBuilder, LogStoreMockBuilder,
        MessageSenderFactoryMockBuilder, PublishRequestBuilder, QueryRequestBuilder, RestartSessionRequestBuilder,
        SenderBuilder, ServerConfigurationBuilder, ServerMockBuilder, StateMachineMockBuilder,
        SubscriptionRequestBuilder, VoteRequestBuilder, VoteResponseBuilder,
    };
    use crate::common::utils::init_tracks;
    use crate::common::utils::Time;
    use std::rc::Rc;
    use std::time::Instant;

    //////////////////////////////////////////////////////////
    // ConfigurationSetup
    //////////////////////////////////////////////////////////

    pub struct ConfigurationSetup {
        pub group: GroupConfigurationBuilder,
        pub server: ServerConfigurationBuilder,
        pub channel_factory: ServerChannelFactoryConfiguration,
        pub channel: ServerChannelConfigurationBuilder,
    }

    impl ConfigurationSetup {
        pub fn new() -> ConfigurationSetup {
            let server = ServerConfigurationBuilder::new();
            let mut group = GroupConfigurationBuilder::new();
            group.set_servers(vec![
                server.build(),
                ServerConfigurationBuilder::new().build(),
                ServerConfigurationBuilder::new().build(),
            ]);
            let channel_factory = ServerChannelFactoryConfiguration::new();
            let channel = ServerChannelConfigurationBuilder::new()
                .set_server(server.build())
                .set_group(group.build())
                .set_log_store(Box::new(MockLogStoreConfiguration::new()))
                .set_message_listener(Box::new(MockMessageListenerConfiguration::new()))
                .set_message_sender(Box::new(MockMessageSenderConfiguration::new()))
                .set_work_dir(String::from("work_dir"));

            ConfigurationSetup {
                group,
                server,
                channel_factory,
                channel,
            }
        }
    }

    //////////////////////////////////////////////////////////
    // Setup
    //////////////////////////////////////////////////////////

    pub struct TestState {
        pub context: Rc<Context>,
        pub server: MockServer,
        pub server_state: ServerState,
        pub flow_controller: Rc<dyn FlowController>,
    }

    pub struct Setup {
        pub configuration: ConfigurationSetup,
        pub message_listener_factory: MockMessageListenerFactory,
        pub message_sender_factory: MessageSenderFactoryMockBuilder,
        pub log_store: LogStoreMockBuilder,
        pub state_machine: StateMachineMockBuilder,
        pub server: ServerMockBuilder,
        pub flow_controller: FlowControllerMockBuilder,
        pub log_entry: LogEntryBuilder,
        pub vote_request: VoteRequestBuilder,
        pub vote_response: VoteResponseBuilder,
        pub configuration_request: ConfigurationRequestBuilder,
        pub restart_session_request: RestartSessionRequestBuilder,
        pub publish_request: PublishRequestBuilder,
        pub query_request: QueryRequestBuilder,
        pub subscription_request: SubscriptionRequestBuilder,
        pub append_entries_request: AppendEntriesRequestBuilder,
        pub append_entries_response: AppendEntriesResponseBuilder,
        pub join_group_request: JoinGroupRequestBuilder,
        pub join_group_response: JoinGroupResponseBuilder,
        pub leave_group_request: LeaveGroupRequestBuilder,
        pub leave_group_response: LeaveGroupResponseBuilder,
        pub sender: SenderBuilder,
        pub persistent: PersistentServerState,
    }

    impl Setup {
        pub fn new() -> Setup {
            Self::init_time();
            init_tracks();

            let configuration = ConfigurationSetup::new();
            let message_listener_factory = MockMessageListenerFactory::new();
            let message_sender_factory = MessageSenderFactoryMockBuilder::new();
            let log_store = LogStoreMockBuilder::new();
            let state_machine = StateMachineMockBuilder::new();
            let server = ServerMockBuilder::new();
            let flow_controller = FlowControllerMockBuilder::new();
            let log_entry = LogEntryBuilder::new();

            let mut vote_request = VoteRequestBuilder::new();
            vote_request
                .set_group_id(configuration.group.id)
                .set_server_id(configuration.server.id);

            let mut vote_response = VoteResponseBuilder::new();
            vote_response
                .set_group_id(configuration.group.id)
                .set_server_id(configuration.server.id);

            let mut configuration_request = ConfigurationRequestBuilder::new();
            configuration_request.set_group_id(configuration.group.id);

            let mut restart_session_request = RestartSessionRequestBuilder::new();
            restart_session_request.set_group_id(configuration.group.id);

            let mut publish_request = PublishRequestBuilder::new();
            publish_request.set_group_id(configuration.group.id);

            let mut query_request = QueryRequestBuilder::new();
            query_request.set_group_id(configuration.group.id);

            let mut subscription_request = SubscriptionRequestBuilder::new();
            subscription_request.set_group_id(configuration.group.id);

            let mut append_entries_request = AppendEntriesRequestBuilder::new();
            append_entries_request
                .set_group_id(configuration.group.id)
                .set_server_id(configuration.server.id);

            let mut append_entries_response = AppendEntriesResponseBuilder::new();
            append_entries_response
                .set_group_id(configuration.group.id)
                .set_server_id(configuration.server.id);

            let mut join_group_request = JoinGroupRequestBuilder::new();
            join_group_request
                .set_group_id(configuration.group.id)
                .set_server_id(configuration.server.id);

            let mut join_group_response = JoinGroupResponseBuilder::new();
            join_group_response
                .set_group_id(configuration.group.id)
                .set_server_id(configuration.server.id);

            let mut leave_group_request = LeaveGroupRequestBuilder::new();
            leave_group_request
                .set_group_id(configuration.group.id)
                .set_server_id(configuration.server.id);

            let mut leave_group_response = LeaveGroupResponseBuilder::new();
            leave_group_response
                .set_group_id(configuration.group.id)
                .set_server_id(configuration.server.id);

            let sender = SenderBuilder::new();
            let persistent = PersistentServerState::new();

            Setup {
                configuration,
                message_listener_factory,
                message_sender_factory,
                log_store,
                state_machine,
                server,
                flow_controller,
                log_entry,
                vote_request,
                vote_response,
                configuration_request,
                restart_session_request,
                publish_request,
                query_request,
                subscription_request,
                append_entries_request,
                append_entries_response,
                join_group_request,
                join_group_response,
                leave_group_request,
                leave_group_response,
                sender,
                persistent,
            }
        }

        pub fn build(self) -> TestState {
            let group = self.configuration.group.build();
            let sender = self.sender.build().0;

            let context = Rc::new(Context::new(
                self.configuration.server.build(),
                self.configuration.group.build().id(),
                Box::new(self.message_listener_factory),
                self.message_sender_factory.build(),
                self.log_store.build(),
                self.state_machine.build(),
                self.configuration.channel_factory,
                self.configuration.channel.build(),
                self.sender.build().0,
            ));

            let mut server_state = ServerState::new(group);
            server_state.persistent = self.persistent;
            for configuration in server_state.configuration.servers() {
                if configuration.id() != context.local_server().id() {
                    let peer = ServerPeer::new(
                        String::from("endpoint"),
                        configuration.clone(),
                        context.clone(),
                        sender.clone(),
                    );
                    server_state.peers.push(peer);
                }
            }

            TestState {
                context,
                server: self.server.build(),
                server_state,
                flow_controller: self.flow_controller.build(),
            }
        }

        pub fn init_time() -> Instant {
            let time = Instant::now();
            Time::set(time);
            time
        }

        pub fn address<T>(value: &T) -> usize {
            value as *const T as usize
        }
    }
}
