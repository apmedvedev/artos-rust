use crate::artos::common::GroupConfiguration;
use crate::artos::server::api::{MembershipChange, MembershipChangeEvent, MembershipListener, MembershipService};
use crate::artos::server::channel::{Context, Server, ServerRole, ServerState};
use crate::artos::server::client::{LeaderClientProtocol, NullLeaderClientProtocol};
use crate::artos::server::membership::{
    JoinGroupProtocol, LeaveGroupProtocol, NullJoinGroupProtocol, NullLeaveGroupProtocol,
};
use crate::artos::server::replication::{
    NullQueryProtocol, NullReplicationProtocol, QueryProtocol, ReplicationProtocol,
};
use crate::artos::server::state::{NullStateTransferProtocol, StateTransferProtocol};
use crate::track;
use crate::track_method;
use log::info;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

include!("membership_tests.rs");

//////////////////////////////////////////////////////////
// MembershipProtocol API
//////////////////////////////////////////////////////////

#[cfg_attr(test, mockall::automock)]
pub trait MembershipProtocol {
    fn on_leader(&self, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn on_follower(&self, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn reconfigure(&self, _new_configuration: GroupConfiguration, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn commit_configuration(&self, _new_configuration: GroupConfiguration, _server_state: &mut ServerState) {
        unimplemented!()
    }
}

pub struct NullMembershipProtocol {}

impl NullMembershipProtocol {
    pub fn new() -> Rc<dyn MembershipProtocol> {
        Rc::new(NullMembershipProtocol {})
    }
}

impl MembershipProtocol for NullMembershipProtocol {}

//////////////////////////////////////////////////////////
// MembershipProtocolImpl
//////////////////////////////////////////////////////////

pub struct MembershipProtocolImpl {
    endpoint: String,
    server: Rc<dyn Server>,
    context: Rc<Context>,
    state: RefCell<State>,
}

struct State {
    replication_protocol: Rc<dyn ReplicationProtocol>,
    join_group_protocol: Rc<dyn JoinGroupProtocol>,
    leave_group_protocol: Rc<dyn LeaveGroupProtocol>,
    state_transfer_protocol: Rc<dyn StateTransferProtocol>,
    leader_client_protocol: Rc<dyn LeaderClientProtocol>,
    query_protocol: Rc<dyn QueryProtocol>,
    membership_service: MembershipServiceImpl,
}

impl MembershipProtocolImpl {
    pub fn new(server: Rc<dyn Server>, context: Rc<Context>) -> MembershipProtocolImpl {
        let endpoint = context.local_server().endpoint().clone();
        MembershipProtocolImpl {
            endpoint: endpoint.clone(),
            server,
            context,
            state: RefCell::new(State {
                replication_protocol: NullReplicationProtocol::new(),
                join_group_protocol: NullJoinGroupProtocol::new(),
                leave_group_protocol: NullLeaveGroupProtocol::new(),
                state_transfer_protocol: NullStateTransferProtocol::new(),
                leader_client_protocol: NullLeaderClientProtocol::new(),
                query_protocol: NullQueryProtocol::new(),
                membership_service: MembershipServiceImpl::new(endpoint.clone()),
            }),
        }
    }

    pub fn set_replication_protocol(&self, value: Rc<dyn ReplicationProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.replication_protocol = value;
    }

    pub fn set_join_group_protocol(&self, value: Rc<dyn JoinGroupProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.join_group_protocol = value;
    }

    pub fn set_leave_group_protocol(&self, value: Rc<dyn LeaveGroupProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.leave_group_protocol = value;
    }

    pub fn set_state_transfer_protocol(&self, value: Rc<dyn StateTransferProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.state_transfer_protocol = value;
    }

    pub fn set_leader_client_protocol(&self, value: Rc<dyn LeaderClientProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.leader_client_protocol = value;
    }

    pub fn set_query_protocol(&self, value: Rc<dyn QueryProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.query_protocol = value;
    }
}

impl MembershipProtocol for MembershipProtocolImpl {
    fn on_leader(&self, server_state: &mut ServerState) {
        track_method!(t, "on_leader");

        let state = &mut self.state.borrow_mut();
        state.replication_protocol.on_leader(server_state);
        state.leave_group_protocol.on_leader();
        state.join_group_protocol.on_leader();
        state.state_transfer_protocol.on_leader(server_state);
        state.membership_service.on_leader();
        state.leader_client_protocol.on_leader(server_state);

        track!(t, "end");
    }

    fn on_follower(&self, server_state: &mut ServerState) {
        track_method!(t, "on_follower");

        let state = &mut self.state.borrow_mut();
        state.replication_protocol.on_follower(server_state);
        state.leave_group_protocol.on_follower();
        state.join_group_protocol.on_follower();
        state.state_transfer_protocol.on_follower(server_state);
        state.membership_service.on_follower();
        state.leader_client_protocol.on_follower();
        state.query_protocol.on_follower();

        track!(t, "end");
    }

    fn reconfigure(&self, new_configuration: GroupConfiguration, server_state: &mut ServerState) {
        let state = self.state.borrow();
        self.reconfigure_state(new_configuration, &state, server_state);
    }

    fn commit_configuration(&self, new_configuration: GroupConfiguration, server_state: &mut ServerState) {
        track_method!(t, "commit_configuration");

        let state = self.state.borrow();
        self.reconfigure_state(new_configuration.clone(), &state, server_state);

        if server_state.catching_up
            && new_configuration
                .servers()
                .iter()
                .find(|x| x.id() == self.context.local_server().id())
                .is_some()
        {
            track!(t, "joined");

            info!("[{}] Current server has joined the group.", self.endpoint);

            server_state.catching_up = false;
            self.on_joined(&state);
        }

        track!(t, "end");
    }
}

impl MembershipProtocolImpl {
    fn reconfigure_state(&self, new_configuration: GroupConfiguration, state: &State, server_state: &mut ServerState) {
        track_method!(t, "reconfigure_state");

        info!(
            "[{}] Configuration is changed. \nOld configuration: {:?}\nNew configuration: {:?}.",
            self.endpoint, server_state.configuration, new_configuration
        );

        server_state.configuration_changing = false;

        let old_configuration = server_state.configuration.clone();
        assert_eq!(new_configuration.id(), old_configuration.id());

        let mut removed_servers = vec![];
        for server in old_configuration.servers() {
            if new_configuration
                .servers()
                .iter()
                .find(|c| c.id() == server.id())
                .is_none()
            {
                removed_servers.push(server.id());
            }
        }

        let mut added_servers = vec![];
        for server in new_configuration.servers() {
            if old_configuration
                .servers()
                .iter()
                .find(|c| c.id() == server.id())
                .is_none()
            {
                added_servers.push(server.clone());
            }
        }

        for server_configuration in &added_servers {
            if server_configuration.id() == self.context.local_server().id() {
                continue;
            }

            let peer = self.server.create_peer(server_configuration.clone());
            peer.set_next_log_index(self.context.log_store().end_index());

            if server_state.role == ServerRole::Leader {
                peer.start();
            }

            server_state.peers.push(peer);

            info!(
                "[{}] Server {:?} has been added to group.",
                self.endpoint, server_configuration
            );
        }

        for id in &removed_servers {
            if *id == self.context.local_server().id() && !server_state.catching_up {
                info!("[{}] Current server has left the group.", self.endpoint);

                self.on_left(&state);
            } else {
                if let Some(index) = server_state.peers.iter().position(|x| x.configuration().id() == *id) {
                    let peer = &server_state.peers[index];
                    peer.stop();

                    info!(
                        "[{}] Server {:?} has been removed from group.",
                        self.endpoint,
                        peer.configuration()
                    );

                    server_state.peers.remove(index);
                }
            }
        }

        server_state.configuration = new_configuration.clone();

        let change = MembershipChange::new(added_servers, removed_servers);
        let event = MembershipChangeEvent::new(old_configuration, new_configuration, change);
        self.on_membership_changed(&state, event);

        track!(t, "end");
    }

    fn on_joined(&self, state: &State) {
        track_method!(t, "on_joined");

        state.join_group_protocol.on_joined();
        state.membership_service.on_joined();

        track!(t, "end");
    }

    fn on_left(&self, state: &State) {
        track_method!(t, "on_left");

        state.leave_group_protocol.on_left();
        state.membership_service.on_left();
        state.replication_protocol.on_left();

        track!(t, "end");
    }

    fn on_membership_changed(&self, state: &State, event: MembershipChangeEvent) {
        track_method!(t, "on_membership_changed");

        state.membership_service.on_membership_changed(event);

        track!(t, "end");
    }
}
//////////////////////////////////////////////////////////
// MembershipServiceImpl
//////////////////////////////////////////////////////////

struct MembershipServiceImpl {
    endpoint: String,
    state: RefCell<MembershipServiceState>,
}

struct MembershipServiceState {
    listeners: Vec<Arc<dyn MembershipListener>>,
}

impl MembershipService for MembershipServiceImpl {
    fn add_listener(&self, listener: Arc<dyn MembershipListener>) {
        let mut state = self.state.borrow_mut();
        state.listeners.push(listener);
    }

    fn remove_all_listeners(&self) {
        let mut state = self.state.borrow_mut();
        state.listeners.clear();
    }
}

impl MembershipServiceImpl {
    fn new(endpoint: String) -> MembershipServiceImpl {
        MembershipServiceImpl {
            endpoint,
            state: RefCell::new(MembershipServiceState { listeners: vec![] }),
        }
    }

    fn on_leader(&self) {
        let state = self.state.borrow();
        for listener in &state.listeners {
            listener.on_leader();
        }
    }

    fn on_follower(&self) {
        let state = self.state.borrow();
        for listener in &state.listeners {
            listener.on_follower();
        }
    }

    fn on_joined(&self) {
        let state = self.state.borrow();
        for listener in &state.listeners {
            listener.on_joined();
        }
    }

    fn on_left(&self) {
        let state = self.state.borrow();
        for listener in &state.listeners {
            listener.on_left();
        }
    }

    fn on_membership_changed(&self, event: MembershipChangeEvent) {
        let state = self.state.borrow();
        for listener in &state.listeners {
            listener.on_membership_changed(&event);
        }
    }
}
