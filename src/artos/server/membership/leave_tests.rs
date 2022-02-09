#[cfg(test)]
mod tests {
    use super::*;
    use crate::artos::server::channel::ServerState;
    use crate::artos::server::test::{
        CompletionHandlerMockBuilder, ElectionProtocolMockBuilder, FollowerMessageSenderMockBuilder,
        MembershipProtocolMockBuilder, ReplicationProtocolMockBuilder, Setup, StateMachineTransactionMockBuilder,
    };
    use crate::common::utils::expect_tracks;
    use crate::common::utils::init_tracks;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_start() {
        let mut setup = Setup::new();

        setup
            .configuration
            .channel_factory
            .set_auto_leave_group_timeout(Duration::from_secs(1));
        let (test, _) = new_with_setup(setup);

        test.set_message_sender(FollowerMessageSenderMockBuilder::new().start().build());

        test.start();

        let state = test.state.borrow();
        assert_eq!(
            state.leave_group_timer.period(),
            test.context.server_channel_factory_configuration().leave_group_period()
        );
        assert_eq!(
            state.auto_leave_group_timeout.period(),
            test.context
                .server_channel_factory_configuration()
                .auto_leave_group_timeout()
        );
        expect_tracks(vec!["start.auto_leave", "start.end"]);
    }

    #[test]
    fn test_stop() {
        let (test, _) = new();

        test.set_message_sender(FollowerMessageSenderMockBuilder::new().stop().build());

        test.stop();
        expect_tracks(vec!["stop.end"]);
    }

    #[test]
    fn test_on_timer() {
        let mut setup = Setup::new();

        setup
            .configuration
            .channel_factory
            .set_auto_leave_group_timeout(Duration::from_secs(10))
            .set_leave_group_period(Duration::from_secs(10));

        let request = setup.leave_group_request.build();

        let (test, mut server_state) = new_with_setup(setup);

        {
            let mut state = test.state.borrow_mut();
            state.leave_group_timer.start();
            state.auto_leave_group_timeout.start();
        }

        test.set_message_sender(
            FollowerMessageSenderMockBuilder::new()
                .send(Request::LeaveGroup(request))
                .build(),
        );

        server_state.configuration_changing = true;

        Time::advance(Duration::from_secs(10));

        test.on_timer(Time::now(), &mut server_state);

        expect_tracks(vec![
            "on_timer.leave_timer",
            "request_leave_group.end",
            "on_timer.auto_leave_timeout",
            "handle_auto_leave_group_timeout.configuration_changing",
            "on_timer.end",
        ]);
    }

    #[test]
    fn test_on_leader() {
        let mut setup = Setup::new();

        setup
            .configuration
            .channel_factory
            .set_auto_leave_group_timeout(Duration::from_secs(10));

        let (test, _) = new_with_setup(setup);

        test.on_leader();

        let state = test.state.borrow();
        assert_eq!(state.auto_leave_group_timeout.is_started(), true);
        expect_tracks(vec!["on_leader.auto_leave_start", "on_leader.end"]);
    }

    #[test]
    fn test_on_follower() {
        let mut setup = Setup::new();

        setup
            .configuration
            .channel_factory
            .set_auto_leave_group_timeout(Duration::from_secs(10));

        let (test, _) = new_with_setup(setup);

        {
            let mut state = test.state.borrow_mut();
            state.auto_leave_group_timeout.start();
        }

        test.on_follower();

        let state = test.state.borrow();
        assert_eq!(state.auto_leave_group_timeout.is_started(), false);
        expect_tracks(vec!["on_follower.auto_leave_stop", "on_follower.end"]);
    }

    #[test]
    fn test_graceful_close() {
        let (test, mut server_state) = new();

        test.set_election_protocol(
            ElectionProtocolMockBuilder::new()
                .become_follower()
                .lock_election()
                .build(),
        );

        server_state.role = ServerRole::Leader;

        test.graceful_close(&mut server_state);

        expect_tracks(vec!["graceful_close.leader", "graceful_close.end"]);
    }

    #[test]
    fn test_leave_group_single_server() {
        let (test, mut server_state) = new();

        server_state.peers.clear();

        test.leave_group(
            Box::new(CompletionHandlerMockBuilder::new().on_failed().build()),
            &mut server_state,
        );

        expect_tracks(vec!["leave_group.single_server"]);
    }

    #[test]
    fn test_leave_group() {
        let (test, mut server_state) = new();

        server_state.role = ServerRole::Leader;
        test.set_election_protocol(
            ElectionProtocolMockBuilder::new()
                .become_follower()
                .lock_election()
                .build(),
        );

        test.leave_group(Box::new(CompletionHandlerMockBuilder::new().build()), &mut server_state);

        expect_tracks(vec![
            "leave_group.leader",
            "leave_group_follower.end",
            "leave_group.end",
        ]);
    }

    #[test]
    fn test_on_left() {
        let (test, _) = new();

        {
            let mut state = test.state.borrow_mut();
            state.leave_group_timer.start();
            state.leave_group_completion_handler =
                Some(Box::new(CompletionHandlerMockBuilder::new().on_succeeded().build()));
        }

        test.on_left();

        let state = test.state.borrow();
        assert_eq!(state.leave_group_timer.is_started(), false);
        assert_eq!(state.leave_group_completion_handler.is_none(), true);
        expect_tracks(vec!["on_left.end"]);
    }

    #[test]
    fn test_handle_leave_group_request_rejected_by_follower() {
        let setup = Setup::new();

        let request = setup.leave_group_request.build();

        let (test, mut server_state) = new_with_setup(setup);

        let response = test.handle_leave_group_request(request, &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), false);
        assert_eq!(response.leader_id(), server_state.leader_id);
        expect_tracks(vec!["handle_leave_group_request.rejected"]);
    }

    #[test]
    fn test_handle_leave_group_request_rejected_by_configuration_changing() {
        let setup = Setup::new();

        let request = setup.leave_group_request.build();

        let (test, mut server_state) = new_with_setup(setup);

        server_state.role = ServerRole::Leader;
        server_state.configuration_changing = true;

        let response = test.handle_leave_group_request(request, &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), false);
        assert_eq!(response.leader_id(), server_state.leader_id);
        expect_tracks(vec!["handle_leave_group_request.rejected"]);
    }

    #[test]
    fn test_handle_leave_group_request_already_left() {
        let mut setup = Setup::new();

        let request = setup.leave_group_request.set_server_id(Uuid::new_v4()).build();

        let (test, mut server_state) = new_with_setup(setup);

        server_state.role = ServerRole::Leader;

        let response = test.handle_leave_group_request(request, &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), true);
        assert_eq!(response.leader_id(), server_state.leader_id);
        assert_eq!(response.is_already_left(), true);
        assert_eq!(response.configuration(), &Some(server_state.configuration));
        expect_tracks(vec![
            "handle_leave_group_request.already_left",
            "handle_leave_group_request.end",
        ]);
    }

    #[test]
    fn test_handle_leave_group_request() {
        let mut setup = Setup::new();

        let term = 5;
        let request = setup
            .leave_group_request
            .set_server_id(setup.configuration.group.servers[1].id())
            .build();

        let configuration = setup.configuration.group.clone().remove_server(1).build();
        setup.log_store.append(
            setup
                .log_entry
                .set_term(term)
                .set_value_type(LogValueType::Configuration)
                .set_value(bincode::serialize(&configuration).unwrap())
                .set_client_id(NULL_UUID)
                .build(),
        );

        let (test, mut server_state) = new_with_setup(setup);

        server_state.persistent.set_term(term);
        test.set_membership_protocol(MembershipProtocolMockBuilder::new().reconfigure(configuration).build());
        test.set_replication_protocol(ReplicationProtocolMockBuilder::new().request_append_entries().build());
        server_state.role = ServerRole::Leader;

        let response = test.handle_leave_group_request(request, &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), true);
        assert_eq!(response.leader_id(), server_state.leader_id);
        expect_tracks(vec![
            "handle_leave_group_request.leave_from_group",
            "remove_servers_from_group.end",
            "handle_leave_group_request.end",
        ]);
    }

    #[test]
    fn test_handle_leave_group_response_rejected_by_message_sender() {
        let setup = Setup::new();

        let response = setup.leave_group_response.build();
        let leader_configuration = setup.configuration.server.build();

        let (test, mut server_state) = new_with_setup(setup);

        test.set_message_sender(
            FollowerMessageSenderMockBuilder::new()
                .handle_succeeded_response(false)
                .build(),
        );
        test.handle_leave_group_response(leader_configuration, response, &mut server_state);

        expect_tracks(vec!["handle_leave_group_response.rejected_by_message_sender"]);
    }

    #[test]
    fn test_handle_leave_group_response_leaving() {
        let mut setup = Setup::new();

        let response = setup
            .leave_group_response
            .set_configuration(Some(setup.configuration.group.build()))
            .build();
        let leader_configuration = setup.configuration.server.build();

        let (test, mut server_state) = new_with_setup(setup);

        test.set_message_sender(
            FollowerMessageSenderMockBuilder::new()
                .handle_succeeded_response(true)
                .build(),
        );
        test.handle_leave_group_response(leader_configuration, response, &mut server_state);

        expect_tracks(vec!["handle_leave_group_response.leaving"]);
    }

    #[test]
    fn test_handle_leave_group_response_already_left() {
        let mut setup = Setup::new();

        let leader_configuration = setup.configuration.server.build();
        setup.configuration.group.remove_server(0);
        let response = setup
            .leave_group_response
            .set_already_left(true)
            .set_configuration(Some(setup.configuration.group.build()))
            .build();

        let (test, mut server_state) = new_with_setup(setup);

        test.set_message_sender(
            FollowerMessageSenderMockBuilder::new()
                .handle_succeeded_response(true)
                .build(),
        );
        test.handle_leave_group_response(leader_configuration, response, &mut server_state);

        expect_tracks(vec!["handle_leave_group_response.already_left"]);
    }

    #[test]
    fn test_handle_leave_group_response() {
        let mut setup = Setup::new();

        let leader_configuration = setup.configuration.server.build();
        let new_configuration = setup.configuration.group.clone().remove_server(0).build();
        let response = setup
            .leave_group_response
            .set_already_left(true)
            .set_configuration(Some(new_configuration.clone()))
            .build();

        setup.state_machine.begin_transaction(
            false,
            StateMachineTransactionMockBuilder::new()
                .write_configuration()
                .commit()
                .build(),
        );

        let (test, mut server_state) = new_with_setup(setup);

        test.set_membership_protocol(
            MembershipProtocolMockBuilder::new()
                .reconfigure(new_configuration)
                .build(),
        );
        test.set_message_sender(
            FollowerMessageSenderMockBuilder::new()
                .handle_succeeded_response(true)
                .build(),
        );

        test.handle_leave_group_response(leader_configuration, response, &mut server_state);

        expect_tracks(vec!["handle_leave_group_response.end"]);
    }

    #[test]
    fn test_handle_leave_group_response_error() {
        let setup = Setup::new();

        let leader_configuration = setup.configuration.server.build();

        let (test, _) = new_with_setup(setup);

        test.set_message_sender(FollowerMessageSenderMockBuilder::new().handle_failed_response().build());

        test.handle_leave_group_response_error(leader_configuration, Box::new(ErrorKind::TimeoutOccurred));

        expect_tracks(vec!["handle_leave_group_response_error.end"]);
    }

    #[test]
    fn test_request_leave_group() {
        let setup = Setup::new();

        let request = setup.leave_group_request.build();

        let (test, mut server_state) = new_with_setup(setup);

        {
            let mut state = test.state.borrow_mut();
            state.leave_group_timer.start();
            state.auto_leave_group_timeout.start();
        }

        test.set_message_sender(
            FollowerMessageSenderMockBuilder::new()
                .send(Request::LeaveGroup(request))
                .build(),
        );

        let mut state = test.state.borrow_mut();

        test.request_leave_group(&mut state, &mut server_state);

        expect_tracks(vec!["request_leave_group.end"]);
    }

    #[test]
    fn test_request_leave_group_fail() {
        let mut setup = Setup::new();

        setup
            .configuration
            .channel_factory
            .set_leave_group_attempt_count(0)
            .set_leave_group_period(Duration::from_secs(1));

        let (test, mut server_state) = new_with_setup(setup);

        let mut state = test.state.borrow_mut();
        state.leave_group_timer.start();
        Time::advance(Duration::from_secs(1));
        state.leave_group_timer.is_fired(Time::now());
        state.leave_group_completion_handler = Some(Box::new(CompletionHandlerMockBuilder::new().on_failed().build()));

        test.request_leave_group(&mut state, &mut server_state);

        assert_eq!(state.leave_group_timer.is_started(), false);
        assert_eq!(state.leave_group_completion_handler.is_none(), true);
        expect_tracks(vec!["request_leave_group.fail_to_leave"]);
    }

    #[test]
    fn test_handle_auto_leave_group_timeout_configuration_changing() {
        let (test, mut server_state) = new();

        server_state.configuration_changing = true;
        let mut state = test.state.borrow_mut();

        test.handle_auto_leave_group_timeout(&mut state, &mut server_state);

        expect_tracks(vec!["handle_auto_leave_group_timeout.configuration_changing"]);
    }

    #[test]
    fn test_handle_auto_leave_group_timeout() {
        let mut setup = Setup::new();

        let term = 5;
        let configuration = setup.configuration.group.clone().remove_server(1).build();
        setup.log_store.append(
            setup
                .log_entry
                .set_term(term)
                .set_value_type(LogValueType::Configuration)
                .set_value(bincode::serialize(&configuration).unwrap())
                .set_client_id(NULL_UUID)
                .build(),
        );

        setup
            .configuration
            .channel_factory
            .set_auto_leave_group_timeout(Duration::from_secs(1));

        let (test, mut server_state) = new_with_setup(setup);

        server_state.persistent.set_term(term);
        test.set_membership_protocol(MembershipProtocolMockBuilder::new().reconfigure(configuration).build());
        test.set_replication_protocol(ReplicationProtocolMockBuilder::new().request_append_entries().build());

        server_state.peers[0].start();
        let mut state = test.state.borrow_mut();

        Time::advance(Duration::from_secs(1));
        for i in 1..server_state.peers.len() {
            server_state.peers[i].start();
        }

        init_tracks();

        test.handle_auto_leave_group_timeout(&mut state, &mut server_state);

        expect_tracks(vec![
            "handle_auto_leave_group_timeout.auto_leave",
            "remove_servers_from_group.end",
            "handle_auto_leave_group_timeout.end",
        ]);
    }

    #[test]
    fn test_remove_servers_from_group() {
        let mut setup = Setup::new();

        let term = 5;
        let server_id = setup.configuration.group.servers[1].id();
        let configuration = setup.configuration.group.clone().remove_server(1).build();
        setup.log_store.append(
            setup
                .log_entry
                .set_term(term)
                .set_value_type(LogValueType::Configuration)
                .set_value(bincode::serialize(&configuration).unwrap())
                .set_client_id(NULL_UUID)
                .build(),
        );

        let (test, mut server_state) = new_with_setup(setup);

        server_state.persistent.set_term(term);
        test.set_membership_protocol(MembershipProtocolMockBuilder::new().reconfigure(configuration).build());
        test.set_replication_protocol(ReplicationProtocolMockBuilder::new().request_append_entries().build());
        let mut state = test.state.borrow_mut();

        test.remove_servers_from_group(vec![server_id], &mut state, &mut server_state);

        assert_eq!(server_state.configuration_changing, true);
        expect_tracks(vec!["remove_servers_from_group.end"]);
    }

    #[test]
    fn test_leave_group_follower() {
        let (test, _) = new();

        let mut state = test.state.borrow_mut();
        test.leave_group_follower(Box::new(CompletionHandlerMockBuilder::new().build()), &mut state);

        assert_eq!(state.leave_group_timer.is_started(), true);
        assert_eq!(state.leave_group_completion_handler.is_some(), true);
        expect_tracks(vec!["leave_group_follower.end"]);
    }

    fn new() -> (LeaveGroupProtocolImpl, ServerState) {
        new_with_setup(Setup::new())
    }

    fn new_with_setup(setup: Setup) -> (LeaveGroupProtocolImpl, ServerState) {
        let test_state = setup.build();
        (LeaveGroupProtocolImpl::new(test_state.context), test_state.server_state)
    }
}
