#[cfg(test)]
mod tests {
    use super::*;
    use crate::artos::message::server::NULL_UUID;
    use crate::artos::server::test::{Mocks, Setup};
    use crate::common::utils::expect_tracks;
    use crate::common::utils::Time;
    use pretty_assertions::assert_eq;
    use std::time::Duration;

    #[test]
    fn test_new() {
        let (test, _) = new();
        let state = test.state.borrow();

        assert_eq!(test.endpoint, *test.context.local_server().endpoint());
        assert_eq!(state.voted_servers.len(), 0);
        assert_eq!(state.transient_states.len(), 0);
        assert_eq!(state.votes_granted, 0);
        assert_eq!(state.election_completed, false);
        assert_eq!(state.election_timer.is_started(), false);
        assert_eq!(state.election_timer.period(), Duration::from_secs(0));
        assert_eq!(state.next_voting_time, Time::now());
        assert_eq!(state.election_locked, false);
        assert_eq!(state.pre_vote_phase, false);
    }

    #[test]
    fn test_start() {
        let (test, mut server_state) = new();

        test.start(&mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.election_timer.is_started(), true);
        expect_tracks(vec!["restart_election_timer.end", "start.end"]);
    }

    #[test]
    fn test_stop() {
        let (test, mut server_state) = new();

        test.start(&mut server_state);
        test.stop();

        let state = test.state.borrow();
        assert_eq!(state.election_timer.is_started(), false);
        expect_tracks(vec!["restart_election_timer.end", "start.end", "stop.end"]);
    }

    #[test]
    fn test_election_timer_fired() {
        let mut setup = Setup::new();

        setup.log_store.last(1).end_index(1);
        setup.server.send();

        let (test, mut server_state) = new_with_setup(setup);

        let period = Duration::from_secs(10);
        {
            let mut state = test.state.borrow_mut();
            state.election_timer.set_period(period);
            state.election_timer.start();
        }

        Time::advance(period + Duration::from_secs(1));

        test.on_timer(Time::now(), &mut server_state);

        expect_tracks(vec![
            "on_timer.election_timer_fired",
            "restart_election_timer.end",
            "clear_election_state.end",
            "request_votes.end",
            "request_pre_vote_for_self.end",
            "request_election.end",
            "on_timer.end",
        ]);
    }

    #[test]
    fn test_delay_election() {
        let (test, mut server_state) = new();

        test.delay_election(&mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.election_timer.is_started(), true);
        let min_election_timeout = test
            .context
            .server_channel_factory_configuration()
            .min_election_timeout();
        assert_eq!(state.next_voting_time, Time::now() + min_election_timeout);
        expect_tracks(vec!["restart_election_timer.end", "delay_election.end"]);
    }

    #[test]
    fn test_become_follower() {
        let (test, mut server_state) = new();

        test.set_membership_protocol(Mocks::membership_protocol().on_follower().build());

        {
            let mut state = test.state.borrow_mut();
            state.election_completed = true;
        }
        server_state.role = ServerRole::Leader;

        test.become_follower(&mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.election_completed, false);
        assert_eq!(server_state.role, ServerRole::Follower);
        expect_tracks(vec![
            "clear_election_state.end",
            "restart_election_timer.end",
            "become_follower_state.end",
            "become_follower.end",
        ]);
    }

    #[test]
    fn test_lock_election() {
        let (test, _) = new();

        test.lock_election();

        let state = test.state.borrow();
        assert_eq!(state.election_locked, true);
        expect_tracks(vec!["lock_election.end"]);
    }

    #[test]
    fn test_is_vote_request_rejected_by_unknown_server() {
        let mut setup = Setup::new();

        let unknown_server_id = Uuid::new_v4();
        let ref request = setup.vote_request.set_server_id(unknown_server_id).build();

        let (test, mut server_state) = new_with_setup(setup);
        let ref mut state = test.state.borrow_mut();

        let accepted = test.is_vote_request_accepted(request, state, &mut server_state);

        assert_eq!(accepted, false);
        expect_tracks(vec!["is_vote_request_accepted.rejected_by_unknown_server"]);
    }

    #[test]
    fn test_is_vote_request_rejected_by_next_voting_time() {
        let setup = Setup::new();
        let ref request = setup.vote_request.build();
        let (test, mut server_state) = new_with_setup(setup);

        let ref mut state = test.state.borrow_mut();
        state.next_voting_time = Time::now() + Duration::from_secs(1);

        let accepted = test.is_vote_request_accepted(request, state, &mut server_state);

        assert_eq!(accepted, false);
        expect_tracks(vec!["is_vote_request_accepted.rejected_by_voting_time"]);
    }

    #[test]
    fn test_is_vote_request_rejected_by_log() {
        let tracks = vec!["is_vote_request_accepted.end"];
        check_is_vote_request_accepted(2, 1, 1, 1, 1, 1, false, None, false, false, tracks);
        let tracks = vec!["is_vote_request_accepted.end"];
        check_is_vote_request_accepted(1, 1, 2, 1, 1, 1, false, None, false, false, tracks);
        let tracks = vec!["is_vote_request_accepted.end"];
        check_is_vote_request_accepted(1, 1, 1, 1, 10, 8, false, None, false, false, tracks);
    }

    #[test]
    fn test_is_vote_request_accepted_by_log() {
        let tracks = vec![
            "is_vote_request_accepted.vote_term_updated",
            "clear_election_state.end",
            "restart_election_timer.end",
            "become_follower_state.end",
            "is_vote_request_accepted.end",
        ];
        check_is_vote_request_accepted(1, 2, 1, 1, 1, 1, true, None, false, true, tracks);
        let tracks = vec!["is_vote_request_accepted.end"];
        check_is_vote_request_accepted(1, 1, 1, 2, 1, 1, false, None, false, true, tracks);
        let tracks = vec!["is_vote_request_accepted.end"];
        check_is_vote_request_accepted(1, 1, 1, 1, 10, 9, false, None, false, true, tracks);
    }

    #[test]
    fn test_is_vote_request_rejected_by_voted_for() {
        let tracks = vec!["is_vote_request_accepted.end"];
        let voted_for = Some(Uuid::new_v4());
        check_is_vote_request_accepted(1, 1, 1, 1, 1, 1, false, voted_for, false, false, tracks);
    }

    #[test]
    fn test_is_vote_request_accepted_by_pre_vote() {
        let tracks = vec!["is_vote_request_accepted.end"];
        let voted_for = Some(Uuid::new_v4());
        check_is_vote_request_accepted(1, 1, 1, 1, 1, 1, false, voted_for, true, true, tracks);
    }

    #[test]
    fn test_is_vote_request_accepted_by_voted_for() {
        let tracks = vec!["is_vote_request_accepted.end"];
        let voted_for = Some(NULL_UUID);
        check_is_vote_request_accepted(1, 1, 1, 1, 1, 1, false, voted_for, false, true, tracks);
    }

    fn check_is_vote_request_accepted(
        log_term: u64,
        request_term: u64,
        log_last_term: u64,
        request_last_term: u64,
        log_last_index: u64,
        request_last_index: u64,
        state_machine_called: bool,
        voted_for: Option<Uuid>,
        pre_vote: bool,
        result: bool,
        expected_tracks: Vec<&str>,
    ) {
        let mut setup = Setup::new();

        let ref request = setup
            .vote_request
            .set_term(request_term)
            .set_last_log_term(request_last_term)
            .set_last_log_index(request_last_index)
            .set_pre_vote(pre_vote)
            .build();

        setup.log_store.last(log_last_term).end_index(log_last_index);

        if state_machine_called {
            setup
                .state_machine
                .begin_transaction(false, Mocks::state_machine_transaction().write_state().commit().build());
        }

        let (test, mut server_state) = new_with_setup(setup);

        if state_machine_called {
            test.set_membership_protocol(Mocks::membership_protocol().on_follower().build());
        }

        let ref mut state = test.state.borrow_mut();
        state.next_voting_time = Time::now() - Duration::from_secs(1);

        server_state.persistent.set_term(log_term);
        server_state.persistent.set_voted_for(voted_for);
        if let Some(voted_for) = voted_for {
            if voted_for == NULL_UUID {
                server_state.persistent.set_voted_for(Some(request.server_id()));
            }
        }

        let accepted = test.is_vote_request_accepted(request, state, &mut server_state);

        assert_eq!(accepted, result);
        expect_tracks(expected_tracks);
    }

    #[test]
    fn test_handle_vote_request_accepted_vote() {
        let mut setup = Setup::new();

        let term = 1;
        let end_index = 1;
        let commit_index = 10;
        let request = setup.vote_request.build();

        setup.log_store.last(term).end_index(end_index);
        setup
            .state_machine
            .begin_transaction(false, Mocks::state_machine_transaction().write_state().commit().build());

        let (test, mut server_state) = new_with_setup(setup);
        test.set_replication_protocol(
            Mocks::replication_protocol()
                .acquire_transient_state(commit_index)
                .build(),
        );
        {
            let ref mut state = test.state.borrow_mut();
            state.next_voting_time = Time::now() - Duration::from_secs(1);
        }

        server_state.persistent.set_term(term);
        let server_id = request.server_id();

        let response = test.handle_vote_request(request, &mut server_state);

        assert_eq!(*server_state.persistent.voted_for().as_ref().unwrap(), server_id);
        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), true);
        assert_eq!(response.term(), term);
        assert_eq!(
            response.transient_state().as_ref().unwrap().commit_index(),
            commit_index
        );
        expect_tracks(vec![
            "is_vote_request_accepted.end",
            "handle_vote_request.accepted_vote",
            "handle_vote_request.end",
        ]);
    }

    #[test]
    fn test_handle_vote_request_accepted_pre_vote() {
        let mut setup = Setup::new();

        let term = 1;
        let end_index = 1;
        let request = setup.vote_request.set_pre_vote(true).build();

        setup.log_store.last(term).end_index(end_index);

        let (test, mut server_state) = new_with_setup(setup);
        {
            let ref mut state = test.state.borrow_mut();
            state.next_voting_time = Time::now() - Duration::from_secs(1);
        }

        server_state.persistent.set_term(term);

        let response = test.handle_vote_request(request, &mut server_state);

        assert_eq!(server_state.persistent.voted_for(), None);
        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), true);
        assert_eq!(response.term(), term);
        expect_tracks(vec!["is_vote_request_accepted.end", "handle_vote_request.end"]);
    }

    #[test]
    fn test_handle_vote_request_rejected() {
        let mut setup = Setup::new();

        let request = setup.vote_request.set_server_id(Uuid::new_v4()).build();
        setup.log_store.last(1).end_index(1);

        let (test, mut server_state) = new_with_setup(setup);

        let response = test.handle_vote_request(request, &mut server_state);

        assert_eq!(server_state.persistent.voted_for(), None);
        assert_eq!(response.server_id(), test.context.local_server().id());
        assert_eq!(response.group_id(), test.context.group_id());
        assert_eq!(response.is_accepted(), false);
        expect_tracks(vec![
            "is_vote_request_accepted.rejected_by_unknown_server",
            "handle_vote_request.end",
        ]);
    }

    #[test]
    fn test_handle_vote_response_ignore_by_group_id() {
        let mut setup = Setup::new();
        let response = setup.vote_response.set_group_id(Uuid::new_v4()).build();

        let (test, mut server_state) = new_with_setup(setup);

        test.handle_vote_response(response, &mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.voted_servers.is_empty(), true);
        assert_eq!(state.votes_granted, 0);
        expect_tracks(vec!["handle_vote_response.invalid_group"]);
    }

    #[test]
    fn test_handle_vote_response_ignore_by_election_complete() {
        let setup = Setup::new();
        let response = setup.vote_response.build();

        let (test, mut server_state) = new_with_setup(setup);

        {
            let ref mut state = test.state.borrow_mut();
            state.election_completed = true;
        }

        test.handle_vote_response(response, &mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.voted_servers.is_empty(), true);
        assert_eq!(state.votes_granted, 0);
        expect_tracks(vec!["handle_vote_response.election_completed"]);
    }

    #[test]
    fn test_handle_vote_response_ignore_by_already_voted() {
        let setup = Setup::new();
        let response = setup.vote_response.build();

        let (test, mut server_state) = new_with_setup(setup);

        {
            let ref mut state = test.state.borrow_mut();
            state.voted_servers.push(response.server_id());
        }

        test.handle_vote_response(response, &mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.voted_servers.len(), 1);
        assert_eq!(state.votes_granted, 0);
        expect_tracks(vec!["handle_vote_response.already_voted"]);
    }

    #[test]
    fn test_handle_vote_response_vote_rejected() {
        let mut setup = Setup::new();
        let response = setup.vote_response.set_accepted(false).build();

        let (test, mut server_state) = new_with_setup(setup);

        test.handle_vote_response(response, &mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.voted_servers.len(), 1);
        assert_eq!(state.votes_granted, 0);
        expect_tracks(vec!["handle_vote_response.end"]);
    }

    #[test]
    fn test_handle_vote_response_vote_accepted_minority() {
        let mut setup = Setup::new();
        let response = setup
            .vote_response
            .set_server_id(setup.configuration.group.servers[1].id())
            .build();

        let (test, mut server_state) = new_with_setup(setup);

        test.handle_vote_response(response, &mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.voted_servers.len(), 1);
        assert_eq!(state.votes_granted, 1);
        assert_eq!(state.election_completed, false);
        assert_eq!(server_state.role, ServerRole::Follower);
        expect_tracks(vec![
            "handle_vote_response.accepted",
            "handle_vote_response.apply_transient_state",
            "handle_vote_response.end",
        ]);
    }

    #[test]
    fn test_handle_vote_response_accepted_majority_pre_vote() {
        let mut setup = Setup::new();
        let response1 = setup
            .vote_response
            .set_server_id(setup.configuration.group.servers[1].id())
            .build();
        let response2 = setup
            .vote_response
            .set_server_id(setup.configuration.group.servers[2].id())
            .build();

        setup.state_machine.write_commit();
        setup.log_store.last(1).end_index(1);
        setup.server.send();

        let (test, mut server_state) = new_with_setup(setup);

        {
            let mut state = test.state.borrow_mut();
            state.pre_vote_phase = true;
        }

        test.handle_vote_response(response1, &mut server_state);
        test.handle_vote_response(response2, &mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.election_completed, false);
        assert_eq!(server_state.role, ServerRole::Candidate);
        expect_tracks(vec![
            "handle_vote_response.accepted",
            "handle_vote_response.end",
            "handle_vote_response.accepted",
            "handle_vote_response.granted_pre_vote",
            "become_candidate.state_follower",
            "restart_election_timer.end",
            "clear_election_state.end",
            "request_votes.end",
            "request_vote_for_self.end",
            "become_candidate.end",
            "handle_vote_response.end",
        ]);
    }

    #[test]
    fn test_handle_vote_response_accepted_majority_vote() {
        let mut setup = Setup::new();
        let response1 = setup
            .vote_response
            .set_server_id(setup.configuration.group.servers[1].id())
            .build();
        let response2 = setup
            .vote_response
            .set_server_id(setup.configuration.group.servers[2].id())
            .build();

        setup.log_store.last(1).end_index(1);
        setup.server.send();

        let (test, mut server_state) = new_with_setup(setup);

        test.set_replication_protocol(Mocks::replication_protocol().apply_transient_state().build());
        test.set_membership_protocol(Mocks::membership_protocol().on_leader().build());

        test.handle_vote_response(response1, &mut server_state);
        test.handle_vote_response(response2, &mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.election_completed, true);
        assert_eq!(server_state.role, ServerRole::Leader);
        expect_tracks(vec![
            "handle_vote_response.accepted",
            "handle_vote_response.apply_transient_state",
            "handle_vote_response.end",
            "handle_vote_response.accepted",
            "handle_vote_response.apply_transient_state",
            "handle_vote_response.granted_vote",
            "election_completed.election_completed",
            "election_completed.end",
            "become_leader.end",
            "handle_vote_response.end",
        ]);
    }

    #[test]
    fn test_handle_vote_response_vote_completed() {
        let mut setup = Setup::new();
        let response0 = setup
            .vote_response
            .set_server_id(setup.configuration.group.servers[0].id())
            .build();
        let response1 = setup
            .vote_response
            .set_server_id(setup.configuration.group.servers[1].id())
            .set_accepted(false)
            .build();
        let response2 = setup
            .vote_response
            .set_server_id(setup.configuration.group.servers[2].id())
            .set_accepted(false)
            .build();

        setup.log_store.last(1).end_index(1);
        setup.server.send();

        let (test, mut server_state) = new_with_setup(setup);

        {
            let mut state = test.state.borrow_mut();
            state.pre_vote_phase = true;
        }

        test.handle_vote_response(response0, &mut server_state);
        test.handle_vote_response(response1, &mut server_state);
        test.handle_vote_response(response2, &mut server_state);

        let state = test.state.borrow();
        assert_eq!(state.election_completed, true);
        expect_tracks(vec![
            "handle_vote_response.accepted",
            "handle_vote_response.end",
            "handle_vote_response.end",
            "handle_vote_response.election_completed",
            "election_completed.election_completed",
            "election_completed.end",
            "handle_vote_response.end",
        ]);
    }

    #[test]
    fn test_restart_election_timer_ignore_catching_up() {
        let (test, mut server_state) = new();

        server_state.catching_up = true;
        let ref mut state = test.state.borrow_mut();
        test.restart_election_timer(state, &server_state);

        assert_eq!(state.election_timer.is_started(), false);
        expect_tracks(vec!["restart_election_timer.catching_up"]);
    }

    #[test]
    fn test_restart_election_timer() {
        let (test, server_state) = new();

        let ref mut state = test.state.borrow_mut();

        test.restart_election_timer(state, &server_state);

        let max_election_timeout = test
            .context
            .server_channel_factory_configuration()
            .max_election_timeout();
        let min_election_timeout = test
            .context
            .server_channel_factory_configuration()
            .min_election_timeout();

        assert_eq!(state.election_timer.is_started(), true);
        assert_eq!(state.election_timer.period() >= min_election_timeout, true);
        assert_eq!(state.election_timer.period() <= max_election_timeout, true);
        expect_tracks(vec!["restart_election_timer.end"]);
    }

    #[test]
    fn test_request_election_ignore_catching_up() {
        let (test, mut server_state) = new();

        server_state.catching_up = true;
        let ref mut state = test.state.borrow_mut();
        test.request_election(state, &mut server_state);

        expect_tracks(vec!["request_election.catching_up"]);
    }

    #[test]
    fn test_request_election_ignore_election_locked() {
        let (test, mut server_state) = new();

        let ref mut state = test.state.borrow_mut();
        state.election_locked = true;
        test.request_election(state, &mut server_state);

        expect_tracks(vec!["request_election.election_locked"]);
    }

    #[test]
    fn test_request_election() {
        let mut setup = Setup::new();

        setup.log_store.last(1).end_index(1);
        setup.server.send();

        let (test, mut server_state) = new_with_setup(setup);

        let ref mut state = test.state.borrow_mut();

        test.request_election(state, &mut server_state);

        expect_tracks(vec![
            "restart_election_timer.end",
            "clear_election_state.end",
            "request_votes.end",
            "request_pre_vote_for_self.end",
            "request_election.end",
        ]);
    }

    #[test]
    fn test_become_leader() {
        let (test, mut server_state) = new();

        test.set_replication_protocol(Mocks::replication_protocol().apply_transient_state().build());
        test.set_membership_protocol(Mocks::membership_protocol().on_leader().build());

        let ref mut state = test.state.borrow_mut();
        state.election_timer.start();

        test.become_leader(state, &mut server_state);

        assert_eq!(state.election_timer.is_started(), false);
        assert_eq!(server_state.role, ServerRole::Leader);
        assert_eq!(
            server_state.leader_id.as_ref().unwrap(),
            &test.context.local_server().id()
        );
        expect_tracks(vec![
            "election_completed.election_completed",
            "election_completed.end",
            "become_leader.end",
        ]);
    }

    #[test]
    fn test_become_candidate() {
        let mut setup = Setup::new();

        setup.state_machine.write_commit();
        setup.log_store.last(1).end_index(1);
        setup.server.send();

        let (test, mut server_state) = new_with_setup(setup);

        let ref mut state = test.state.borrow_mut();
        server_state.persistent.set_voted_for(Some(Uuid::new_v4()));
        server_state.persistent.set_term(1);

        test.become_candidate(state, &mut server_state);

        assert_eq!(server_state.persistent.term(), 2);
        assert_eq!(
            server_state.persistent.voted_for(),
            Some(test.context.local_server().id())
        );
        assert_eq!(server_state.role, ServerRole::Candidate);
        expect_tracks(vec![
            "become_candidate.state_follower",
            "restart_election_timer.end",
            "clear_election_state.end",
            "request_votes.end",
            "request_vote_for_self.end",
            "become_candidate.end",
        ]);
    }

    #[test]
    fn test_become_candidate_ignore_already_candidate() {
        let (test, mut server_state) = new();

        let ref mut state = test.state.borrow_mut();
        server_state.role = ServerRole::Candidate;
        test.become_candidate(state, &mut server_state);

        expect_tracks(vec!["become_candidate.end"]);
    }

    #[test]
    fn test_request_pre_vote_for_self_single_server() {
        let mut setup = Setup::new();

        setup
            .state_machine
            .begin_transaction(false, Mocks::state_machine_transaction().write_state().commit().build());
        setup.log_store.last(1).end_index(1);
        setup.server.send();

        let (test, mut server_state) = new_with_setup(setup);

        test.set_membership_protocol(Mocks::membership_protocol().on_leader().build());
        test.set_replication_protocol(Mocks::replication_protocol().apply_transient_state().build());
        let ref mut state = test.state.borrow_mut();
        server_state.peers.clear();

        test.request_pre_vote_for_self(state, &mut server_state);

        expect_tracks(vec![
            "restart_election_timer.end",
            "clear_election_state.end",
            "request_pre_vote_for_self.single_server",
            "become_candidate.state_follower",
            "restart_election_timer.end",
            "clear_election_state.end",
            "request_vote_for_self.single_server",
            "election_completed.election_completed",
            "election_completed.end",
            "become_leader.end",
            "become_candidate.end",
        ]);
    }

    #[test]
    fn test_request_pre_vote_for_self() {
        let mut setup = Setup::new();

        setup.log_store.last(1).end_index(1);
        setup.server.send();

        let (test, mut server_state) = new_with_setup(setup);

        let ref mut state = test.state.borrow_mut();

        test.request_pre_vote_for_self(state, &mut server_state);

        assert_eq!(state.pre_vote_phase, true);
        expect_tracks(vec![
            "restart_election_timer.end",
            "clear_election_state.end",
            "request_votes.end",
            "request_pre_vote_for_self.end",
        ]);
    }

    #[test]
    fn test_request_vote_for_self_single_server() {
        let mut setup = Setup::new();

        setup.log_store.last(1).end_index(1);
        setup.server.send();

        let (test, mut server_state) = new_with_setup(setup);

        test.set_membership_protocol(Mocks::membership_protocol().on_leader().build());
        test.set_replication_protocol(Mocks::replication_protocol().apply_transient_state().build());
        let ref mut state = test.state.borrow_mut();
        server_state.peers.clear();

        test.request_vote_for_self(state, &mut server_state);

        expect_tracks(vec![
            "restart_election_timer.end",
            "clear_election_state.end",
            "request_vote_for_self.single_server",
            "election_completed.election_completed",
            "election_completed.end",
            "become_leader.end",
        ]);
    }

    #[test]
    fn test_request_vote_for_self() {
        let mut setup = Setup::new();

        setup
            .state_machine
            .begin_transaction(false, Mocks::state_machine_transaction().write_state().commit().build());
        setup.log_store.last(1).end_index(1);
        setup.server.send();

        let (test, mut server_state) = new_with_setup(setup);

        let ref mut state = test.state.borrow_mut();

        test.request_vote_for_self(state, &mut server_state);

        assert_eq!(
            server_state.persistent.voted_for(),
            Some(test.context.local_server().id())
        );
        expect_tracks(vec![
            "restart_election_timer.end",
            "clear_election_state.end",
            "request_votes.end",
            "request_vote_for_self.end",
        ]);
    }

    #[test]
    fn test_request_votes() {
        let mut setup = Setup::new();

        setup.log_store.last(1).end_index(1);
        setup.server.send();

        let (test, mut server_state) = new_with_setup(setup);

        let ref mut state = test.state.borrow_mut();

        test.request_votes(state, &mut server_state, true);

        assert_eq!(state.votes_granted, 1);
        assert_eq!(state.voted_servers, vec![test.context.local_server().id()]);
        expect_tracks(vec!["request_votes.end"]);
    }

    #[test]
    fn test_clear_election_state() {
        let (test, _) = new();

        let ref mut state = test.state.borrow_mut();
        state.election_completed = true;
        state.votes_granted = 1;
        state.voted_servers.push(Uuid::new_v4());
        state.pre_vote_phase = true;

        test.clear_election_state(state);

        assert_eq!(state.election_completed, false);
        assert_eq!(state.votes_granted, 0);
        assert_eq!(state.voted_servers, vec![]);
        assert_eq!(state.pre_vote_phase, false);
        expect_tracks(vec!["clear_election_state.end"]);
    }

    #[test]
    fn test_election_completed() {
        let (test, mut server_state) = new();

        let ref mut state = test.state.borrow_mut();
        state.election_completed = false;
        state.pre_vote_phase = true;

        test.election_completed(state, &mut server_state);

        assert_eq!(state.election_completed, true);
        assert_eq!(state.pre_vote_phase, false);
        expect_tracks(vec!["election_completed.election_completed", "election_completed.end"]);
    }

    fn new() -> (ElectionProtocolImpl, ServerState) {
        new_with_setup(Setup::new())
    }

    fn new_with_setup(setup: Setup) -> (ElectionProtocolImpl, ServerState) {
        let test_state = setup.build();
        (
            ElectionProtocolImpl::new(Rc::new(test_state.server), test_state.context),
            test_state.server_state,
        )
    }
}
