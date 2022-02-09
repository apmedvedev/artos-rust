#[cfg(test)]
mod tests {
    use super::*;
    use crate::artos::common::ErrorKind;
    use crate::artos::server::channel::ServerState;
    use crate::artos::server::test::{
        FollowerMessageSenderMockBuilder, MembershipProtocolMockBuilder, ReplicationProtocolMockBuilder, Setup,
        StateTransferProtocolMockBuilder,
    };
    use crate::common::utils::expect_tracks;
    use crate::common::utils::Time;
    use pretty_assertions::assert_eq;
    use uuid::Uuid;

    #[test]
    fn test_start() {
        let (test, _) = new();

        test.set_message_sender(FollowerMessageSenderMockBuilder::new().start().build());

        test.start();

        let state = test.state.borrow();
        assert_eq!(state.rewind_step, 1);
        assert_eq!(state.join_group_timer.is_started(), true);
        assert_eq!(
            state.join_group_timer.period(),
            test.context.server_channel_factory_configuration().join_group_period()
        );
        expect_tracks(vec!["start.end"]);
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

        let start_index = 10;

        setup.log_store.start_index(start_index).end_index(start_index);

        let request = setup
            .join_group_request
            .set_last_log_term(0)
            .set_last_log_index(0)
            .set_configuration(setup.configuration.server.build())
            .build();

        let (test, mut server_state) = new_with_setup(setup);

        test.set_message_sender(
            FollowerMessageSenderMockBuilder::new()
                .send(Request::JoinGroup(request))
                .build(),
        );

        {
            let mut state = test.state.borrow_mut();
            state.join_group_timer.start();
        }

        Time::advance(Duration::from_secs(1));

        test.on_timer(Time::now(), &mut server_state);

        expect_tracks(vec!["on_timer.join_timer", "request_join_group.end", "on_timer.end"]);
    }

    #[test]
    fn test_on_leader() {
        let (test, _) = new();

        {
            let mut state = test.state.borrow_mut();
            state.join_group_timer.start();
        }

        test.on_leader();

        let state = test.state.borrow();
        assert_eq!(state.join_group_timer.is_started(), false);
        expect_tracks(vec!["on_leader.end"]);
    }

    #[test]
    fn test_on_joined() {
        let (test, _) = new();

        {
            let mut state = test.state.borrow_mut();
            state.join_group_timer.start();
        }

        test.on_joined();

        let state = test.state.borrow();
        assert_eq!(state.join_group_timer.is_started(), false);
        expect_tracks(vec!["on_joined.end"]);
    }

    #[test]
    fn test_on_state_transfer_completed() {
        let (test, _) = new();

        test.on_state_transfer_completed();

        let state = test.state.borrow();
        assert_eq!(state.join_group_timer.is_started(), true);
        expect_tracks(vec!["on_state_transfer_completed.end"]);
    }

    #[test]
    fn test_handle_join_request_rejected_by_follower() {
        let setup = Setup::new();

        let request = setup.join_group_request.build();

        let (test, mut server_state) = new_with_setup(setup);

        server_state.leader_id = Some(Uuid::new_v4());

        let response = test.handle_join_group_request(request, &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), false);
        assert_eq!(response.leader_id(), server_state.leader_id);
        expect_tracks(vec!["handle_join_group_request.rejected"]);
    }

    #[test]
    fn test_handle_join_request_rejected_by_configuration_changing() {
        let setup = Setup::new();

        let request = setup.join_group_request.build();

        let (test, mut server_state) = new_with_setup(setup);

        server_state.role = ServerRole::Leader;
        server_state.configuration_changing = true;

        let response = test.handle_join_group_request(request, &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), false);
        assert_eq!(response.leader_id(), server_state.leader_id);
        expect_tracks(vec!["handle_join_group_request.rejected"]);
    }

    #[test]
    fn test_handle_join_request_already_joined() {
        let setup = Setup::new();

        let request = setup.join_group_request.build();

        let (test, mut server_state) = new_with_setup(setup);

        server_state.role = ServerRole::Leader;

        let response = test.handle_join_group_request(request, &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), true);
        assert_eq!(response.leader_id(), server_state.leader_id);
        assert_eq!(response.is_already_joined(), true);
        expect_tracks(vec![
            "handle_join_group_request.already_joined",
            "handle_join_group_request.end",
        ]);
    }

    #[test]
    fn test_handle_join_request_log_not_available() {
        let mut setup = Setup::new();

        let host = String::from("host");
        let port = 1234;
        let start_index = 10;
        let end_index = start_index + 1;
        let request = setup
            .join_group_request
            .set_server_id(Uuid::new_v4())
            .set_last_log_index(start_index - 1)
            .build();
        setup.log_store.start_index(start_index).end_index(end_index);

        let (test, mut server_state) = new_with_setup(setup);

        test.set_state_transfer_protocol(
            StateTransferProtocolMockBuilder::new()
                .host(host.clone())
                .port(port)
                .build(),
        );
        server_state.role = ServerRole::Leader;

        let response = test.handle_join_group_request(request, &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), true);
        assert_eq!(response.leader_id(), server_state.leader_id);
        assert_eq!(response.is_catching_up(), true);
        assert_eq!(response.is_state_transfer_required(), true);
        assert_eq!(
            response.state_transfer_configuration(),
            &Some(StateTransferConfiguration::new(host, port, 1))
        );
        expect_tracks(vec![
            "handle_join_group_request.new_server",
            "handle_join_group_request.log_not_available",
            "handle_join_group_request.end",
        ]);
    }

    #[test]
    fn test_handle_join_request_log_stale_by_index() {
        let mut setup = Setup::new();

        let term = 5;
        let start_index = 10;
        let end_index = start_index + 1;
        let request = setup
            .join_group_request
            .set_server_id(Uuid::new_v4())
            .set_last_log_index(end_index)
            .set_last_log_term(term)
            .build();
        setup.log_store.start_index(start_index).end_index(end_index);

        let (test, mut server_state) = new_with_setup(setup);

        server_state.role = ServerRole::Leader;

        let response = test.handle_join_group_request(request, &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), true);
        assert_eq!(response.leader_id(), server_state.leader_id);
        assert_eq!(response.is_catching_up(), true);
        assert_eq!(response.is_rewind_repeat_join(), true);
        expect_tracks(vec![
            "handle_join_group_request.new_server",
            "handle_join_group_request.log_available",
            "handle_join_group_request.log_stale",
            "handle_join_group_request.end",
        ]);
    }

    #[test]
    fn test_handle_join_request_log_stale_by_term() {
        let mut setup = Setup::new();

        let term = 5;
        let start_index = 10;
        let end_index = start_index + 1;
        let request = setup
            .join_group_request
            .set_server_id(Uuid::new_v4())
            .set_last_log_index(start_index)
            .set_last_log_term(term)
            .build();
        setup
            .log_store
            .start_index(start_index)
            .end_index(end_index)
            .get_at(vec![(start_index, setup.log_entry.set_term(term + 1).build())]);

        let (test, mut server_state) = new_with_setup(setup);

        server_state.role = ServerRole::Leader;

        let response = test.handle_join_group_request(request, &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), true);
        assert_eq!(response.leader_id(), server_state.leader_id);
        assert_eq!(response.is_catching_up(), true);
        assert_eq!(response.is_rewind_repeat_join(), true);
        expect_tracks(vec![
            "handle_join_group_request.new_server",
            "handle_join_group_request.log_available",
            "term_for_log_index.end",
            "handle_join_group_request.log_stale",
            "handle_join_group_request.end",
        ]);
    }

    #[test]
    fn test_handle_join_request_log_outdated() {
        let mut setup = Setup::new();

        let gap = 100;
        setup
            .configuration
            .channel_factory
            .set_min_state_transfer_gap_log_entry_count(gap);

        let host = String::from("host");
        let port = 1234;

        let term = 5;
        let start_index = 10;
        let end_index = start_index + gap + 1;
        let request = setup
            .join_group_request
            .set_server_id(Uuid::new_v4())
            .set_last_log_index(start_index)
            .set_last_log_term(term)
            .build();
        setup
            .log_store
            .start_index(start_index)
            .end_index(end_index)
            .get_at(vec![(start_index, setup.log_entry.set_term(term).build())]);

        let (test, mut server_state) = new_with_setup(setup);

        test.set_state_transfer_protocol(
            StateTransferProtocolMockBuilder::new()
                .host(host.clone())
                .port(port)
                .build(),
        );
        server_state.role = ServerRole::Leader;

        let response = test.handle_join_group_request(request, &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), true);
        assert_eq!(response.leader_id(), server_state.leader_id);
        assert_eq!(response.is_catching_up(), true);
        assert_eq!(response.is_state_transfer_required(), true);
        assert_eq!(
            response.state_transfer_configuration(),
            &Some(StateTransferConfiguration::new(host, port, start_index + 1))
        );
        expect_tracks(vec![
            "handle_join_group_request.new_server",
            "handle_join_group_request.log_available",
            "term_for_log_index.end",
            "handle_join_group_request.log_ok",
            "handle_join_group_request.log_outdated",
            "handle_join_group_request.end",
        ]);
    }

    #[test]
    fn test_handle_join_request() {
        let mut setup = Setup::new();

        let term = 5;
        let start_index = 10;
        let end_index = start_index + 1;
        let new_server = ServerConfiguration::new(Uuid::new_v4(), String::from("test"));
        let request = setup
            .join_group_request
            .set_server_id(new_server.id())
            .set_last_log_index(start_index)
            .set_last_log_term(term)
            .set_configuration(new_server.clone())
            .build();
        let new_configuration = setup.configuration.group.clone().add_server(new_server.clone()).build();
        setup
            .log_store
            .start_index(start_index)
            .end_index(end_index)
            .get_at(vec![(start_index, setup.log_entry.set_term(term).build())])
            .append(
                setup
                    .log_entry
                    .set_term(term)
                    .set_client_id(NULL_UUID)
                    .set_message_id(0)
                    .set_value_type(LogValueType::Configuration)
                    .set_value(bincode::serialize(&new_configuration).unwrap())
                    .build(),
            );

        let (test, mut server_state) = new_with_setup(setup);

        test.set_membership_protocol(
            MembershipProtocolMockBuilder::new()
                .reconfigure(new_configuration.clone())
                .build(),
        );
        test.set_replication_protocol(ReplicationProtocolMockBuilder::new().request_append_entries().build());
        server_state.persistent.set_term(term);
        server_state.role = ServerRole::Leader;

        let response = test.handle_join_group_request(request, &mut server_state);

        assert_eq!(server_state.configuration_changing, true);
        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), true);
        assert_eq!(response.leader_id(), server_state.leader_id);
        assert_eq!(response.is_catching_up(), true);
        expect_tracks(vec![
            "handle_join_group_request.new_server",
            "handle_join_group_request.log_available",
            "term_for_log_index.end",
            "handle_join_group_request.log_ok",
            "handle_join_group_request.accepted",
            "handle_join_group_request.end",
        ]);
    }

    #[test]
    fn test_handle_join_response_rejected_by_sender() {
        let setup = Setup::new();

        let leader_configuration = setup.configuration.server.build();
        let response = setup.join_group_response.build();

        let (test, mut server_state) = new_with_setup(setup);

        test.set_message_sender(
            FollowerMessageSenderMockBuilder::new()
                .handle_succeeded_response(false)
                .build(),
        );

        test.handle_join_group_response(leader_configuration, response, &mut server_state);

        expect_tracks(vec!["handle_join_group_response.rejected_by_sender"]);
    }

    #[test]
    fn test_handle_join_response_already_joined() {
        let mut setup = Setup::new();

        let leader_configuration = setup.configuration.server.build();
        let response = setup.join_group_response.set_already_joined(true).build();

        let (test, mut server_state) = new_with_setup(setup);

        server_state.catching_up = true;
        {
            let mut state = test.state.borrow_mut();
            state.join_group_timer.start();
        }

        test.set_message_sender(
            FollowerMessageSenderMockBuilder::new()
                .handle_succeeded_response(true)
                .build(),
        );

        test.handle_join_group_response(leader_configuration, response, &mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.rewind_step, 1);
        assert_eq!(state.join_group_timer.is_started(), false);
        assert_eq!(server_state.catching_up, false);
        expect_tracks(vec![
            "handle_join_group_response.already_joined",
            "handle_join_group_response.end",
        ]);
    }

    #[test]
    fn test_handle_join_response_catching_up() {
        let mut setup = Setup::new();

        let leader_configuration = setup.configuration.server.build();
        let response = setup.join_group_response.set_catching_up(true).build();

        let (test, mut server_state) = new_with_setup(setup);

        test.set_message_sender(
            FollowerMessageSenderMockBuilder::new()
                .handle_succeeded_response(true)
                .build(),
        );

        test.handle_join_group_response(leader_configuration, response, &mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.rewind_step, 1);
        assert_eq!(server_state.catching_up, true);
        expect_tracks(vec![
            "handle_join_group_response.catching_up",
            "handle_join_group_response.end",
        ]);
    }

    #[test]
    fn test_handle_join_response_state_transfer_required() {
        let mut setup = Setup::new();

        let host = String::from("host");
        let port = 1234;
        let log_index = 111;
        let leader_configuration = setup.configuration.server.build();
        let response = setup
            .join_group_response
            .set_catching_up(true)
            .set_state_transfer_required(true)
            .set_state_transfer_configuration(Some(StateTransferConfiguration::new(host.clone(), port, log_index)))
            .build();

        let (test, mut server_state) = new_with_setup(setup);

        {
            let mut state = test.state.borrow_mut();
            state.join_group_timer.start();
        }

        test.set_message_sender(
            FollowerMessageSenderMockBuilder::new()
                .handle_succeeded_response(true)
                .build(),
        );
        test.set_state_transfer_protocol(
            StateTransferProtocolMockBuilder::new()
                .request_state_transfer(host, port, log_index)
                .build(),
        );

        test.handle_join_group_response(leader_configuration, response, &mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.rewind_step, 1);
        assert_eq!(state.join_group_timer.is_started(), false);
        assert_eq!(server_state.catching_up, true);
        expect_tracks(vec![
            "handle_join_group_response.catching_up",
            "handle_join_group_response.state_transfer_required",
            "handle_join_group_response.end",
        ]);
    }

    #[test]
    fn test_handle_join_response_rewind_repeat_join() {
        let mut setup = Setup::new();

        let leader_configuration = setup.configuration.server.build();
        let response = setup.join_group_response.set_rewind_repeat_join(true).build();

        let (test, mut server_state) = new_with_setup(setup);

        test.set_message_sender(
            FollowerMessageSenderMockBuilder::new()
                .handle_succeeded_response(true)
                .build(),
        );

        test.handle_join_group_response(leader_configuration, response, &mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.rewind_step, 2);
        expect_tracks(vec![
            "handle_join_group_response.rewind_join",
            "handle_join_group_response.end",
        ]);
    }

    #[test]
    fn test_handle_join_response_error() {
        let (test, _) = new();

        test.set_message_sender(FollowerMessageSenderMockBuilder::new().handle_failed_response().build());

        test.handle_join_group_response_error(
            ServerConfiguration::new(Uuid::new_v4(), String::from("test")),
            Box::new(ErrorKind::TimeoutOccurred),
        );

        expect_tracks(vec!["handle_join_group_response_error.end"]);
    }

    #[test]
    fn test_request_join_group() {
        let mut setup = Setup::new();

        let term = 10;
        let index = 20;
        setup
            .log_store
            .start_index(index)
            .end_index(index + 1)
            .get_at(vec![(index, setup.log_entry.set_term(term).build())]);
        let request = setup
            .join_group_request
            .set_last_log_term(term)
            .set_last_log_index(index)
            .set_configuration(setup.configuration.server.build())
            .build();

        let (test, mut server_state) = new_with_setup(setup);

        test.set_message_sender(
            FollowerMessageSenderMockBuilder::new()
                .send(Request::JoinGroup(request))
                .build(),
        );
        let mut state = test.state.borrow_mut();

        test.request_join_group(&mut state, &mut server_state);

        expect_tracks(vec!["request_join_group.end"]);
    }

    #[test]
    fn test_term_for_log_index() {
        let mut setup = Setup::new();

        let term = 5;
        let start_index = 10;
        let entry = LogEntry::new(term, vec![], LogValueType::Application, NULL_UUID, 0);
        setup
            .log_store
            .start_index(start_index)
            .get_at(vec![(start_index + 1, entry)]);

        let (test, _) = new_with_setup(setup);

        let result = JoinGroupProtocolImpl::term_for_log_index(test.context.log_store(), 0);
        assert_eq!(result, 0);

        let result = JoinGroupProtocolImpl::term_for_log_index(test.context.log_store(), start_index - 1);
        assert_eq!(result, MAX);

        let result = JoinGroupProtocolImpl::term_for_log_index(test.context.log_store(), start_index + 1);
        assert_eq!(result, term);

        expect_tracks(vec![
            "term_for_log_index.log_index_not_set",
            "term_for_log_index.log_index_max",
            "term_for_log_index.end",
        ]);
    }

    fn new() -> (JoinGroupProtocolImpl, ServerState) {
        new_with_setup(Setup::new())
    }

    fn new_with_setup(setup: Setup) -> (JoinGroupProtocolImpl, ServerState) {
        let test_state = setup.build();
        (JoinGroupProtocolImpl::new(test_state.context), test_state.server_state)
    }
}
