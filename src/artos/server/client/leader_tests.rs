#[cfg(test)]
mod tests {
    use super::*;
    use crate::artos::server::test::{CompletionHandlerMockBuilder, ReplicationProtocolMockBuilder, Setup};
    use crate::common::utils::expect_tracks;
    use crate::common::utils::init_tracks;
    use crate::common::utils::Time;
    use pretty_assertions::assert_eq;
    use std::time::Duration;

    #[test]
    fn test_new() {
        let (test, _) = new();

        let state = test.state.borrow();
        assert_eq!(
            state.commit_timer.period(),
            test.context.server_channel_factory_configuration().commit_period()
        );
        assert_eq!(state.next_message_id, 1);
        assert_eq!(state.flow_locked, false);
        assert_eq!(&test.endpoint, test.context.local_server().endpoint());
        assert_eq!(
            test.max_value_size,
            test.context.server_channel_factory_configuration().max_value_size()
        );
        assert_eq!(
            test.lock_queue_capacity,
            test.context
                .server_channel_factory_configuration()
                .lock_queue_capacity()
        );
        assert_eq!(
            test.unlock_queue_capacity,
            test.context
                .server_channel_factory_configuration()
                .unlock_queue_capacity()
        );
    }

    #[test]
    fn test_on_timer() {
        let mut setup = Setup::new();

        setup
            .configuration
            .channel_factory
            .set_commit_period(Duration::from_secs(1));

        let (test, mut server_state) = new_with_setup(setup);

        test.set_replication_protocol(
            ReplicationProtocolMockBuilder::new()
                .last_committed_message_id(0)
                .build(),
        );
        {
            let mut state = test.state.borrow_mut();
            state.commit_timer.start();
        }

        Time::advance(Duration::from_secs(10));

        test.on_timer(Time::now(), &mut server_state);

        expect_tracks(vec!["on_timer.commit_timer_fired", "handle_commit.end", "on_timer.end"]);
    }

    #[test]
    fn test_publish_max_value_exceeded() {
        let mut setup = Setup::new();

        setup.configuration.channel_factory.set_max_value_size(2);
        let handler = Rc::new(CompletionHandlerMockBuilder::new().on_failed().build());
        let (test, mut server_state) = new_with_setup_handler(setup, handler);

        test.publish(vec![1, 2, 3], 0, &mut server_state);

        expect_tracks(vec!["publish.max_value_exceeded"]);
    }

    #[test]
    fn test_publish_not_leader() {
        let setup = Setup::new();

        let handler = Rc::new(CompletionHandlerMockBuilder::new().on_failed().build());
        let (test, mut server_state) = new_with_setup_handler(setup, handler);

        test.publish(vec![], 0, &mut server_state);

        expect_tracks(vec!["publish.not_leader"]);
    }

    #[test]
    fn test_publish() {
        let mut setup = Setup::new();

        let entry = setup
            .log_entry
            .set_term(0)
            .set_client_id(NULL_UUID)
            .set_message_id(1)
            .build();

        let (test, mut server_state) = new_with_setup(setup);

        server_state.role = ServerRole::Leader;

        test.set_replication_protocol(ReplicationProtocolMockBuilder::new().publish(0, 1).build());

        test.publish(vec![], 0, &mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.next_message_id, 2);
        assert_eq!(state.queue.front().as_ref().unwrap().log_entry, entry);
        expect_tracks(vec!["check_flow.end", "publish.end"]);
    }

    #[test]
    fn test_publish_committed() {
        let setup = Setup::new();

        let handler = Rc::new(CompletionHandlerMockBuilder::new().on_succeeded().build());

        let (test, mut server_state) = new_with_setup_handler(setup, handler);

        test.set_replication_protocol(ReplicationProtocolMockBuilder::new().publish(1, 1).build());
        server_state.role = ServerRole::Leader;

        test.publish(vec![], 0, &mut server_state);

        expect_tracks(vec![
            "check_flow.end",
            "publish.remove_committed",
            "check_flow.end",
            "remove_committed_entries.end",
            "publish.end",
        ]);
    }

    #[test]
    fn test_on_leader() {
        let last_received_message_id = 10;

        let (test, mut server_state) = new();

        test.set_replication_protocol(
            ReplicationProtocolMockBuilder::new()
                .last_received_message_id(last_received_message_id)
                .build(),
        );

        test.on_leader(&mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.commit_timer.is_started(), true);
        assert_eq!(state.next_message_id, last_received_message_id + 1);
        expect_tracks(vec!["on_leader.end"]);
    }

    #[test]
    fn test_on_follower() {
        let (test, _) = new();

        {
            let mut state = test.state.borrow_mut();
            state.commit_timer.start();
        }

        test.on_follower();

        let state = test.state.borrow();
        assert_eq!(state.commit_timer.is_started(), false);
        expect_tracks(vec!["check_flow.end", "clear.end", "on_follower.end"]);
    }

    #[test]
    fn test_handle_commit() {
        let (test, mut server_state) = new();

        test.set_replication_protocol(
            ReplicationProtocolMockBuilder::new()
                .last_committed_message_id(1)
                .build(),
        );

        let mut state = test.state.borrow_mut();

        test.handle_commit(&mut state, &mut server_state);

        expect_tracks(vec![
            "handle_commit.remove_committed",
            "remove_committed_entries.end",
            "handle_commit.end",
        ]);
    }

    #[test]
    fn test_remove_committed_entries() {
        let setup = Setup::new();

        let handler = Rc::new(CompletionHandlerMockBuilder::new().on_succeeded().build());

        let (test, mut server_state) = new_with_setup_handler(setup, handler);

        test.set_replication_protocol(ReplicationProtocolMockBuilder::new().publish(0, 2).build());
        server_state.role = ServerRole::Leader;

        test.publish(vec![], 0, &mut server_state);
        test.publish(vec![], 0, &mut server_state);

        init_tracks();
        let mut state = test.state.borrow_mut();

        test.remove_committed_entries(1, &mut state);

        assert_eq!(state.queue.len(), 1);
        expect_tracks(vec!["check_flow.end", "remove_committed_entries.end"]);
    }

    #[test]
    fn test_check_flow_lock() {
        let mut setup = Setup::new();

        let entry = setup.log_entry.build();
        setup
            .configuration
            .channel_factory
            .set_lock_queue_capacity(1)
            .set_unlock_queue_capacity(0);
        setup.flow_controller.lock_flow();

        let (test, _) = new_with_setup(setup);

        let mut state = test.state.borrow_mut();
        state.queue.push_back(EntryInfo {
            log_entry: entry,
            user_data: 0,
        });

        test.check_flow(&mut state);
        test.check_flow(&mut state);

        assert_eq!(state.flow_locked, true);
        expect_tracks(vec!["check_flow.flow_locked", "check_flow.end", "check_flow.end"]);
    }

    #[test]
    fn test_check_flow_unlock() {
        let mut setup = Setup::new();

        let entry = setup.log_entry.build();
        setup.configuration.channel_factory.set_unlock_queue_capacity(1);
        setup.flow_controller.unlock_flow();

        let (test, _) = new_with_setup(setup);

        let mut state = test.state.borrow_mut();
        state.flow_locked = true;
        state.queue.push_back(EntryInfo {
            log_entry: entry,
            user_data: 0,
        });

        test.check_flow(&mut state);
        test.check_flow(&mut state);

        assert_eq!(state.flow_locked, false);
        expect_tracks(vec!["check_flow.flow_unlocked", "check_flow.end", "check_flow.end"]);
    }

    #[test]
    fn test_clear() {
        let setup = Setup::new();

        let entry = setup.log_entry.build();
        let handler = Rc::new(CompletionHandlerMockBuilder::new().on_failed().build());

        let (test, _) = new_with_setup_handler(setup, handler);

        let mut state = test.state.borrow_mut();
        state.next_message_id = 2;
        state.queue.push_back(EntryInfo {
            log_entry: entry,
            user_data: 0,
        });

        test.clear(&mut state);

        assert_eq!(state.next_message_id, 1);
        assert_eq!(state.queue.is_empty(), true);
        expect_tracks(vec!["check_flow.end", "clear.end"]);
    }

    fn new() -> (LeaderClientProtocolImpl, ServerState) {
        new_with_setup(Setup::new())
    }

    fn new_with_setup(setup: Setup) -> (LeaderClientProtocolImpl, ServerState) {
        new_with_setup_handler(setup, Rc::new(CompletionHandlerMockBuilder::new().build()))
    }

    fn new_with_setup_handler(
        setup: Setup,
        handler: Rc<dyn CompletionHandler<usize, Error>>,
    ) -> (LeaderClientProtocolImpl, ServerState) {
        let test_state = setup.build();
        (
            LeaderClientProtocolImpl::new(test_state.context, handler, test_state.flow_controller),
            test_state.server_state,
        )
    }
}
