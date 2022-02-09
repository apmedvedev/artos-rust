#[cfg(test)]
mod tests {
    use super::*;
    use crate::artos::message::client;
    use crate::artos::server::test::{Mocks, ServerMockBuilder, Setup};
    use crate::common::utils::expect_tracks;
    use crate::common::utils::Time;
    use pretty_assertions::assert_eq;
    use std::time::Duration;

    #[test]
    fn test_start() {
        let mut setup = Setup::new();

        let commit_index = 10;
        setup.log_store.end_index(commit_index + 1);
        setup.state_machine.begin_transaction(
            true,
            Mocks::state_machine_transaction()
                .read_state(None)
                .read_configuration(None)
                .commit()
                .build(),
        );

        let (test, mut server_state) = new_with_setup_peers(setup);

        server_state.persistent.set_commit_index(commit_index);

        test.start(&mut server_state);

        assert_eq!(server_state.quick_commit_index, commit_index);
        assert_eq!(server_state.catching_up, false);
        assert_eq!(
            server_state.peers.len() == server_state.configuration.servers().len() - 1,
            true
        );

        for server in server_state.configuration.servers() {
            if server.id() != test.context.local_server().id() {
                server_state
                    .peers
                    .iter()
                    .find(|x| x.configuration().id() == server.id())
                    .unwrap();
            }
        }

        expect_tracks(vec!["start.end"]);
    }

    #[test]
    fn test_start_read_state() {
        let mut setup = Setup::new();

        let commit_index = 10;
        setup.log_store.end_index(commit_index + 1);

        setup.persistent.set_commit_index(commit_index);
        let persistent = Some(setup.persistent);
        let configuration = Some(setup.configuration.group.build());
        setup.state_machine.begin_transaction(
            true,
            Mocks::state_machine_transaction()
                .read_state(persistent)
                .read_configuration(configuration)
                .commit()
                .build(),
        );

        let (test, mut server_state) = new_with_setup_peers(setup);

        test.start(&mut server_state);

        assert_eq!(server_state.persistent.commit_index(), commit_index);
        expect_tracks(vec![
            "start.persistent_state_read",
            "start.configuration_read",
            "start.end",
        ]);
    }

    #[test]
    fn test_start_catching_up() {
        let mut setup = Setup::new();

        let commit_index = 10;
        setup.log_store.end_index(commit_index + 1);

        let configuration = Some(setup.configuration.group.remove_server(0).build());
        setup.state_machine.begin_transaction(
            true,
            Mocks::state_machine_transaction()
                .read_state(None)
                .read_configuration(configuration)
                .commit()
                .build(),
        );

        let (test, mut server_state) = new_with_setup_peers(setup);

        test.start(&mut server_state);

        assert_eq!(server_state.catching_up, true);
        expect_tracks(vec!["start.configuration_read", "start.catching_up", "start.end"]);
    }

    #[test]
    fn test_on_timer_heartbeat() {
        let mut setup = Setup::new();

        let term = 5;
        let start_index = 10;
        let end_index = 13;
        let commit_index = 10;
        let peer_index = start_index;

        setup.log_store.start_index(start_index).end_index(end_index);

        let entry0 = LogEntry::new(term - 1, vec![], LogValueType::Application, Uuid::new_v4(), 10);
        let entry1 = LogEntry::new(term, vec![], LogValueType::Application, Uuid::new_v4(), 100);
        let entry2 = LogEntry::new(term, vec![], LogValueType::Application, Uuid::new_v4(), 200);
        setup
            .log_store
            .get(start_index + 1, end_index, vec![entry1.clone(), entry2.clone()])
            .get_at(vec![(peer_index, entry0.clone())]);
        setup.server.send();

        let (test, mut server_state) = new_with_setup(setup);

        server_state.quick_commit_index = commit_index;
        server_state.persistent.set_term(term);
        server_state.role = ServerRole::Leader;

        let peer = &server_state.peers[0];
        peer.set_next_log_index(peer_index + 1);
        peer.set_message_sender_mock();
        peer.set_next_heartbeat_time(Time::now());
        peer.set_heartbeat_enabled(true);

        let peer = &server_state.peers[1];
        peer.set_next_log_index(peer_index + 1);
        peer.set_message_sender_mock();
        peer.set_next_heartbeat_time(Time::now());
        peer.set_heartbeat_enabled(true);

        test.on_timer(Time::now(), &mut server_state);

        expect_tracks(vec![
            "on_timer.leader",
            "ensure_message_sender.already_created",
            "can_heartbeat.end",
            "on_timer.heartbeat",
            "request_append_entries_peer.send",
            "term_for_log_index.end",
            "create_append_entries_request.append_log_entries",
            "create_append_entries_request.end",
            "request_append_entries_peer.end",
            "ensure_message_sender.already_created",
            "can_heartbeat.end",
            "on_timer.heartbeat",
            "request_append_entries_peer.send",
            "term_for_log_index.end",
            "create_append_entries_request.append_log_entries",
            "create_append_entries_request.end",
            "request_append_entries_peer.end",
            "on_timer.end",
        ]);
    }

    #[test]
    fn test_on_timer_disconnect() {
        let (test, mut server_state) = new();

        server_state.role = ServerRole::Leader;
        test.set_election_protocol(Mocks::election_protocol().become_follower().build());

        test.on_timer(Time::now(), &mut server_state);
        expect_tracks(vec![
            "on_timer.leader",
            "can_heartbeat.heartbeat_disabled",
            "on_timer.disconnected",
            "can_heartbeat.heartbeat_disabled",
            "on_timer.disconnected",
            "on_timer.become_follower",
            "on_timer.end",
        ]);
    }

    #[test]
    fn test_on_timer_remove_timed_out_sessions() {
        let (test, mut server_state) = new();

        test.set_query_protocol(Mocks::query_protocol().on_session_removed().build());
        {
            let mut state = test.state.borrow_mut();
            state.client_session_manager.ensure_session(Uuid::new_v4());
            state.client_session_manager.ensure_session(Uuid::new_v4());
        }

        Time::advance(Duration::from_secs(1000));

        test.on_timer(Time::now(), &mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.client_session_manager.sessions().is_empty(), true);
        expect_tracks(vec![
            "on_timer.remove_timed_out_sessions",
            "on_session_removed.end",
            "on_session_removed.end",
            "on_timer.end",
        ]);
    }

    #[test]
    fn test_handle_publish_request() {
        let mut setup = Setup::new();

        let term = 1;
        let commit_index = 10;
        let message_id = 1;

        let client_entry = client::LogEntry::new(message_id, vec![]);
        let request = setup
            .publish_request
            .set_client_id(Uuid::new_v4())
            .set_entries(vec![client_entry])
            .build();

        let entry = setup
            .log_entry
            .set_message_id(message_id)
            .set_client_id(request.client_id())
            .build();
        setup.log_store.append(entry.clone());

        let (test, mut server_state) = new_with_setup(setup);

        server_state.leader_id = Some(test.context.local_server().id());
        server_state.role = ServerRole::Leader;
        server_state.persistent.set_term(term);

        {
            let mut state = test.state.borrow_mut();
            let session = state.client_session_manager.ensure_session(request.client_id());
            session.set_last_committed_message_id(commit_index);
        }

        let response = test.handle_publish_request(request, &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.leader_id(), server_state.leader_id);
        assert_eq!(*response.configuration(), None);
        assert_eq!(response.is_accepted(), true);
        assert_eq!(response.last_received_message_id(), message_id);
        assert_eq!(response.last_committed_message_id(), commit_index);
        assert_eq!(response.is_out_of_order_received(), false);
        expect_tracks(vec![
            "handle_publish_request.with_log_entries",
            "handle_publish_request.log_received",
            "handle_publish_request.appended",
            "request_append_entries_peer.end",
            "request_append_entries_peer.end",
            "request_append_entries_state.end",
            "handle_publish_request.end",
        ]);
    }

    #[test]
    fn test_handle_publish_request_not_leader() {
        let mut setup = Setup::new();

        let request = setup.publish_request.set_client_id(Uuid::new_v4()).build();

        let (test, mut server_state) = new_with_setup(setup);

        server_state.leader_id = Some(test.context.local_server().id());

        let response = test.handle_publish_request(request, &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.leader_id(), server_state.leader_id);
        assert_eq!(*response.configuration(), Some(server_state.configuration));
        assert_eq!(response.is_accepted(), false);
        expect_tracks(vec!["handle_publish_request.rejected"]);
    }

    #[test]
    fn test_handle_configuration_request() {
        let mut setup = Setup::new();

        let request = setup.configuration_request.set_client_id(Uuid::new_v4()).build();

        let (test, mut server_state) = new_with_setup(setup);

        let out_of_order = true;
        let last_committed = 10;
        let last_received = 100;

        {
            let mut state = test.state.borrow_mut();
            let session = state.client_session_manager.ensure_session(request.client_id());
            session.set_out_of_order_received(out_of_order);
            session.set_last_received_message_id(last_received);
            session.set_last_committed_message_id(last_committed);
        }

        server_state.leader_id = Some(test.context.local_server().id());
        server_state.role = ServerRole::Leader;

        let response = test.handle_configuration_request(request, &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.leader_id(), server_state.leader_id);
        assert_eq!(*response.configuration(), Some(server_state.configuration));
        assert_eq!(response.is_accepted(), true);
        assert_eq!(response.is_out_of_order_received(), out_of_order);
        assert_eq!(response.last_received_message_id(), last_received);
        assert_eq!(response.last_committed_message_id(), last_committed);

        expect_tracks(vec!["handle_configuration_request.end"]);
    }

    #[test]
    fn test_handle_configuration_request_rejected_not_leader() {
        let mut setup = Setup::new();

        let request = setup.configuration_request.set_client_id(Uuid::new_v4()).build();

        let (test, mut server_state) = new_with_setup(setup);

        server_state.leader_id = Some(server_state.configuration.servers()[1].id());

        let response = test.handle_configuration_request(request, &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.leader_id(), server_state.leader_id);
        assert_eq!(*response.configuration(), Some(server_state.configuration));
        assert_eq!(response.is_accepted(), false);

        expect_tracks(vec!["handle_configuration_request.rejected"]);
    }

    #[test]
    fn test_handle_restart_session_request() {
        let mut setup = Setup::new();

        let out_of_order = true;
        let last_committed = 10;
        let last_sent = 100;

        let request = setup
            .restart_session_request
            .set_client_id(Uuid::new_v4())
            .set_last_sent_message_id(last_sent)
            .build();

        let (test, mut server_state) = new_with_setup(setup);

        {
            let mut state = test.state.borrow_mut();
            let session = state.client_session_manager.ensure_session(request.client_id());
            session.set_out_of_order_received(out_of_order);
            session.set_last_committed_message_id(last_committed);
        }

        server_state.leader_id = Some(test.context.local_server().id());
        server_state.role = ServerRole::Leader;

        let response = test.handle_restart_session_request(request, &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.leader_id(), server_state.leader_id);
        assert_eq!(*response.configuration(), None);
        assert_eq!(response.is_accepted(), true);
        assert_eq!(response.is_out_of_order_received(), false);
        assert_eq!(response.last_received_message_id(), last_sent);
        assert_eq!(response.last_committed_message_id(), last_committed);

        expect_tracks(vec!["handle_restart_session_request.end"]);
    }

    #[test]
    fn test_handle_restart_session_request_rejected_not_leader() {
        let mut setup = Setup::new();

        let request = setup.restart_session_request.set_client_id(Uuid::new_v4()).build();

        let (test, mut server_state) = new_with_setup(setup);

        server_state.leader_id = Some(server_state.configuration.servers()[1].id());

        let response = test.handle_restart_session_request(request, &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.leader_id(), server_state.leader_id);
        assert_eq!(*response.configuration(), Some(server_state.configuration));
        assert_eq!(response.is_accepted(), false);

        expect_tracks(vec!["handle_restart_session_request.rejected"]);
    }

    #[test]
    fn test_handle_append_entries_request() {
        let mut setup = Setup::new();

        let term = 10;
        let start_index = 1;
        let end_index = 11;
        let at_index = 10;

        let log_entry = setup.log_entry.build();
        let request_entry = setup.log_entry.build();
        let request = setup
            .append_entries_request
            .set_term(term)
            .set_entries(vec![request_entry.clone()])
            .set_last_log_index(end_index - 1)
            .build();

        setup
            .log_store
            .start_index(start_index)
            .end_index(end_index)
            .get_at(vec![(at_index, log_entry)])
            .append(request_entry.clone());

        let (test, mut server_state) = new_with_setup(setup);

        server_state.persistent.set_term(term);
        test.set_election_protocol(Mocks::election_protocol().delay_election().build());

        {
            let mut state = test.state.borrow_mut();
            state.commits_locked = true;
        }

        let response = test.handle_append_entries_request(request.clone(), &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), true);
        assert_eq!(response.term(), term);
        assert_eq!(response.state_transfer_required(), false);
        assert_eq!(response.next_index(), request.last_log_index() + 2);
        assert_eq!(server_state.leader_id, Some(request.server_id()));
        expect_tracks(vec![
            "handle_append_entries_request.delay_election",
            "term_for_log_index.end",
            "handle_append_entries_request.append_entries",
            "append_entries.end",
            "local_commit.commit_locked",
            "commit.end",
            "handle_append_entries_request.end",
        ]);
    }

    #[test]
    fn test_handle_append_entries_request_update_term() {
        let mut setup = Setup::new();

        let term = 10;
        let start_index = 1;
        let end_index = 11;
        let at_index = 10;

        let log_entry = setup.log_entry.build();
        let request_entry = setup.log_entry.build();
        let request = setup
            .append_entries_request
            .set_term(term)
            .set_entries(vec![request_entry.clone()])
            .set_last_log_index(end_index - 1)
            .build();

        setup
            .log_store
            .start_index(start_index)
            .end_index(end_index)
            .get_at(vec![(at_index, log_entry)])
            .append(request_entry.clone());
        setup
            .state_machine
            .begin_transaction(false, Mocks::state_machine_transaction().write_state().commit().build());

        let (test, mut server_state) = new_with_setup(setup);

        server_state.persistent.set_term(term - 1);
        server_state.persistent.set_voted_for(Some(Uuid::new_v4()));
        test.set_election_protocol(Mocks::election_protocol().become_follower().delay_election().build());

        {
            let mut state = test.state.borrow_mut();
            state.commits_locked = true;
        }

        let response = test.handle_append_entries_request(request.clone(), &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), true);
        assert_eq!(response.term(), term);
        assert_eq!(response.state_transfer_required(), false);
        assert_eq!(response.next_index(), request.last_log_index() + 2);
        assert_eq!(server_state.leader_id, Some(request.server_id()));
        expect_tracks(vec![
            "handle_append_entries_request.term_updated",
            "handle_append_entries_request.delay_election",
            "term_for_log_index.end",
            "handle_append_entries_request.append_entries",
            "append_entries.end",
            "local_commit.commit_locked",
            "commit.end",
            "handle_append_entries_request.end",
        ]);
    }

    #[test]
    fn test_handle_append_entries_request_candidate() {
        let mut setup = Setup::new();

        let term = 10;
        let start_index = 1;
        let end_index = 11;
        let at_index = 10;

        let log_entry = setup.log_entry.build();
        let request_entry = setup.log_entry.build();
        let request = setup
            .append_entries_request
            .set_term(term)
            .set_entries(vec![request_entry.clone()])
            .set_last_log_index(end_index - 1)
            .build();

        setup
            .log_store
            .start_index(start_index)
            .end_index(end_index)
            .get_at(vec![(at_index, log_entry)])
            .append(request_entry.clone());

        let (test, mut server_state) = new_with_setup(setup);

        server_state.role = ServerRole::Candidate;
        server_state.persistent.set_term(term);
        test.set_election_protocol(Mocks::election_protocol().become_follower().build());

        {
            let mut state = test.state.borrow_mut();
            state.commits_locked = true;
        }

        test.handle_append_entries_request(request.clone(), &mut server_state);

        expect_tracks(vec![
            "handle_append_entries_request.become_follower",
            "term_for_log_index.end",
            "handle_append_entries_request.append_entries",
            "append_entries.end",
            "local_commit.commit_locked",
            "commit.end",
            "handle_append_entries_request.end",
        ]);
    }

    #[test]
    fn test_handle_append_entries_request_state_transfer_snapshot() {
        let mut setup = Setup::new();

        let term = 10;
        let start_index = 1;
        let end_index = 11;
        let host = String::from("test-host");
        let port = 1234;
        let start_log_index = 1;

        let state_transfer_configuration = StateTransferConfiguration::new(host.clone(), port, start_log_index);
        let entry = setup
            .log_entry
            .set_value(bincode::serialize(&state_transfer_configuration).unwrap())
            .build();
        let request = setup
            .append_entries_request
            .set_term(term)
            .set_state_transfer_request(Some(StateTransferType::Snapshot))
            .set_entries(vec![entry])
            .build();

        setup.log_store.start_index(start_index).end_index(end_index);

        let (test, mut server_state) = new_with_setup(setup);

        server_state.persistent.set_term(term);
        test.set_election_protocol(Mocks::election_protocol().delay_election().build());
        test.set_state_transfer_protocol(
            Mocks::state_transfer_protocol()
                .request_state_transfer(host, port, start_log_index)
                .build(),
        );

        {
            let mut state = test.state.borrow_mut();
            state.commits_locked = true;
        }

        let response = test.handle_append_entries_request(request.clone(), &mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.state_transfer_in_progress, true);
        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), false);
        assert_eq!(response.term(), term);
        assert_eq!(response.state_transfer_required(), true);
        assert_eq!(response.next_index(), end_index);
        expect_tracks(vec![
            "handle_append_entries_request.delay_election",
            "handle_append_entries_request.state_transfer_required",
            "handle_append_entries_request.request_state_transfer",
            "handle_append_entries_request.rejected_by_stale_log",
        ]);
    }

    #[test]
    fn test_handle_append_entries_request_state_transfer_log() {
        let mut setup = Setup::new();

        let term = 10;
        let start_index = 1;
        let end_index = 11;
        let host = String::from("test-host");
        let port = 1234;

        let state_transfer_configuration = StateTransferConfiguration::new(host.clone(), port, 0);
        let entry = setup
            .log_entry
            .set_value(bincode::serialize(&state_transfer_configuration).unwrap())
            .build();
        let request = setup
            .append_entries_request
            .set_term(term)
            .set_state_transfer_request(Some(StateTransferType::Log))
            .set_entries(vec![entry])
            .build();

        setup.log_store.start_index(start_index).end_index(end_index);

        let (test, mut server_state) = new_with_setup(setup);

        server_state.persistent.set_term(term);
        test.set_election_protocol(Mocks::election_protocol().delay_election().build());
        test.set_state_transfer_protocol(
            Mocks::state_transfer_protocol()
                .request_state_transfer(host, port, 1)
                .build(),
        );

        {
            let mut state = test.state.borrow_mut();
            state.commits_locked = true;
        }

        let response = test.handle_append_entries_request(request.clone(), &mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.state_transfer_in_progress, true);
        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), false);
        assert_eq!(response.term(), term);
        assert_eq!(response.state_transfer_required(), true);
        assert_eq!(response.next_index(), end_index);
        expect_tracks(vec![
            "handle_append_entries_request.delay_election",
            "handle_append_entries_request.state_transfer_required",
            "handle_append_entries_request.request_state_transfer",
            "handle_append_entries_request.rejected_by_stale_log",
        ]);
    }

    #[test]
    fn test_handle_append_entries_request_rejected_by_unknown_server() {
        let mut setup = Setup::new();

        let request = setup.append_entries_request.set_server_id(Uuid::new_v4()).build();

        let (test, mut server_state) = new_with_setup(setup);

        test.handle_append_entries_request(request, &mut server_state);

        expect_tracks(vec!["handle_append_entries_request.rejected_by_unknown_server"]);
    }

    #[test]
    fn test_handle_append_entries_request_rejected_by_stale_log_by_term() {
        let mut setup = Setup::new();

        let term = 10;
        let start_index = 1;
        let end_index = 11;
        let at_index = 10;

        let log_entry = setup.log_entry.build();
        let request_entry = setup.log_entry.build();
        let request = setup
            .append_entries_request
            .set_term(term - 1)
            .set_entries(vec![request_entry.clone()])
            .set_last_log_index(end_index - 1)
            .build();

        setup
            .log_store
            .start_index(start_index)
            .end_index(end_index)
            .get_at(vec![(at_index, log_entry)]);

        let (test, mut server_state) = new_with_setup(setup);

        server_state.persistent.set_term(term);

        {
            let mut state = test.state.borrow_mut();
            state.commits_locked = true;
        }

        let response = test.handle_append_entries_request(request.clone(), &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), false);
        assert_eq!(response.term(), term);
        assert_eq!(response.state_transfer_required(), false);
        assert_eq!(response.next_index(), end_index);
        expect_tracks(vec![
            "term_for_log_index.end",
            "handle_append_entries_request.rejected_by_stale_log",
        ]);
    }

    #[test]
    fn test_handle_append_entries_request_rejected_by_stale_log_by_index() {
        let mut setup = Setup::new();

        let term = 10;
        let start_index = 1;
        let end_index = 11;
        let at_index = 10;

        let log_entry = setup.log_entry.build();
        let request_entry = setup.log_entry.build();
        let request = setup
            .append_entries_request
            .set_term(term)
            .set_entries(vec![request_entry.clone()])
            .set_last_log_index(end_index + 1)
            .build();

        setup
            .log_store
            .start_index(start_index)
            .end_index(end_index)
            .get_at(vec![(at_index, log_entry)]);

        let (test, mut server_state) = new_with_setup(setup);

        server_state.persistent.set_term(term);
        test.set_election_protocol(Mocks::election_protocol().delay_election().build());

        {
            let mut state = test.state.borrow_mut();
            state.commits_locked = true;
        }

        let response = test.handle_append_entries_request(request.clone(), &mut server_state);

        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), false);
        assert_eq!(response.term(), term);
        assert_eq!(response.state_transfer_required(), false);
        assert_eq!(response.next_index(), end_index);
        expect_tracks(vec![
            "handle_append_entries_request.delay_election",
            "handle_append_entries_request.rejected_by_stale_log",
        ]);
    }

    #[test]
    fn test_handle_append_entries_response() {
        let mut setup = Setup::new();

        let next_index = 10;
        let end_index = 5;
        let matched_index = 3;
        let response = setup
            .append_entries_response
            .set_accepted(true)
            .set_server_id(setup.configuration.group.servers[1].id())
            .set_next_index(next_index)
            .build();

        setup.log_store.end_index(end_index);

        let (test, mut server_state) = new_with_setup(setup);

        server_state.role = ServerRole::Leader;

        let peer = &server_state.peers[0];
        peer.set_matched_index(matched_index);
        peer.set_rewind_step(2);

        {
            let mut state = test.state.borrow_mut();
            state.commits_locked = true;
        }

        test.handle_append_entries_response(response, &mut server_state);

        let peer = &server_state.peers[0];
        assert_eq!(peer.rewind_step(), 1);
        assert_eq!(peer.next_log_index(), next_index);
        assert_eq!(peer.matched_index(), next_index - 1);
        assert_eq!(server_state.quick_commit_index, end_index - 1);
        expect_tracks(vec![
            "handle_append_entries_response.response_accepted",
            "commit.leader",
            "request_append_entries_peer.end",
            "request_append_entries_peer.end",
            "local_commit.commit_locked",
            "commit.end",
            "handle_append_entries_response.end",
        ]);
    }

    #[test]
    fn test_handle_append_entries_response_rejected_by_not_leader() {
        let setup = Setup::new();

        let response = setup.append_entries_response.build();

        let (test, mut server_state) = new_with_setup(setup);

        test.handle_append_entries_response(response, &mut server_state);
        expect_tracks(vec!["handle_append_entries_response.rejected_not_leader"]);
    }

    #[test]
    fn test_handle_append_entries_response_rejected_by_invalid_group() {
        let mut setup = Setup::new();

        let response = setup.append_entries_response.set_group_id(Uuid::new_v4()).build();

        let (test, mut server_state) = new_with_setup(setup);

        server_state.role = ServerRole::Leader;
        test.handle_append_entries_response(response, &mut server_state);
        expect_tracks(vec!["handle_append_entries_response.rejected_invalid_group"]);
    }

    #[test]
    fn test_handle_append_entries_response_rejected_by_unknown_server() {
        let mut setup = Setup::new();

        let response = setup.append_entries_response.set_server_id(Uuid::new_v4()).build();

        let (test, mut server_state) = new_with_setup(setup);

        server_state.role = ServerRole::Leader;
        test.handle_append_entries_response(response, &mut server_state);
        expect_tracks(vec!["handle_append_entries_response.rejected_by_unknown_server"]);
    }

    #[test]
    fn test_handle_append_entries_response_rejected_by_state_transfer() {
        let mut setup = Setup::new();

        let next_index = 10;
        let response = setup
            .append_entries_response
            .set_accepted(false)
            .set_server_id(setup.configuration.group.servers[1].id())
            .set_next_index(next_index)
            .set_state_transfer_required(true)
            .build();

        let (test, mut server_state) = new_with_setup(setup);

        server_state.role = ServerRole::Leader;

        let peer = &server_state.peers[0];
        peer.set_rewind_step(2);

        test.handle_append_entries_response(response, &mut server_state);

        let peer = &server_state.peers[0];
        assert_eq!(peer.rewind_step(), 1);
        assert_eq!(peer.next_log_index(), next_index);
        expect_tracks(vec![
            "handle_append_entries_response.response_rejected_by_state_transfer",
            "handle_append_entries_response.end",
        ]);
    }

    #[test]
    fn test_handle_append_entries_response_rejected_by_stale_log() {
        let mut setup = Setup::new();

        let next_index = 10;
        let response = setup
            .append_entries_response
            .set_accepted(false)
            .set_server_id(setup.configuration.group.servers[1].id())
            .set_next_index(next_index)
            .build();

        let (test, mut server_state) = new_with_setup(setup);

        server_state.role = ServerRole::Leader;

        let peer = &server_state.peers[0];
        peer.set_rewind_step(2);
        peer.set_next_log_index(next_index + 1);

        test.handle_append_entries_response(response.clone(), &mut server_state);

        let peer = &server_state.peers[0];
        assert_eq!(peer.rewind_step(), 1);
        assert_eq!(peer.next_log_index(), next_index);

        let rewind_step = 2;
        peer.set_rewind_step(rewind_step);
        peer.set_next_log_index(next_index);

        test.handle_append_entries_response(response.clone(), &mut server_state);

        let peer = &server_state.peers[0];
        assert_eq!(peer.rewind_step(), rewind_step * 2);
        assert_eq!(peer.next_log_index(), next_index - rewind_step);

        peer.set_rewind_step(next_index + 1);
        peer.set_next_log_index(next_index);

        test.handle_append_entries_response(response.clone(), &mut server_state);

        let peer = &server_state.peers[0];
        assert_eq!(peer.rewind_step(), 1);
        assert_eq!(peer.next_log_index(), 1);

        expect_tracks(vec![
            "handle_append_entries_response.response_rejected_by_stale_log",
            "handle_append_entries_response.need_to_catchup",
            "request_append_entries_peer.end",
            "handle_append_entries_response.end",
            "handle_append_entries_response.response_rejected_by_stale_log",
            "handle_append_entries_response.need_to_catchup",
            "request_append_entries_peer.end",
            "handle_append_entries_response.end",
            "handle_append_entries_response.response_rejected_by_stale_log",
            "handle_append_entries_response.need_to_catchup",
            "request_append_entries_peer.end",
            "handle_append_entries_response.end",
        ]);
    }

    #[test]
    fn test_on_leader_initial() {
        let mut setup = Setup::new();

        let term = 10;
        let end_index = 1;
        let entry = LogEntry::new(
            term,
            bincode::serialize(&setup.configuration.group.build()).unwrap(),
            LogValueType::Configuration,
            NULL_UUID,
            0,
        );
        setup.log_store.append(entry).end_index(end_index);

        let (test, mut server_state) = new_with_setup(setup);

        server_state.persistent.set_term(term);
        server_state.role = ServerRole::Leader;
        server_state.quick_commit_index = 0;

        test.on_leader(&mut server_state);

        assert_eq!(server_state.configuration_changing, true);
        expect_tracks(vec![
            "on_leader.add_initial_configuration",
            "add_initial_configuration.end",
            "resume_heartbeating.end",
            "update_next_heartbeat_time.end",
            "start.end",
            "resume_heartbeating.end",
            "update_next_heartbeat_time.end",
            "start.end",
            "request_append_entries_peer.end",
            "request_append_entries_peer.end",
            "request_append_entries_state.end",
            "on_leader.end",
        ]);
    }

    #[test]
    fn test_on_leader() {
        let mut setup = Setup::new();

        let configuration = setup.configuration.group.build();
        let term = 10;
        let end_index = 3;
        let entry = LogEntry::new(
            term,
            bincode::serialize(&configuration).unwrap(),
            LogValueType::Configuration,
            NULL_UUID,
            0,
        );
        setup.log_store.get_at(vec![(2, entry)]).end_index(end_index);

        let (test, mut server_state) = new_with_setup(setup);

        test.set_membership_protocol(Mocks::membership_protocol().reconfigure(configuration.clone()).build());
        server_state.role = ServerRole::Leader;
        server_state.quick_commit_index = 1;
        server_state.persistent.set_term(term);

        test.on_leader(&mut server_state);

        for peer in server_state.peers {
            assert_eq!(peer.is_started(), true);
            assert_eq!(peer.next_log_index(), end_index);
        }
        assert_eq!(server_state.configuration_changing, true);
        expect_tracks(vec![
            "resume_heartbeating.end",
            "update_next_heartbeat_time.end",
            "start.end",
            "resume_heartbeating.end",
            "update_next_heartbeat_time.end",
            "start.end",
            "on_leader.uncommitted_configuration_change",
            "request_append_entries_peer.end",
            "request_append_entries_peer.end",
            "request_append_entries_state.end",
            "on_leader.end",
        ]);
    }

    #[test]
    fn test_on_follower() {
        let (test, mut server_state) = new();

        server_state.peers.iter().for_each(|f| f.start());

        test.on_follower(&mut server_state);

        server_state
            .peers
            .iter()
            .for_each(|f| assert_eq!(f.is_started(), false));

        expect_tracks(vec![
            "resume_heartbeating.end",
            "update_next_heartbeat_time.end",
            "start.end",
            "resume_heartbeating.end",
            "update_next_heartbeat_time.end",
            "start.end",
            "on_peer_disconnected.already_disconnected",
            "stop.end",
            "on_peer_disconnected.already_disconnected",
            "stop.end",
            "on_follower.end",
        ]);
    }

    #[test]
    fn test_on_left() {
        let (test, _) = new();

        test.set_query_protocol(Mocks::query_protocol().on_session_removed().build());

        {
            let mut state = test.state.borrow_mut();
            state.client_session_manager.ensure_session(Uuid::new_v4());
            state.client_session_manager.ensure_session(Uuid::new_v4());
        }

        test.on_left();

        let state = test.state.borrow();
        assert_eq!(state.client_session_manager.sessions().is_empty(), true);
        expect_tracks(vec!["on_session_removed.end", "on_session_removed.end", "on_left.end"]);
    }

    #[test]
    fn test_on_state_tarnsfer_completed() {
        let (test, _) = new();

        {
            let mut state = test.state.borrow_mut();
            state.state_transfer_in_progress = true;
        }

        test.on_state_transfer_completed();

        let state = test.state.borrow();
        assert_eq!(state.state_transfer_in_progress, false);
    }

    #[test]
    fn test_publish() {
        let mut setup = Setup::new();

        let term = 1;
        let commit_index = 10;
        let message_id = 1;
        let entry = setup
            .log_entry
            .set_message_id(message_id)
            .set_client_id(setup.configuration.server.id)
            .build();
        setup.log_store.append(entry.clone());

        let (test, mut server_state) = new_with_setup(setup);

        server_state.persistent.set_term(term);
        server_state.role = ServerRole::Leader;

        {
            let mut state = test.state.borrow_mut();
            let session = state
                .client_session_manager
                .ensure_session(test.context.local_server().id());
            session.set_last_committed_message_id(commit_index);
        }

        let result = test.publish(entry, &mut server_state);
        assert_eq!(result, commit_index);

        let mut state = test.state.borrow_mut();
        let session = state
            .client_session_manager
            .ensure_session(test.context.local_server().id());
        assert_eq!(session.last_received_message_id(), message_id);
        expect_tracks(vec![
            "request_append_entries_peer.end",
            "request_append_entries_peer.end",
            "request_append_entries_state.end",
            "publish.end",
            "last_committed_message_id.end",
        ]);
    }

    #[test]
    fn test_publish_follower() {
        let setup = Setup::new();
        let entry = setup.log_entry.build();

        let (test, mut server_state) = new_with_setup(setup);

        assert_eq!(test.publish(entry, &mut server_state), 0);

        expect_tracks(vec!["publish.not_leader"]);
    }

    #[test]
    fn test_last_committed_message_id() {
        let last_committed = 10;

        let (test, mut server_state) = new();

        let mut state = test.state.borrow_mut();
        let session = state
            .client_session_manager
            .ensure_session(test.context.local_server().id());
        session.set_last_committed_message_id(last_committed);

        server_state.role = ServerRole::Leader;

        assert_eq!(
            test.last_committed_message_id_state(&mut state, &mut server_state),
            last_committed
        );

        server_state.role = ServerRole::Follower;

        assert_eq!(test.last_committed_message_id_state(&mut state, &mut server_state), 0);
        expect_tracks(vec![
            "last_committed_message_id.end",
            "last_committed_message_id.not_leader",
        ]);
    }

    #[test]
    fn test_last_received_message_id() {
        let last_received = 10;

        let (test, mut server_state) = new();

        {
            let mut state = test.state.borrow_mut();
            let session = state
                .client_session_manager
                .ensure_session(test.context.local_server().id());
            session.set_last_received_message_id(last_received);
        }

        server_state.role = ServerRole::Leader;

        assert_eq!(test.last_received_message_id(&mut server_state), last_received);

        server_state.role = ServerRole::Follower;

        assert_eq!(test.last_received_message_id(&mut server_state), 0);
        expect_tracks(vec![
            "last_received_message_id.end",
            "last_received_message_id.not_leader",
        ]);
    }

    #[test]
    fn test_lock_commits() {
        let (test, _) = new();
        {
            let mut state = test.state.borrow_mut();
            state.commits_locked = false;
        }

        test.lock_commits();

        let state = test.state.borrow();
        assert_eq!(state.commits_locked, true);
        expect_tracks(vec!["lock_commits.end"]);
    }

    #[test]
    fn test_unlock_commits() {
        let (test, _) = new();
        {
            let mut state = test.state.borrow_mut();
            state.commits_locked = false;
        }

        test.unlock_commits();

        let state = test.state.borrow();
        assert_eq!(state.commits_locked, false);
        expect_tracks(vec!["unlock_commits.end"]);
    }

    #[test]
    fn test_acquire_transient_state() {
        let (test, mut server_state) = new();

        let client_id1 = Uuid::new_v4();
        let client_id2 = Uuid::new_v4();
        let last_committed1 = 10;
        let last_committed2 = 20;

        server_state.quick_commit_index = 5;

        {
            let mut state = test.state.borrow_mut();

            let session1 = state.client_session_manager.ensure_session(client_id1);
            session1.set_last_committed_message_id(last_committed1);

            let session2 = state.client_session_manager.ensure_session(client_id2);
            session2.set_last_committed_message_id(last_committed2);
        }

        let transient_state = test.acquire_transient_state(&mut server_state);

        assert_eq!(transient_state.commit_index(), server_state.quick_commit_index);
        assert_eq!(
            *transient_state.client_sessions(),
            vec![
                ClientSessionTransientState::new(client_id1, last_committed1),
                ClientSessionTransientState::new(client_id2, last_committed2)
            ]
        );
        expect_tracks(vec!["acquire_transient_state.end"]);
    }

    #[test]
    fn test_apply_transient_state() {
        let mut setup = Setup::new();

        let client_id1 = Uuid::new_v4();
        let client_id2 = Uuid::new_v4();
        let min_commit_index = 5;
        let entry1 = setup.log_entry.set_client_id(client_id1).set_message_id(100).build();
        let entry2 = setup.log_entry.set_client_id(client_id2).set_message_id(200).build();

        setup
            .log_store
            .start_index(min_commit_index)
            .end_index(min_commit_index + 3)
            .get_at(vec![
                (min_commit_index + 1, entry1.clone()),
                (min_commit_index + 2, entry2.clone()),
            ]);

        let (test, mut server_state) = new_with_setup(setup);

        server_state.quick_commit_index = min_commit_index + 5;
        server_state.role = ServerRole::Leader;

        let transient_states = vec![
            (
                Uuid::new_v4(),
                ServerTransientState::new(
                    vec![
                        ClientSessionTransientState::new(client_id1, 12),
                        ClientSessionTransientState::new(client_id2, 50),
                    ],
                    min_commit_index,
                ),
            ),
            (
                Uuid::new_v4(),
                ServerTransientState::new(
                    vec![
                        ClientSessionTransientState::new(client_id1, 11),
                        ClientSessionTransientState::new(client_id2, 100),
                    ],
                    min_commit_index + 1,
                ),
            ),
        ];

        {
            let mut state = test.state.borrow_mut();

            let session1 = state.client_session_manager.ensure_session(client_id1);
            session1.set_last_committed_message_id(20);
        }

        test.apply_transient_state(transient_states, &mut server_state);

        let mut state = test.state.borrow_mut();
        let session1 = state.client_session_manager.ensure_session(client_id1);
        assert_eq!(session1.last_committed_message_id(), 20);
        assert_eq!(session1.last_received_message_id(), entry1.message_id());

        let session2 = state.client_session_manager.ensure_session(client_id2);
        assert_eq!(session2.last_committed_message_id(), 100);
        assert_eq!(session2.is_committed(), false);
        assert_eq!(session2.last_received_message_id(), entry2.message_id());

        expect_tracks(vec![
            "apply_transient_state.update_session",
            "apply_transient_state.update_session",
            "apply_transient_state.end",
        ]);
    }

    #[test]
    fn test_request_append_entries_peer() {
        let mut setup = Setup::new();

        let term = 5;
        let start_index = 10;
        let end_index = 13;
        let commit_index = 10;
        let peer_index = start_index;

        setup.log_store.start_index(start_index).end_index(end_index);

        let entry0 = LogEntry::new(term - 1, vec![], LogValueType::Application, Uuid::new_v4(), 10);
        let entry1 = LogEntry::new(term, vec![], LogValueType::Application, Uuid::new_v4(), 100);
        let entry2 = LogEntry::new(term, vec![], LogValueType::Application, Uuid::new_v4(), 200);
        setup
            .log_store
            .get(start_index + 1, end_index, vec![entry1.clone(), entry2.clone()])
            .get_at(vec![(peer_index, entry0.clone())]);
        setup.server.send();

        let (test, mut server_state) = new_with_setup(setup);

        server_state.quick_commit_index = commit_index;
        server_state.persistent.set_term(term);
        server_state.role = ServerRole::Leader;

        let peer = &server_state.peers[1];
        peer.set_next_log_index(peer_index + 1);

        let state = test.state.borrow();

        test.request_append_entries_peer(peer, &state, &server_state);

        peer.set_message_sender_mock();
        peer.set_request_in_progress(true);

        test.request_append_entries_peer(peer, &state, &server_state);

        peer.set_message_sender_mock();
        peer.set_request_in_progress(false);

        test.request_append_entries_peer(peer, &state, &server_state);

        expect_tracks(vec![
            "request_append_entries_peer.end",
            "request_append_entries_peer.end",
            "request_append_entries_peer.send",
            "term_for_log_index.end",
            "create_append_entries_request.append_log_entries",
            "create_append_entries_request.end",
            "request_append_entries_peer.end",
        ]);
    }

    #[test]
    fn test_request_append_entries_state() {
        let (test, mut server_state) = new();
        let mut state = test.state.borrow_mut();

        test.request_append_entries_state(&mut state, &mut server_state);
        expect_tracks(vec![
            "request_append_entries_peer.end",
            "request_append_entries_peer.end",
            "request_append_entries_state.end",
        ]);
    }

    #[test]
    fn test_request_append_entries_state_single_server() {
        let mut setup = Setup::new();
        let commit_index = 10;

        setup.log_store.end_index(commit_index + 2);

        let (test, mut server_state) = new_with_setup(setup);
        server_state.quick_commit_index = commit_index;
        server_state.persistent.set_commit_index(commit_index);
        server_state.peers.clear();

        let mut state = test.state.borrow_mut();
        state.commits_locked = true;

        test.request_append_entries_state(&mut state, &mut server_state);

        assert_eq!(server_state.quick_commit_index, commit_index + 1);
        expect_tracks(vec![
            "request_append_entries_state.single_server",
            "local_commit.commit_locked",
            "commit.end",
        ]);
    }

    #[test]
    fn test_append_entries() {
        let mut setup = Setup::new();

        let term = 5;
        let start_index = 1;
        let end_index = start_index + 2;
        let entry1 = setup.log_entry.set_term(term).build();
        let entry21 = setup.log_entry.set_term(term).build();
        let entry22 = setup.log_entry.set_term(term + 1).build();
        let entry3 = setup.log_entry.set_term(term + 1).build();

        setup
            .log_store
            .end_index(end_index)
            .get_at(vec![(start_index, entry1.clone()), (start_index + 1, entry21.clone())])
            .set_at(start_index + 1, entry22.clone())
            .append(entry3.clone());

        let (test, _) = new_with_setup(setup);

        test.append_entries(&vec![entry1, entry22, entry3], start_index);

        expect_tracks(vec!["append_entries.end"]);
    }

    #[test]
    fn test_on_session_removed() {
        let (test, _) = new();

        test.set_query_protocol(Mocks::query_protocol().on_session_removed().build());

        let mut state = test.state.borrow_mut();
        let session = ClientSession::new(Uuid::new_v4());

        test.on_session_removed(&session, &mut state);
        expect_tracks(vec!["on_session_removed.end"]);
    }

    #[test]
    fn test_add_initial_configuration() {
        let mut setup = Setup::new();

        let term = 10;
        let entry = LogEntry::new(
            term,
            bincode::serialize(&setup.configuration.group.build()).unwrap(),
            LogValueType::Configuration,
            NULL_UUID,
            0,
        );
        setup.log_store.append(entry);

        let (test, mut server_state) = new_with_setup(setup);

        server_state.persistent.set_term(term);

        test.add_initial_configuration(&mut server_state);

        assert_eq!(server_state.configuration_changing, true);
        expect_tracks(vec!["add_initial_configuration.end"]);
    }

    #[test]
    fn test_commit_leader() {
        let mut setup = Setup::new();

        let term = 5;
        let start_index = 10;
        let end_index = 13;
        let commit_index = 10;
        let peer_index = start_index;

        setup.log_store.start_index(start_index).end_index(end_index);

        let entry0 = LogEntry::new(term - 1, vec![], LogValueType::Application, Uuid::new_v4(), 10);
        let entry1 = LogEntry::new(term, vec![], LogValueType::Application, Uuid::new_v4(), 100);
        let entry2 = LogEntry::new(term, vec![], LogValueType::Application, Uuid::new_v4(), 200);
        setup
            .log_store
            .get(start_index + 1, end_index, vec![entry1.clone(), entry2.clone()])
            .get_at(vec![(peer_index, entry0.clone())]);
        setup.server.send();

        let (test, mut server_state) = new_with_setup(setup);

        server_state.quick_commit_index = commit_index;
        server_state.persistent.set_term(term);
        server_state.role = ServerRole::Leader;
        server_state.persistent.set_commit_index(commit_index);

        let peer = &server_state.peers[1];
        peer.set_next_log_index(peer_index + 1);

        let mut state = test.state.borrow_mut();
        state.commits_locked = true;

        test.request_append_entries_peer(peer, &state, &server_state);

        peer.set_message_sender_mock();
        peer.set_request_in_progress(false);

        test.request_append_entries_peer(peer, &state, &server_state);

        peer.set_message_sender_mock();
        peer.set_request_in_progress(false);

        test.commit(commit_index + 1, &mut state, &mut server_state);

        assert_eq!(server_state.quick_commit_index, commit_index + 1);
        expect_tracks(vec![
            "request_append_entries_peer.end",
            "request_append_entries_peer.send",
            "term_for_log_index.end",
            "create_append_entries_request.append_log_entries",
            "create_append_entries_request.end",
            "request_append_entries_peer.end",
            "commit.leader",
            "request_append_entries_peer.end",
            "request_append_entries_peer.send",
            "term_for_log_index.end",
            "create_append_entries_request.append_log_entries",
            "create_append_entries_request.end",
            "request_append_entries_peer.end",
            "local_commit.commit_locked",
            "commit.end",
        ]);
    }

    #[test]
    fn test_commit_follower() {
        let commit_index = 10;
        let (test, mut server_state) = new();
        server_state.quick_commit_index = commit_index;
        server_state.persistent.set_commit_index(commit_index);

        let mut state = test.state.borrow_mut();
        state.commits_locked = true;
        test.commit(commit_index + 1, &mut state, &mut server_state);

        assert_eq!(server_state.quick_commit_index, commit_index + 1);
        expect_tracks(vec!["local_commit.commit_locked", "commit.end"]);
    }

    #[test]
    fn test_commit_not_required() {
        let commit_index = 10;
        let (test, mut server_state) = new();
        server_state.quick_commit_index = commit_index;
        server_state.persistent.set_commit_index(commit_index);

        let mut state = test.state.borrow_mut();
        test.commit(commit_index - 1, &mut state, &mut server_state);
        expect_tracks(vec!["local_commit.commit_not_required", "commit.end"]);
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

        let result = ReplicationProtocolImpl::term_for_log_index(test.context.log_store(), 0);
        assert_eq!(result, 0);

        let result = ReplicationProtocolImpl::term_for_log_index(test.context.log_store(), start_index - 1);
        assert_eq!(result, MAX);

        let result = ReplicationProtocolImpl::term_for_log_index(test.context.log_store(), start_index + 1);
        assert_eq!(result, term);

        expect_tracks(vec![
            "term_for_log_index.log_index_not_set",
            "term_for_log_index.log_index_max",
            "term_for_log_index.end",
        ]);
    }

    #[test]
    fn test_create_append_entries_request() {
        let mut setup = Setup::new();

        let term = 5;
        let start_index = 10;
        let end_index = 13;
        let commit_index = 10;
        let peer_index = start_index;

        setup.log_store.start_index(start_index).end_index(end_index);

        let entry0 = LogEntry::new(term - 1, vec![], LogValueType::Application, Uuid::new_v4(), 10);
        let entry1 = LogEntry::new(term, vec![], LogValueType::Application, Uuid::new_v4(), 100);
        let entry2 = LogEntry::new(term, vec![], LogValueType::Application, Uuid::new_v4(), 200);
        setup
            .log_store
            .get(start_index + 1, end_index, vec![entry1.clone(), entry2.clone()])
            .get_at(vec![(peer_index, entry0.clone())]);

        let (test, mut server_state) = new_with_setup(setup);

        server_state.quick_commit_index = commit_index;
        server_state.persistent.set_term(term);
        server_state.role = ServerRole::Leader;

        let peer = &server_state.peers[1];
        peer.set_next_log_index(peer_index + 1);

        let state = test.state.borrow();

        if let Request::AppendEntries(request) = test.create_append_entries_request(peer, &state, &server_state) {
            assert_eq!(request.server_id(), test.context.local_server().id());
            assert_eq!(request.group_id(), test.context.group_id());
            assert_eq!(request.term(), term);
            assert_eq!(request.last_log_term(), term - 1);
            assert_eq!(request.last_log_index(), start_index);
            assert_eq!(request.commit_index(), commit_index);
            assert_eq!(request.entries(), &vec![entry1, entry2]);
            assert_eq!(request.state_transfer_request(), None);
        } else {
            assert!(false);
        }

        expect_tracks(vec![
            "term_for_log_index.end",
            "create_append_entries_request.append_log_entries",
            "create_append_entries_request.end",
        ]);
    }

    #[test]
    fn test_create_append_entries_request_state_transfer_snapshot() {
        let mut setup = Setup::new();

        let term = 5;
        let start_index = 10;
        let end_index = 13;
        let commit_index = 10;
        let peer_index = start_index - 1;
        let host = String::from("host");
        let port = 1234;

        setup.log_store.start_index(start_index).end_index(end_index);

        let (test, mut server_state) = new_with_setup(setup);

        test.set_state_transfer_protocol(Mocks::state_transfer_protocol().host(host.clone()).port(port).build());

        server_state.quick_commit_index = commit_index;
        server_state.persistent.set_term(term);
        server_state.role = ServerRole::Leader;

        let peer = &server_state.peers[1];
        peer.set_next_log_index(peer_index + 1);

        let state = test.state.borrow();

        if let Request::AppendEntries(request) = test.create_append_entries_request(peer, &state, &server_state) {
            assert_eq!(request.server_id(), test.context.local_server().id());
            assert_eq!(request.group_id(), test.context.group_id());
            assert_eq!(request.term(), term);
            assert_eq!(request.last_log_term(), MAX);
            assert_eq!(request.last_log_index(), peer_index);
            assert_eq!(request.commit_index(), commit_index);
            assert_eq!(request.state_transfer_request(), Some(StateTransferType::Snapshot));

            let entry = LogEntry::new(
                0,
                bincode::serialize(&StateTransferConfiguration::new(host, port, 1)).unwrap(),
                LogValueType::Application,
                NULL_UUID,
                0,
            );
            assert_eq!(request.entries(), &vec![entry]);
        } else {
            assert!(false);
        }

        expect_tracks(vec![
            "term_for_log_index.log_index_max",
            "create_append_entries_request.state_transfer_request",
            "create_append_entries_request.end",
        ]);
    }

    #[test]
    fn test_create_append_entries_request_state_transfer_log() {
        let mut setup = Setup::new();
        setup
            .configuration
            .channel_factory
            .set_min_state_transfer_gap_log_entry_count(10);

        let term = 5;
        let start_index = 10;
        let end_index = 23;
        let commit_index = 10;
        let peer_index = start_index;
        let host = String::from("host");
        let port = 1234;

        setup.log_store.start_index(start_index).end_index(end_index);

        let entry0 = LogEntry::new(term - 1, vec![], LogValueType::Application, Uuid::new_v4(), 10);
        setup.log_store.get_at(vec![(peer_index, entry0.clone())]);

        let (test, mut server_state) = new_with_setup(setup);

        test.set_state_transfer_protocol(Mocks::state_transfer_protocol().host(host.clone()).port(port).build());

        server_state.quick_commit_index = commit_index;
        server_state.persistent.set_term(term);
        server_state.role = ServerRole::Leader;

        let peer = &server_state.peers[1];
        peer.set_next_log_index(peer_index + 1);

        let state = test.state.borrow();

        if let Request::AppendEntries(request) = test.create_append_entries_request(peer, &state, &server_state) {
            assert_eq!(request.server_id(), test.context.local_server().id());
            assert_eq!(request.group_id(), test.context.group_id());
            assert_eq!(request.term(), term);
            assert_eq!(request.last_log_term(), term - 1);
            assert_eq!(request.last_log_index(), peer_index);
            assert_eq!(request.commit_index(), commit_index);
            assert_eq!(request.state_transfer_request(), Some(StateTransferType::Log));

            let entry = LogEntry::new(
                0,
                bincode::serialize(&StateTransferConfiguration::new(host, port, 1)).unwrap(),
                LogValueType::Application,
                NULL_UUID,
                0,
            );
            assert_eq!(request.entries(), &vec![entry]);
        } else {
            assert!(false);
        }

        expect_tracks(vec![
            "term_for_log_index.end",
            "create_append_entries_request.state_transfer_request",
            "create_append_entries_request.end",
        ]);
    }

    #[test]
    fn test_local_commit() {
        let mut setup = Setup::new();

        let term = 10;
        let configuration = setup.configuration.group.build();
        let entry1 = LogEntry::new(
            term,
            bincode::serialize(&configuration).unwrap(),
            LogValueType::Configuration,
            NULL_UUID,
            0,
        );
        let entry2 = LogEntry::new(term, vec![], LogValueType::Application, Uuid::new_v4(), 100);
        let entry3 = LogEntry::new(term, vec![], LogValueType::Application, Uuid::new_v4(), 200);

        let commit_index = 10;
        setup
            .log_store
            .end_index(commit_index + 4)
            .get_at(vec![
                (commit_index + 1, entry1.clone()),
                (commit_index + 2, entry2.clone()),
                (commit_index + 3, entry3.clone()),
            ])
            .commit(commit_index + 3);

        setup.state_machine.begin_transaction(
            false,
            Mocks::state_machine_transaction()
                .write_state()
                .write_configuration()
                .publish(2)
                .commit()
                .build(),
        );

        let (test, mut server_state) = new_with_setup(setup);

        test.set_query_protocol(Mocks::query_protocol().on_commit().build());
        test.set_membership_protocol(Mocks::membership_protocol().commit_configuration(configuration).build());

        server_state.persistent.set_commit_index(commit_index);
        server_state.quick_commit_index = commit_index + 3;
        let mut state = test.state.borrow_mut();

        test.local_commit(&mut state, &mut server_state);

        let session1 = state.client_session_manager.ensure_session(entry2.client_id());
        assert_eq!(session1.last_committed_message_id(), 100);
        assert_eq!(session1.is_committed(), false);

        let session2 = state.client_session_manager.ensure_session(entry3.client_id());
        assert_eq!(session2.last_committed_message_id(), 200);
        assert_eq!(session2.is_committed(), false);

        assert_eq!(server_state.persistent.commit_index(), commit_index + 3);

        expect_tracks(vec!["local_commit.end"]);
    }

    #[test]
    fn test_local_commit_locked() {
        let (test, mut server_state) = new();

        let mut state = test.state.borrow_mut();
        state.commits_locked = true;

        test.local_commit(&mut state, &mut server_state);

        expect_tracks(vec!["local_commit.commit_locked"]);
    }

    #[test]
    fn test_local_commit_not_required() {
        let mut setup = Setup::new();

        let commit_index = 10;
        setup.log_store.end_index(commit_index + 1);

        let (test, mut server_state) = new_with_setup(setup);

        server_state.persistent.set_commit_index(commit_index);
        server_state.quick_commit_index = commit_index - 1;
        let mut state = test.state.borrow_mut();

        test.local_commit(&mut state, &mut server_state);

        server_state.quick_commit_index = commit_index + 1;

        test.local_commit(&mut state, &mut server_state);

        expect_tracks(vec![
            "local_commit.commit_not_required",
            "local_commit.commit_not_required",
        ]);
    }

    fn new() -> (ReplicationProtocolImpl, ServerState) {
        new_with_setup(Setup::new())
    }

    fn new_with_setup(setup: Setup) -> (ReplicationProtocolImpl, ServerState) {
        let test_state = setup.build();
        (
            ReplicationProtocolImpl::new(Rc::new(test_state.server), test_state.context),
            test_state.server_state,
        )
    }

    fn new_with_setup_peers(setup: Setup) -> (ReplicationProtocolImpl, ServerState) {
        let mut test_state = setup.build();

        let mut peers = vec![];
        std::mem::swap(&mut test_state.server_state.peers, &mut peers);
        ServerMockBuilder::create_peers_mock(&mut test_state.server, peers);

        (
            ReplicationProtocolImpl::new(Rc::new(test_state.server), test_state.context),
            test_state.server_state,
        )
    }
}
