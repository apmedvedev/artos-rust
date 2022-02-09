#[cfg(test)]
mod tests {
    use super::*;
    use crate::artos::common::ServerConfiguration;
    use crate::artos::server::channel::{ServerPeer, ServerState};
    use crate::artos::server::test::{
        JoinGroupProtocolMockBuilder, LeaderClientProtocolMockBuilder, LeaveGroupProtocolMockBuilder,
        MembershipListenerMockBuilder, QueryProtocolMockBuilder, ReplicationProtocolMockBuilder, ServerMockBuilder,
        Setup, StateTransferProtocolMockBuilder,
    };
    use crate::common::utils::expect_tracks;
    use pretty_assertions::assert_eq;
    use uuid::Uuid;

    #[test]
    fn test_on_leader() {
        let (test, mut server_state) = new();

        test.set_replication_protocol(ReplicationProtocolMockBuilder::new().on_leader().build());
        test.set_leave_group_protocol(LeaveGroupProtocolMockBuilder::new().on_leader().build());
        test.set_join_group_protocol(JoinGroupProtocolMockBuilder::new().on_leader().build());
        test.set_state_transfer_protocol(StateTransferProtocolMockBuilder::new().on_leader().build());
        test.set_leader_client_protocol(LeaderClientProtocolMockBuilder::new().on_leader().build());
        {
            let state = test.state.borrow_mut();
            state
                .membership_service
                .add_listener(MembershipListenerMockBuilder::new().on_leader().build());
        }

        test.on_leader(&mut server_state);
        expect_tracks(vec!["on_leader.end"]);
    }

    #[test]
    fn test_on_follower() {
        let (test, mut server_state) = new();

        test.set_replication_protocol(ReplicationProtocolMockBuilder::new().on_follower().build());
        test.set_leave_group_protocol(LeaveGroupProtocolMockBuilder::new().on_follower().build());
        test.set_join_group_protocol(JoinGroupProtocolMockBuilder::new().on_follower().build());
        test.set_state_transfer_protocol(StateTransferProtocolMockBuilder::new().on_follower().build());
        test.set_leader_client_protocol(LeaderClientProtocolMockBuilder::new().on_follower().build());
        test.set_query_protocol(QueryProtocolMockBuilder::new().on_follower().build());
        {
            let state = test.state.borrow_mut();
            state
                .membership_service
                .add_listener(MembershipListenerMockBuilder::new().on_follower().build());
        }

        test.on_follower(&mut server_state);
        expect_tracks(vec!["on_follower.end"]);
    }

    #[test]
    fn test_commit_configuration() {
        let setup = Setup::new();

        let new_membership = setup.configuration.group.build();

        let (test, mut server_state) = new_with_setup(setup);

        test.set_join_group_protocol(JoinGroupProtocolMockBuilder::new().on_joined().build());
        server_state.catching_up = true;

        test.commit_configuration(new_membership, &mut server_state);

        assert_eq!(server_state.catching_up, false);
        expect_tracks(vec![
            "on_membership_changed.end",
            "reconfigure_state.end",
            "commit_configuration.joined",
            "on_joined.end",
            "commit_configuration.end",
        ]);
    }

    #[test]
    fn test_reconfigure_state() {
        let mut setup = Setup::new();

        let new_server1 = ServerConfiguration::new(Uuid::new_v4(), String::from("endpoint11"));
        let new_server2 = ServerConfiguration::new(Uuid::new_v4(), String::from("endpoint12"));
        let old_membership = setup.configuration.group.build();
        let new_membership = GroupConfiguration::new(
            old_membership.name().clone(),
            old_membership.id(),
            vec![
                old_membership.servers()[2].clone(),
                new_server1.clone(),
                new_server2.clone(),
            ],
            false,
        );

        let end_index = 10;
        setup.log_store.end_index(end_index);

        let (test, mut server_state) = new_with_setup_peers(setup, vec![new_server1.clone(), new_server2.clone()]);

        test.set_replication_protocol(ReplicationProtocolMockBuilder::new().on_left().build());
        test.set_leave_group_protocol(LeaveGroupProtocolMockBuilder::new().on_left().build());
        server_state.configuration_changing = true;
        let state = test.state.borrow();

        server_state.role = ServerRole::Leader;

        test.reconfigure_state(new_membership.clone(), &state, &mut server_state);

        assert_eq!(server_state.peers.len(), 3);
        assert_eq!(server_state.peers[1].configuration(), &new_server1);
        assert_eq!(server_state.peers[1].is_started(), true);
        assert_eq!(server_state.peers[1].next_log_index(), end_index);
        assert_eq!(server_state.peers[2].configuration(), &new_server2);
        assert_eq!(server_state.peers[2].is_started(), true);
        assert_eq!(server_state.peers[2].next_log_index(), end_index);
        assert_eq!(server_state.configuration_changing, false);
        assert_eq!(server_state.configuration, new_membership);
        expect_tracks(vec![
            "resume_heartbeating.end",
            "update_next_heartbeat_time.end",
            "start.end",
            "resume_heartbeating.end",
            "update_next_heartbeat_time.end",
            "start.end",
            "on_left.end",
            "on_peer_disconnected.already_disconnected",
            "stop.end",
            "on_membership_changed.end",
            "reconfigure_state.end",
        ]);
    }

    #[test]
    fn test_on_joined() {
        let (test, _) = new();

        test.set_join_group_protocol(JoinGroupProtocolMockBuilder::new().on_joined().build());
        let state = test.state.borrow_mut();
        state
            .membership_service
            .add_listener(MembershipListenerMockBuilder::new().on_joined().build());

        test.on_joined(&state);
        expect_tracks(vec!["on_joined.end"]);
    }

    #[test]
    fn test_on_left() {
        let (test, _) = new();

        test.set_replication_protocol(ReplicationProtocolMockBuilder::new().on_left().build());
        test.set_leave_group_protocol(LeaveGroupProtocolMockBuilder::new().on_left().build());
        let state = test.state.borrow_mut();
        state
            .membership_service
            .add_listener(MembershipListenerMockBuilder::new().on_left().build());

        test.on_left(&state);
        expect_tracks(vec!["on_left.end"]);
    }

    #[test]
    fn test_on_membership_changed() {
        let setup = Setup::new();

        let old_membership = setup.configuration.group.build();
        let new_membership = setup.configuration.group.build();

        let (test, _) = new_with_setup(setup);

        let state = test.state.borrow_mut();
        state
            .membership_service
            .add_listener(MembershipListenerMockBuilder::new().on_membership_changed().build());

        let event = MembershipChangeEvent::new(old_membership, new_membership, MembershipChange::new(vec![], vec![]));

        test.on_membership_changed(&state, event);
        expect_tracks(vec!["on_membership_changed.end"]);
    }

    fn new() -> (MembershipProtocolImpl, ServerState) {
        new_with_setup(Setup::new())
    }

    fn new_with_setup(setup: Setup) -> (MembershipProtocolImpl, ServerState) {
        let test_state = setup.build();
        (
            MembershipProtocolImpl::new(Rc::new(test_state.server), test_state.context),
            test_state.server_state,
        )
    }

    fn new_with_setup_peers(setup: Setup, servers: Vec<ServerConfiguration>) -> (MembershipProtocolImpl, ServerState) {
        let (sender, _) = setup.sender.build();
        let mut test_state = setup.build();

        let mut peers = vec![];
        for server in servers {
            peers.push(ServerPeer::new(
                String::from("test"),
                server,
                test_state.context.clone(),
                sender.clone(),
            ));
        }
        ServerMockBuilder::create_peers_mock(&mut test_state.server, peers);

        (
            MembershipProtocolImpl::new(Rc::new(test_state.server), test_state.context),
            test_state.server_state,
        )
    }
}
