#[cfg(test)]
mod tests {
    use super::*;
    use crate::artos::message::client::SubscriptionRequestItem;
    use crate::artos::server::channel::ServerState;
    use crate::artos::server::replication::ClientSessionManager;
    use crate::artos::server::test::{Mocks, ResponseHandlerMockBuilder, Setup};
    use crate::common::utils::expect_tracks;
    use pretty_assertions::assert_eq;
    use std::cell::RefMut;
    use std::time::Duration;

    #[test]
    fn test_start() {
        let (test, _) = new();

        test.start();

        let state = test.state.borrow();
        assert_eq!(state.subscription_timeout_timer.is_started(), true);
        expect_tracks(vec!["start.end"]);
    }

    #[test]
    fn test_stop() {
        let (test, _) = new();

        {
            let mut state = test.state.borrow_mut();
            state.subscription_timeout_timer.start();
        }

        test.stop();

        let state = test.state.borrow();
        assert_eq!(state.subscription_timeout_timer.is_started(), false);
        expect_tracks(vec!["stop.end"]);
    }

    #[test]
    fn test_on_timer() {
        let (test, _) = new();

        {
            let mut state = test.state.borrow_mut();
            state.subscription_timeout_timer.start();
        }

        test.set_replication_protocol(ReplicationProtocolMock::new());
        Time::advance(Duration::from_secs(1000));

        test.on_timer(Time::now());

        expect_tracks(vec!["handle_subscription_timeout.end", "on_timer.end"]);
    }

    #[test]
    fn test_on_commit() {
        let mut setup = Setup::new();

        setup
            .state_machine
            .begin_transaction(true, Mocks::state_machine_transaction().query().commit().build());

        let (test, _) = new_with_setup(setup);

        let replication_protocol = ReplicationProtocolMock::new();
        test.set_replication_protocol(replication_protocol.clone());

        let mut client_session_manager = replication_protocol.client_session_manager();
        let mut session = client_session_manager.ensure_session(Uuid::new_v4());

        let last_committed_message_id = 10;
        session.set_last_committed_message_id(10);
        session.add_query((
            QueryRequestItem::new(1, last_committed_message_id - 1, vec![]),
            ResponseHandlerMockBuilder::new().build(),
        ));
        session.add_query((
            QueryRequestItem::new(1, last_committed_message_id + 1, vec![]),
            ResponseHandlerMockBuilder::new().build(),
        ));

        test.on_commit(&mut session);

        assert_eq!(session.query_queue().len(), 1);
        expect_tracks(vec!["process_query.end", "on_commit.end"]);
    }

    #[test]
    fn test_on_follower() {
        let (test, _) = new();

        let replication_protocol = ReplicationProtocolMock::new();
        test.set_replication_protocol(replication_protocol.clone());

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        {
            let mut client_session_manager = replication_protocol.client_session_manager();

            let session1 = client_session_manager.ensure_session(id1);
            session1.add_query((
                QueryRequestItem::new(1, 1, vec![]),
                ResponseHandlerMockBuilder::new().build(),
            ));

            let session2 = client_session_manager.ensure_session(id2);
            session2.add_query((
                QueryRequestItem::new(1, 1, vec![]),
                ResponseHandlerMockBuilder::new().build(),
            ));
        }

        test.on_follower();

        let mut client_session_manager = replication_protocol.client_session_manager();
        let session1 = client_session_manager.ensure_session(id1);
        assert_eq!(session1.query_queue().is_empty(), true);

        let session2 = client_session_manager.ensure_session(id2);
        assert_eq!(session2.query_queue().is_empty(), true);

        expect_tracks(vec!["on_follower.end"]);
    }

    #[test]
    fn test_on_session_removed() {
        let mut setup = Setup::new();

        setup
            .state_machine
            .begin_transaction(true, Mocks::state_machine_transaction().unsubscribe().commit().build());

        let (test, _) = new_with_setup(setup);

        let replication_protocol = ReplicationProtocolMock::new();
        test.set_replication_protocol(replication_protocol.clone());

        let mut client_session_manager = replication_protocol.client_session_manager();
        let mut session = client_session_manager.ensure_session(Uuid::new_v4());

        session.set_subscriptions(vec![Uuid::new_v4()]);

        test.on_session_removed(&mut session);

        expect_tracks(vec!["process_subscriptions.end", "on_session_removed.end"]);
    }

    #[test]
    fn test_handle_query_request_empty() {
        let mut setup = Setup::new();

        let client_id = Uuid::new_v4();
        let request = setup.query_request.set_client_id(client_id).build();
        let (test, _) = new_with_setup(setup);

        let replication_protocol = ReplicationProtocolMock::new();
        test.set_replication_protocol(replication_protocol.clone());

        test.handle_query_request(request, ResponseHandlerMockBuilder::new().on_succeeded().build());

        let client_session_manager = replication_protocol.client_session_manager();
        assert_eq!(client_session_manager.has_session(client_id), true);

        expect_tracks(vec!["handle_query_request.empty_query", "handle_query_request.end"]);
    }

    #[test]
    fn test_handle_query_request_early() {
        let mut setup = Setup::new();

        let last_committed_message_id = 10;
        let client_id = Uuid::new_v4();
        let request = setup
            .query_request
            .set_client_id(client_id)
            .set_queries(vec![QueryRequestItem::new(1, last_committed_message_id + 1, vec![])])
            .build();
        let (test, _) = new_with_setup(setup);

        let replication_protocol = ReplicationProtocolMock::new();
        test.set_replication_protocol(replication_protocol.clone());

        {
            let mut client_session_manager = replication_protocol.client_session_manager();
            let session = client_session_manager.ensure_session(client_id);
            session.set_last_committed_message_id(last_committed_message_id);
        }

        test.handle_query_request(request, ResponseHandlerMockBuilder::new().build());

        let mut client_session_manager = replication_protocol.client_session_manager();
        let session = client_session_manager.ensure_session(client_id);
        assert_eq!(session.query_queue().len(), 1);

        expect_tracks(vec!["handle_query_request.early_query", "handle_query_request.end"]);
    }

    #[test]
    fn test_handle_query_request() {
        let mut setup = Setup::new();

        setup
            .state_machine
            .begin_transaction(true, Mocks::state_machine_transaction().query().commit().build());

        let last_committed_message_id = 10;
        let client_id = Uuid::new_v4();
        let request = setup
            .query_request
            .set_client_id(client_id)
            .set_queries(vec![QueryRequestItem::new(1, last_committed_message_id - 1, vec![])])
            .build();
        let (test, _) = new_with_setup(setup);

        let replication_protocol = ReplicationProtocolMock::new();
        test.set_replication_protocol(replication_protocol.clone());

        {
            let mut client_session_manager = replication_protocol.client_session_manager();
            let session = client_session_manager.ensure_session(client_id);
            session.set_last_committed_message_id(last_committed_message_id);
        }

        test.handle_query_request(request, ResponseHandlerMockBuilder::new().build());

        let mut client_session_manager = replication_protocol.client_session_manager();
        let session = client_session_manager.ensure_session(client_id);
        assert_eq!(session.query_queue().is_empty(), true);

        expect_tracks(vec!["process_query.end", "handle_query_request.end"]);
    }

    #[test]
    fn test_handle_subscription_request() {
        let mut setup = Setup::new();

        setup.state_machine.begin_transaction(
            true,
            Mocks::state_machine_transaction()
                .subscribe(2)
                .unsubscribe()
                .commit()
                .build(),
        );

        let client_id = Uuid::new_v4();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        let request = setup
            .subscription_request
            .set_client_id(client_id)
            .set_subscriptions(vec![
                SubscriptionRequestItem::new(id2, vec![]),
                SubscriptionRequestItem::new(id3, vec![]),
            ])
            .build();
        let (test, _) = new_with_setup(setup);

        let replication_protocol = ReplicationProtocolMock::new();
        test.set_replication_protocol(replication_protocol.clone());

        {
            let mut client_session_manager = replication_protocol.client_session_manager();
            let session = client_session_manager.ensure_session(client_id);
            session.set_subscriptions(vec![id1, id2]);
        }

        test.handle_subscription_request(request, ResponseHandlerMockBuilder::new().build());

        let mut client_session_manager = replication_protocol.client_session_manager();
        let session = client_session_manager.ensure_session(client_id);
        assert_eq!(session.subscriptions(), &vec![id2, id3]);

        expect_tracks(vec!["process_subscriptions.end", "handle_subscription_request.end"]);
    }

    #[test]
    fn test_process_query() {
        let mut setup = Setup::new();

        setup
            .state_machine
            .begin_transaction(true, Mocks::state_machine_transaction().query().commit().build());

        let (test, _) = new_with_setup(setup);

        let query = QueryRequestItem::new(1, 1, vec![]);
        let handler = ResponseHandlerMockBuilder::new().build();

        test.process_query(&query, handler);

        expect_tracks(vec!["process_query.end"]);
    }

    #[test]
    fn test_process_subscriptions() {
        let mut setup = Setup::new();

        let request = setup
            .subscription_request
            .set_subscriptions(vec![SubscriptionRequestItem::new(Uuid::new_v4(), vec![])])
            .build();
        setup.state_machine.begin_transaction(
            true,
            Mocks::state_machine_transaction()
                .subscribe(1)
                .unsubscribe()
                .commit()
                .build(),
        );

        let (test, _) = new_with_setup(setup);

        let handler = ResponseHandlerMockBuilder::new().build();

        test.process_subscriptions(Some(request), &vec![Uuid::new_v4()], Some(handler));

        expect_tracks(vec!["process_subscriptions.end"]);
    }

    #[test]
    fn test_handle_subscription_timeout() {
        let mut setup = Setup::new();

        setup
            .state_machine
            .begin_transaction(true, Mocks::state_machine_transaction().unsubscribe().commit().build());

        let (test, _) = new_with_setup(setup);

        let replication_protocol = ReplicationProtocolMock::new();
        test.set_replication_protocol(replication_protocol.clone());

        let client_id = Uuid::new_v4();
        {
            let mut client_session_manager = replication_protocol.client_session_manager();
            let session = client_session_manager.ensure_session(client_id);
            session.set_subscriptions(vec![Uuid::new_v4()]);
        }

        Time::advance(Duration::from_secs(600));

        let mut state = test.state.borrow_mut();
        test.handle_subscription_timeout(&mut state);

        let mut client_session_manager = replication_protocol.client_session_manager();
        let session = client_session_manager.ensure_session(client_id);
        assert_eq!(session.subscriptions().is_empty(), true);
        expect_tracks(vec![
            "handle_subscription_timeout.timed_out",
            "handle_subscription_timeout.end",
        ]);
    }

    fn new() -> (QueryProtocolImpl, ServerState) {
        new_with_setup(Setup::new())
    }

    fn new_with_setup(setup: Setup) -> (QueryProtocolImpl, ServerState) {
        let test_state = setup.build();
        (QueryProtocolImpl::new(test_state.context), test_state.server_state)
    }

    struct ReplicationProtocolMock {
        state: RefCell<State>,
    }

    struct State {
        client_session_manager: ClientSessionManager,
    }

    impl ReplicationProtocolMock {
        fn new() -> Rc<ReplicationProtocolMock> {
            Rc::new(ReplicationProtocolMock {
                state: RefCell::new(State {
                    client_session_manager: ClientSessionManager::new(Duration::from_secs(600)),
                }),
            })
        }
    }
    impl ReplicationProtocol for ReplicationProtocolMock {
        fn client_session_manager<'a>(&'a self) -> RefMut<'a, ClientSessionManager> {
            let state = self.state.borrow_mut();
            RefMut::map(state, |v| &mut v.client_session_manager)
        }
    }
}
