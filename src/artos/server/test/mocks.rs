#[cfg(test)]
pub mod tests {
    use crate::artos::common::api::MockFlowController;
    use crate::artos::common::api::MockResponseHandler;
    use crate::artos::common::Result;
    use crate::artos::common::{GroupConfiguration, MessageSender};
    use crate::artos::message::server::{LogEntry, LogValueType, ServerTransientState, NULL_UUID};
    use crate::artos::server::api::MockMembershipListener;
    use crate::artos::server::api::MockStateMachine;
    use crate::artos::server::api::{MockLogStore, StateMachineTransaction};
    use crate::artos::server::api::{MockStateMachineTransaction, PersistentServerState};
    use crate::artos::server::channel::MockServer;
    use crate::artos::server::channel::ServerPeer;
    use crate::artos::server::client::MockFollowerMessageSender;
    use crate::artos::server::client::MockLeaderClientProtocol;
    use crate::artos::server::election::MockElectionProtocol;
    use crate::artos::server::membership::MockJoinGroupProtocol;
    use crate::artos::server::membership::MockLeaveGroupProtocol;
    use crate::artos::server::membership::MockMembershipProtocol;
    use crate::artos::server::replication::MockQueryProtocol;
    use crate::artos::server::replication::MockReplicationProtocol;
    use crate::artos::server::state::MockStateTransferProtocol;
    use crate::artos::server::utils::MockCompletionHandler;
    use mockall::predicate::{always, eq};
    use std::rc::Rc;

    use crate::artos::common::MockMessageSender;
    use crate::artos::common::MockMessageSenderFactory;
    use crate::artos::message::Request;
    use std::sync::Arc;

    //////////////////////////////////////////////////////////
    // Mocks
    //////////////////////////////////////////////////////////

    pub struct Mocks {}

    impl Mocks {
        pub fn log_store() -> LogStoreMockBuilder {
            LogStoreMockBuilder::new()
        }

        pub fn state_machine() -> StateMachineMockBuilder {
            StateMachineMockBuilder::new()
        }

        pub fn state_machine_transaction() -> StateMachineTransactionMockBuilder {
            StateMachineTransactionMockBuilder::new()
        }

        pub fn server() -> ServerMockBuilder {
            ServerMockBuilder::new()
        }

        pub fn membership_protocol() -> MembershipProtocolMockBuilder {
            MembershipProtocolMockBuilder::new()
        }

        pub fn replication_protocol() -> ReplicationProtocolMockBuilder {
            ReplicationProtocolMockBuilder::new()
        }

        pub fn query_protocol() -> QueryProtocolMockBuilder {
            QueryProtocolMockBuilder::new()
        }

        pub fn state_transfer_protocol() -> StateTransferProtocolMockBuilder {
            StateTransferProtocolMockBuilder::new()
        }

        pub fn election_protocol() -> ElectionProtocolMockBuilder {
            ElectionProtocolMockBuilder::new()
        }
    }

    //////////////////////////////////////////////////////////
    // LogStoreMockBuilder
    //////////////////////////////////////////////////////////

    pub struct LogStoreMockBuilder {
        mock: MockLogStore,
    }

    impl LogStoreMockBuilder {
        pub fn new() -> LogStoreMockBuilder {
            LogStoreMockBuilder {
                mock: MockLogStore::new(),
            }
        }

        pub fn start_index(&mut self, value: u64) -> &mut LogStoreMockBuilder {
            self.mock.expect_start_index().return_const(value);
            self
        }

        pub fn end_index(&mut self, value: u64) -> &mut LogStoreMockBuilder {
            self.mock.expect_end_index().return_const(value);
            self
        }

        pub fn get_at(&mut self, entries: Vec<(u64, LogEntry)>) -> &mut LogStoreMockBuilder {
            for entry in entries {
                self.mock.expect_get_at().with(eq(entry.0)).return_const_st(entry.1);
            }

            self
        }

        pub fn get(
            &mut self,
            start_log_index: u64,
            end_log_index: u64,
            entries: Vec<LogEntry>,
        ) -> &mut LogStoreMockBuilder {
            self.mock
                .expect_get()
                .with(eq(start_log_index), eq(end_log_index))
                .return_const_st(entries);
            self
        }

        pub fn last(&mut self, term: u64) -> &mut LogStoreMockBuilder {
            let entry = LogEntry::new(term, vec![], LogValueType::Application, NULL_UUID, 0);
            self.mock.expect_last().return_const_st(entry);
            self
        }

        pub fn set_at(&mut self, index: u64, entry: LogEntry) -> &mut LogStoreMockBuilder {
            self.mock
                .expect_set_at()
                .withf_st(move |x, y| *x == index && *y == entry)
                .times(1)
                .return_const_st(());
            self
        }

        pub fn append(&mut self, entry: LogEntry) -> &mut LogStoreMockBuilder {
            self.mock
                .expect_append()
                .withf_st(move |x| x == &entry)
                .times(1)
                .return_const(0u64);
            self
        }

        pub fn clear(&mut self, commit_index: u64) -> &mut LogStoreMockBuilder {
            self.mock
                .expect_clear()
                .with(eq(commit_index))
                .times(1)
                .return_const(());
            self
        }

        pub fn commit(&mut self, commit_index: u64) -> &mut LogStoreMockBuilder {
            self.mock
                .expect_commit()
                .with(eq(commit_index))
                .times(1)
                .return_const(());
            self
        }

        pub fn build(self) -> Box<MockLogStore> {
            Box::new(self.mock)
        }
    }

    //////////////////////////////////////////////////////////
    // StateMachineMockBuilder
    //////////////////////////////////////////////////////////

    pub struct StateMachineMockBuilder {
        mock: MockStateMachine,
    }

    impl StateMachineMockBuilder {
        pub fn new() -> StateMachineMockBuilder {
            StateMachineMockBuilder {
                mock: MockStateMachine::new(),
            }
        }

        pub fn begin_transaction(
            &mut self,
            read_only: bool,
            transaction: Box<dyn StateMachineTransaction>,
        ) -> &mut StateMachineMockBuilder {
            self.mock
                .expect_begin_transaction()
                .with(eq(read_only))
                .return_once_st(move |_| transaction);
            self
        }

        pub fn write_commit(&mut self) -> &mut StateMachineMockBuilder {
            self.mock
                .expect_begin_transaction()
                .with(eq(false))
                .returning_st(|_| Self::transaction_write_commit());
            self
        }

        pub fn build(self) -> Box<MockStateMachine> {
            Box::new(self.mock)
        }

        fn transaction_write_commit() -> Box<dyn StateMachineTransaction> {
            Mocks::state_machine_transaction().write_state().commit().build()
        }
    }

    //////////////////////////////////////////////////////////
    // StateMachineTransactionMockBuilder
    //////////////////////////////////////////////////////////

    pub struct StateMachineTransactionMockBuilder {
        mock: MockStateMachineTransaction,
    }

    impl StateMachineTransactionMockBuilder {
        pub fn new() -> StateMachineTransactionMockBuilder {
            StateMachineTransactionMockBuilder {
                mock: MockStateMachineTransaction::new(),
            }
        }

        pub fn read_state(mut self, state: Option<PersistentServerState>) -> StateMachineTransactionMockBuilder {
            self.mock.expect_read_state().times(1).return_const(state);
            self
        }

        pub fn read_configuration(
            mut self,
            configuration: Option<GroupConfiguration>,
        ) -> StateMachineTransactionMockBuilder {
            self.mock
                .expect_read_configuration()
                .times(1)
                .return_const(configuration);
            self
        }

        pub fn write_state(mut self) -> StateMachineTransactionMockBuilder {
            self.mock.expect_write_state().times(1).return_const(());
            self
        }

        pub fn write_configuration(mut self) -> StateMachineTransactionMockBuilder {
            self.mock.expect_write_configuration().times(1).return_const(());
            self
        }

        pub fn publish(mut self, count: usize) -> StateMachineTransactionMockBuilder {
            self.mock.expect_publish().times(count).return_const(());
            self
        }

        pub fn query(mut self) -> StateMachineTransactionMockBuilder {
            self.mock.expect_query().times(1).return_const(());
            self
        }

        pub fn subscribe(mut self, count: usize) -> StateMachineTransactionMockBuilder {
            self.mock.expect_subscribe().times(count).return_const(());
            self
        }

        pub fn unsubscribe(mut self) -> StateMachineTransactionMockBuilder {
            self.mock.expect_unsubscribe().times(1).return_const(());
            self
        }

        pub fn commit(mut self) -> StateMachineTransactionMockBuilder {
            self.mock.expect_commit().times(1).return_const(());
            self
        }

        pub fn build(self) -> Box<MockStateMachineTransaction> {
            Box::new(self.mock)
        }
    }

    //////////////////////////////////////////////////////////
    // MessageSenderFactoryMockBuilder
    //////////////////////////////////////////////////////////

    pub struct MessageSenderFactoryMockBuilder {
        mock: MockMessageSenderFactory,
    }

    impl MessageSenderFactoryMockBuilder {
        pub fn new() -> MessageSenderFactoryMockBuilder {
            MessageSenderFactoryMockBuilder {
                mock: MockMessageSenderFactory::new(),
            }
        }

        pub fn create_sender(
            &mut self,
            sender: Result<Box<dyn MessageSender>>,
        ) -> &mut MessageSenderFactoryMockBuilder {
            self.mock
                .expect_create_sender()
                .times(1)
                .return_once_st(move |_, _| sender);
            self
        }

        pub fn build(self) -> Box<MockMessageSenderFactory> {
            Box::new(self.mock)
        }
    }

    //////////////////////////////////////////////////////////
    // MessageSenderMockBuilder
    //////////////////////////////////////////////////////////

    pub struct MessageSenderMockBuilder {
        mock: MockMessageSender,
    }

    impl MessageSenderMockBuilder {
        pub fn new() -> MessageSenderMockBuilder {
            MessageSenderMockBuilder {
                mock: MockMessageSender::new(),
            }
        }

        pub fn send(mut self) -> MessageSenderMockBuilder {
            self.mock.expect_send().times(1).return_const(());
            self
        }

        pub fn build(self) -> Box<MockMessageSender> {
            Box::new(self.mock)
        }
    }

    //////////////////////////////////////////////////////////
    // ServerMockBuilder
    //////////////////////////////////////////////////////////

    pub struct ServerMockBuilder {
        mock: MockServer,
    }

    impl ServerMockBuilder {
        pub fn new() -> ServerMockBuilder {
            ServerMockBuilder {
                mock: MockServer::new(),
            }
        }

        pub fn create_peers(&mut self, peers: Vec<ServerPeer>) -> &mut ServerMockBuilder {
            Self::create_peers_mock(&mut self.mock, peers);
            self
        }

        pub fn send(&mut self) -> &mut ServerMockBuilder {
            self.mock.expect_send().return_const(());
            self
        }

        pub fn build(self) -> MockServer {
            self.mock
        }

        pub fn create_peers_mock(mock: &mut MockServer, peers: Vec<ServerPeer>) {
            for peer in peers {
                mock.expect_create_peer()
                    .with(eq(peer.configuration().clone()))
                    .times(1)
                    .return_once_st(|_| peer);
            }
        }
    }

    //////////////////////////////////////////////////////////
    // MembershipProtocolMockBuilder
    //////////////////////////////////////////////////////////

    pub struct MembershipProtocolMockBuilder {
        mock: MockMembershipProtocol,
    }

    impl MembershipProtocolMockBuilder {
        pub fn new() -> MembershipProtocolMockBuilder {
            MembershipProtocolMockBuilder {
                mock: MockMembershipProtocol::new(),
            }
        }

        pub fn on_follower(mut self) -> MembershipProtocolMockBuilder {
            self.mock.expect_on_follower().times(1).return_const(());
            self
        }

        pub fn on_leader(mut self) -> MembershipProtocolMockBuilder {
            self.mock.expect_on_leader().times(1).return_const(());
            self
        }

        pub fn commit_configuration(mut self, configuration: GroupConfiguration) -> MembershipProtocolMockBuilder {
            self.mock
                .expect_commit_configuration()
                .with(eq(configuration), always())
                .times(1)
                .return_const(());
            self
        }

        pub fn reconfigure(mut self, configuration: GroupConfiguration) -> MembershipProtocolMockBuilder {
            self.mock
                .expect_reconfigure()
                .with(eq(configuration), always())
                .times(1)
                .return_const(());
            self
        }

        pub fn build(self) -> Rc<MockMembershipProtocol> {
            Rc::new(self.mock)
        }
    }

    //////////////////////////////////////////////////////////
    // ReplicationProtocolMockBuilder
    //////////////////////////////////////////////////////////

    pub struct ReplicationProtocolMockBuilder {
        mock: MockReplicationProtocol,
    }

    impl ReplicationProtocolMockBuilder {
        pub fn new() -> ReplicationProtocolMockBuilder {
            ReplicationProtocolMockBuilder {
                mock: MockReplicationProtocol::new(),
            }
        }

        pub fn acquire_transient_state(mut self, commit_index: u64) -> ReplicationProtocolMockBuilder {
            let transient_state = ServerTransientState::new(vec![], commit_index);
            self.mock
                .expect_acquire_transient_state()
                .times(1)
                .return_const(transient_state);
            self
        }

        pub fn apply_transient_state(mut self) -> ReplicationProtocolMockBuilder {
            self.mock.expect_apply_transient_state().times(1).return_const(());
            self
        }

        pub fn on_leader(mut self) -> ReplicationProtocolMockBuilder {
            self.mock.expect_on_leader().times(1).return_const(());
            self
        }

        pub fn on_follower(mut self) -> ReplicationProtocolMockBuilder {
            self.mock.expect_on_follower().times(1).return_const(());
            self
        }

        pub fn on_left(mut self) -> ReplicationProtocolMockBuilder {
            self.mock.expect_on_left().times(1).return_const(());
            self
        }

        pub fn request_append_entries(mut self) -> ReplicationProtocolMockBuilder {
            self.mock.expect_request_append_entries().times(1).return_const(());
            self
        }

        pub fn last_committed_message_id(mut self, value: u64) -> ReplicationProtocolMockBuilder {
            self.mock
                .expect_last_committed_message_id()
                .times(1)
                .return_const(value);
            self
        }

        pub fn last_received_message_id(mut self, value: u64) -> ReplicationProtocolMockBuilder {
            self.mock.expect_last_received_message_id().times(1).return_const(value);
            self
        }

        pub fn publish(mut self, result: u64, count: usize) -> ReplicationProtocolMockBuilder {
            self.mock.expect_publish().times(count).return_const(result);
            self
        }

        pub fn build(self) -> Rc<MockReplicationProtocol> {
            Rc::new(self.mock)
        }
    }

    //////////////////////////////////////////////////////////
    // QueryProtocolMockBuiler
    //////////////////////////////////////////////////////////

    pub struct QueryProtocolMockBuilder {
        mock: MockQueryProtocol,
    }

    impl QueryProtocolMockBuilder {
        pub fn new() -> QueryProtocolMockBuilder {
            QueryProtocolMockBuilder {
                mock: MockQueryProtocol::new(),
            }
        }

        pub fn on_session_removed(mut self) -> QueryProtocolMockBuilder {
            self.mock.expect_on_session_removed().times(1..).return_const(());
            self
        }

        pub fn on_commit(mut self) -> QueryProtocolMockBuilder {
            self.mock.expect_on_commit().times(1..).return_const(());
            self
        }

        pub fn on_follower(mut self) -> QueryProtocolMockBuilder {
            self.mock.expect_on_follower().times(1).return_const(());
            self
        }

        pub fn build(self) -> Rc<MockQueryProtocol> {
            Rc::new(self.mock)
        }
    }

    //////////////////////////////////////////////////////////
    // ElectionProtocolMockBuilder
    //////////////////////////////////////////////////////////

    pub struct ElectionProtocolMockBuilder {
        mock: MockElectionProtocol,
    }

    impl ElectionProtocolMockBuilder {
        pub fn new() -> ElectionProtocolMockBuilder {
            ElectionProtocolMockBuilder {
                mock: MockElectionProtocol::new(),
            }
        }

        pub fn become_follower(mut self) -> ElectionProtocolMockBuilder {
            self.mock.expect_become_follower().times(1).return_const(());
            self
        }

        pub fn delay_election(mut self) -> ElectionProtocolMockBuilder {
            self.mock.expect_delay_election().times(1).return_const(());
            self
        }

        pub fn lock_election(mut self) -> ElectionProtocolMockBuilder {
            self.mock.expect_lock_election().times(1).return_const(());
            self
        }

        pub fn build(self) -> Rc<MockElectionProtocol> {
            Rc::new(self.mock)
        }
    }

    //////////////////////////////////////////////////////////
    // StateTransferProtocolMockBuilder
    //////////////////////////////////////////////////////////

    pub struct StateTransferProtocolMockBuilder {
        mock: MockStateTransferProtocol,
    }

    impl StateTransferProtocolMockBuilder {
        pub fn new() -> StateTransferProtocolMockBuilder {
            StateTransferProtocolMockBuilder {
                mock: MockStateTransferProtocol::new(),
            }
        }

        pub fn host(mut self, host: String) -> StateTransferProtocolMockBuilder {
            self.mock.expect_host().times(1).return_const(host);
            self
        }

        pub fn port(mut self, port: u16) -> StateTransferProtocolMockBuilder {
            self.mock.expect_port().times(1).return_const(port);
            self
        }

        pub fn request_state_transfer(
            mut self,
            host: String,
            port: u16,
            start_log_index: u64,
        ) -> StateTransferProtocolMockBuilder {
            self.mock
                .expect_request_state_transfer()
                .with(eq(host), eq(port), eq(start_log_index), always())
                .times(1)
                .return_const(());

            self
        }

        pub fn on_leader(mut self) -> StateTransferProtocolMockBuilder {
            self.mock.expect_on_leader().times(1).return_const(());
            self
        }

        pub fn on_follower(mut self) -> StateTransferProtocolMockBuilder {
            self.mock.expect_on_follower().times(1).return_const(());
            self
        }

        pub fn build(self) -> Rc<MockStateTransferProtocol> {
            Rc::new(self.mock)
        }
    }

    //////////////////////////////////////////////////////////
    // ResponseHandlerMockBuilder
    //////////////////////////////////////////////////////////

    pub struct ResponseHandlerMockBuilder {
        mock: MockResponseHandler,
    }

    impl ResponseHandlerMockBuilder {
        pub fn new() -> ResponseHandlerMockBuilder {
            ResponseHandlerMockBuilder {
                mock: MockResponseHandler::new(),
            }
        }

        pub fn on_succeeded(mut self) -> ResponseHandlerMockBuilder {
            self.mock.expect_on_succeeded().times(1).return_const(());
            self
        }

        pub fn on_failed(mut self) -> ResponseHandlerMockBuilder {
            self.mock.expect_on_failed().times(1).return_const(());
            self
        }

        pub fn build(self) -> Arc<MockResponseHandler> {
            Arc::new(self.mock)
        }
    }

    //////////////////////////////////////////////////////////
    // CompletionHandlerMockBuilder
    //////////////////////////////////////////////////////////

    pub struct CompletionHandlerMockBuilder<T: 'static, E: 'static> {
        mock: MockCompletionHandler<T, E>,
    }

    impl<T, E> CompletionHandlerMockBuilder<T, E> {
        pub fn new() -> CompletionHandlerMockBuilder<T, E> {
            CompletionHandlerMockBuilder {
                mock: MockCompletionHandler::new(),
            }
        }

        pub fn on_succeeded(mut self) -> CompletionHandlerMockBuilder<T, E> {
            self.mock.expect_on_succeeded().times(1).return_const(());
            self
        }

        pub fn on_failed(mut self) -> CompletionHandlerMockBuilder<T, E> {
            self.mock.expect_on_failed().times(1).return_const(());
            self
        }

        pub fn build(self) -> MockCompletionHandler<T, E> {
            self.mock
        }
    }

    //////////////////////////////////////////////////////////
    // LeaveGroupProtocolMockBuilder
    //////////////////////////////////////////////////////////

    pub struct LeaveGroupProtocolMockBuilder {
        mock: MockLeaveGroupProtocol,
    }

    impl LeaveGroupProtocolMockBuilder {
        pub fn new() -> LeaveGroupProtocolMockBuilder {
            LeaveGroupProtocolMockBuilder {
                mock: MockLeaveGroupProtocol::new(),
            }
        }

        pub fn on_follower(mut self) -> LeaveGroupProtocolMockBuilder {
            self.mock.expect_on_follower().times(1).return_const(());
            self
        }

        pub fn on_leader(mut self) -> LeaveGroupProtocolMockBuilder {
            self.mock.expect_on_leader().times(1).return_const(());
            self
        }

        pub fn on_left(mut self) -> LeaveGroupProtocolMockBuilder {
            self.mock.expect_on_left().times(1).return_const(());
            self
        }

        pub fn build(self) -> Rc<MockLeaveGroupProtocol> {
            Rc::new(self.mock)
        }
    }

    //////////////////////////////////////////////////////////
    // JoinGroupProtocolMockBuilder
    //////////////////////////////////////////////////////////

    pub struct JoinGroupProtocolMockBuilder {
        mock: MockJoinGroupProtocol,
    }

    impl JoinGroupProtocolMockBuilder {
        pub fn new() -> JoinGroupProtocolMockBuilder {
            JoinGroupProtocolMockBuilder {
                mock: MockJoinGroupProtocol::new(),
            }
        }

        pub fn on_follower(mut self) -> JoinGroupProtocolMockBuilder {
            self.mock.expect_on_follower().times(1).return_const(());
            self
        }

        pub fn on_leader(mut self) -> JoinGroupProtocolMockBuilder {
            self.mock.expect_on_leader().times(1).return_const(());
            self
        }

        pub fn on_joined(mut self) -> JoinGroupProtocolMockBuilder {
            self.mock.expect_on_joined().times(1).return_const(());
            self
        }

        pub fn build(self) -> Rc<MockJoinGroupProtocol> {
            Rc::new(self.mock)
        }
    }

    //////////////////////////////////////////////////////////
    // MembershipListenerMockBuilder
    //////////////////////////////////////////////////////////

    pub struct MembershipListenerMockBuilder {
        mock: MockMembershipListener,
    }

    impl MembershipListenerMockBuilder {
        pub fn new() -> MembershipListenerMockBuilder {
            MembershipListenerMockBuilder {
                mock: MockMembershipListener::new(),
            }
        }

        pub fn on_follower(mut self) -> MembershipListenerMockBuilder {
            self.mock.expect_on_follower().times(1).return_const(());
            self
        }

        pub fn on_leader(mut self) -> MembershipListenerMockBuilder {
            self.mock.expect_on_leader().times(1).return_const(());
            self
        }

        pub fn on_joined(mut self) -> MembershipListenerMockBuilder {
            self.mock.expect_on_joined().times(1).return_const(());
            self
        }

        pub fn on_left(mut self) -> MembershipListenerMockBuilder {
            self.mock.expect_on_left().times(1).return_const(());
            self
        }

        pub fn on_membership_changed(mut self) -> MembershipListenerMockBuilder {
            self.mock.expect_on_membership_changed().times(1).return_const(());
            self
        }

        pub fn build(self) -> Arc<MockMembershipListener> {
            Arc::new(self.mock)
        }
    }

    //////////////////////////////////////////////////////////
    // LeaderClientProtocolMockBuilder
    //////////////////////////////////////////////////////////

    pub struct LeaderClientProtocolMockBuilder {
        mock: MockLeaderClientProtocol,
    }

    impl LeaderClientProtocolMockBuilder {
        pub fn new() -> LeaderClientProtocolMockBuilder {
            LeaderClientProtocolMockBuilder {
                mock: MockLeaderClientProtocol::new(),
            }
        }

        pub fn on_follower(mut self) -> LeaderClientProtocolMockBuilder {
            self.mock.expect_on_follower().times(1).return_const(());
            self
        }

        pub fn on_leader(mut self) -> LeaderClientProtocolMockBuilder {
            self.mock.expect_on_leader().times(1).return_const(());
            self
        }

        pub fn build(self) -> Rc<MockLeaderClientProtocol> {
            Rc::new(self.mock)
        }
    }

    //////////////////////////////////////////////////////////
    // FollowerMessageSenderMockBuilder
    //////////////////////////////////////////////////////////

    pub struct FollowerMessageSenderMockBuilder {
        mock: MockFollowerMessageSender,
    }

    impl FollowerMessageSenderMockBuilder {
        pub fn new() -> FollowerMessageSenderMockBuilder {
            FollowerMessageSenderMockBuilder {
                mock: MockFollowerMessageSender::new(),
            }
        }

        pub fn start(mut self) -> FollowerMessageSenderMockBuilder {
            self.mock.expect_start().times(1).return_const(());
            self
        }

        pub fn stop(mut self) -> FollowerMessageSenderMockBuilder {
            self.mock.expect_stop().times(1).return_const(());
            self
        }

        pub fn handle_succeeded_response(mut self, result: bool) -> FollowerMessageSenderMockBuilder {
            self.mock
                .expect_handle_succeeded_response()
                .times(1)
                .return_const(result);
            self
        }

        pub fn handle_failed_response(mut self) -> FollowerMessageSenderMockBuilder {
            self.mock.expect_handle_failed_response().times(1).return_const(());
            self
        }

        pub fn send(mut self, request: Request) -> FollowerMessageSenderMockBuilder {
            self.mock
                .expect_send()
                .withf_st(move |x, _| x == &request)
                .times(1)
                .return_const(());
            self
        }

        pub fn build(self) -> Rc<MockFollowerMessageSender> {
            Rc::new(self.mock)
        }
    }

    //////////////////////////////////////////////////////////
    // FlowControllerMockBuilder
    //////////////////////////////////////////////////////////

    pub struct FlowControllerMockBuilder {
        mock: MockFlowController,
    }

    impl FlowControllerMockBuilder {
        pub fn new() -> FlowControllerMockBuilder {
            FlowControllerMockBuilder {
                mock: MockFlowController::new(),
            }
        }

        pub fn lock_flow(&mut self) -> &mut FlowControllerMockBuilder {
            self.mock.expect_lock_flow().times(1).return_const(());
            self
        }

        pub fn unlock_flow(&mut self) -> &mut FlowControllerMockBuilder {
            self.mock.expect_unlock_flow().times(1).return_const(());
            self
        }

        pub fn build(self) -> Rc<MockFlowController> {
            Rc::new(self.mock)
        }
    }
}
