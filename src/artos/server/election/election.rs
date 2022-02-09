use crate::artos::common::utils::VecExt;
use crate::artos::message::server::{ServerTransientState, VoteRequest, VoteResponse};
use crate::artos::message::Request;
use crate::artos::server::channel::{Context, Server, ServerRole, ServerState};
use crate::artos::server::membership::{MembershipProtocol, NullMembershipProtocol};
use crate::artos::server::replication::{NullReplicationProtocol, ReplicationProtocol};
use crate::artos::server::utils;
use crate::common::utils::Time;
use crate::compartment::Timer;
use crate::track;
use crate::track_method;
use log::debug;
use log::info;
use log::log_enabled;
use log::Level::Debug;
use rand::Rng;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};
use uuid::Uuid;

include!("election_tests.rs");

//////////////////////////////////////////////////////////
// ElectionProtocol API
//////////////////////////////////////////////////////////

#[cfg_attr(test, mockall::automock)]
pub trait ElectionProtocol {
    fn start(&self, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn stop(&self) {
        unimplemented!()
    }

    fn on_timer(&self, _current_time: Instant, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn delay_election(&self, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn become_follower(&self, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn lock_election(&self) {
        unimplemented!()
    }

    fn handle_vote_request(&self, _request: VoteRequest, _server_state: &mut ServerState) -> VoteResponse {
        unimplemented!()
    }

    fn handle_vote_response(&self, _response: VoteResponse, _server_state: &mut ServerState) {
        unimplemented!()
    }
}

pub struct NullElectionProtocol {}

impl NullElectionProtocol {
    pub fn new() -> Rc<dyn ElectionProtocol> {
        Rc::new(NullElectionProtocol {})
    }
}

impl ElectionProtocol for NullElectionProtocol {}

//////////////////////////////////////////////////////////
// SingleServerElectionProtocolImpl
//////////////////////////////////////////////////////////

struct SingleServerElectionProtocolImpl {
    endpoint: String,
    server: Rc<dyn Server>,
    context: Rc<Context>,
    state: RefCell<SingleServerState>,
}

struct SingleServerState {
    membership_protocol: Rc<dyn MembershipProtocol>,
}

impl SingleServerElectionProtocolImpl {
    fn new(server: Rc<dyn Server>, context: Rc<Context>) -> SingleServerElectionProtocolImpl {
        SingleServerElectionProtocolImpl {
            endpoint: context.local_server().endpoint().clone(),
            server,
            context,
            state: RefCell::new(SingleServerState {
                membership_protocol: NullMembershipProtocol::new(),
            }),
        }
    }

    pub fn set_membership_protocol(&self, value: Rc<dyn MembershipProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.membership_protocol = value;
    }
}

impl ElectionProtocol for SingleServerElectionProtocolImpl {
    fn start(&self, server_state: &mut ServerState) {
        let state = &mut self.state.borrow_mut();
        server_state.role = ServerRole::Leader;
        server_state.leader_id = Some(self.context.local_server().id());

        state.membership_protocol.on_leader(server_state);

        info!(
            "[{}] Server is elected as a leader for term {}.",
            self.endpoint,
            server_state.persistent.term()
        );
    }

    fn stop(&self) {}

    fn on_timer(&self, _current_time: Instant, _server_state: &mut ServerState) {}

    fn delay_election(&self, _server_state: &mut ServerState) {}

    fn become_follower(&self, _server_state: &mut ServerState) {}

    fn lock_election(&self) {}
}

//////////////////////////////////////////////////////////
// ElectionProtocolImpl
//////////////////////////////////////////////////////////

pub struct ElectionProtocolImpl {
    endpoint: String,
    server: Rc<dyn Server>,
    context: Rc<Context>,
    state: RefCell<State>,
}

struct State {
    membership_protocol: Rc<dyn MembershipProtocol>,
    replication_protocol: Rc<dyn ReplicationProtocol>,
    voted_servers: Vec<Uuid>,
    transient_states: Vec<(Uuid, ServerTransientState)>,
    votes_granted: usize,
    election_completed: bool,
    election_timer: Timer,
    next_voting_time: Instant,
    election_locked: bool,
    pre_vote_phase: bool,
}

impl ElectionProtocolImpl {
    pub fn new(server: Rc<dyn Server>, context: Rc<Context>) -> ElectionProtocolImpl {
        ElectionProtocolImpl {
            endpoint: context.local_server().endpoint().clone(),
            server,
            context,
            state: RefCell::new(State {
                membership_protocol: NullMembershipProtocol::new(),
                replication_protocol: NullReplicationProtocol::new(),
                voted_servers: vec![],
                transient_states: vec![],
                votes_granted: 0,
                election_completed: false,
                election_timer: Timer::new(false, Duration::from_secs(0)),
                next_voting_time: Time::now(),
                election_locked: false,
                pre_vote_phase: false,
            }),
        }
    }

    pub fn set_membership_protocol(&self, value: Rc<dyn MembershipProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.membership_protocol = value;
    }

    pub fn set_replication_protocol(&self, value: Rc<dyn ReplicationProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.replication_protocol = value;
    }
}

impl ElectionProtocol for ElectionProtocolImpl {
    fn start(&self, server_state: &mut ServerState) {
        track_method!(t, "start");

        let state = &mut self.state.borrow_mut();
        self.restart_election_timer(state, server_state);

        track!(t, "end");
    }

    fn stop(&self) {
        track_method!(t, "stop");

        let state = &mut self.state.borrow_mut();
        state.election_timer.stop();

        track!(t, "end");
    }

    fn on_timer(&self, current_time: Instant, server_state: &mut ServerState) {
        track_method!(t, "on_timer");

        let state = &mut self.state.borrow_mut();
        if state.election_timer.is_fired(current_time) {
            track!(t, "election_timer_fired");

            self.request_election(state, server_state);
        }

        track!(t, "end");
    }

    fn delay_election(&self, server_state: &mut ServerState) {
        track_method!(t, "delay_election");

        let state = &mut self.state.borrow_mut();

        self.restart_election_timer(state, server_state);

        state.election_locked = false;
        state.next_voting_time = Time::now()
            + self
                .context
                .server_channel_factory_configuration()
                .min_election_timeout();

        track!(t, "end");
    }

    fn become_follower(&self, server_state: &mut ServerState) {
        track_method!(t, "become_follower");

        let state = &mut self.state.borrow_mut();
        self.become_follower_state(state, server_state);

        track!(t, "end");
    }

    fn lock_election(&self) {
        track_method!(t, "lock_election");

        let state = &mut self.state.borrow_mut();
        state.election_locked = true;

        track!(t, "end");
    }

    fn handle_vote_request(&self, request: VoteRequest, server_state: &mut ServerState) -> VoteResponse {
        track_method!(t, "handle_vote_request");

        let mut state = self.state.borrow_mut();

        let accepted = self.is_vote_request_accepted(&request, &mut state, server_state);

        let mut transient_state: Option<ServerTransientState> = None;
        if accepted && !request.is_pre_vote() {
            track!(t, "accepted_vote");

            server_state.persistent.set_voted_for(Some(request.server_id()));

            let transaction = self.context.state_machine().begin_transaction(false);
            transaction.write_state(server_state.persistent);
            transaction.commit();

            transient_state = Some(state.replication_protocol.acquire_transient_state(server_state));
        }

        if log_enabled!(Debug) {
            debug!(
                "[{}] Vote request has been received: {:?}\n    Last entry: {:?}\n    End-log-index: {}, term: {}, accepted: {}.",
                self.endpoint,
                utils::to_string(&request, request.server_id(), &server_state, true),
                self.context.log_store().last(),
                self.context.log_store().end_index(),
                server_state.persistent.term(),
                accepted
            );
        }

        track!(t, "end");

        VoteResponse::new(
            self.context.local_server().id(),
            self.context.group_id(),
            accepted,
            server_state.persistent.term(),
            transient_state,
        )
    }

    fn handle_vote_response(&self, response: VoteResponse, server_state: &mut ServerState) {
        track_method!(t, "handle_vote_response");

        if response.group_id() != server_state.configuration.id() {
            track!(t, "invalid_group");
            return;
        }

        let state = &mut self.state.borrow_mut();

        if state.election_completed {
            track!(t, "election_completed");
            return;
        }

        if state.voted_servers.contains(&response.server_id()) {
            track!(t, "already_voted");
            return;
        }

        state.voted_servers.push(response.server_id());

        if response.is_accepted() {
            track!(t, "accepted");

            state.votes_granted += 1;

            if !state.pre_vote_phase {
                track!(t, "apply_transient_state");

                if let Some(transient_state) = response.transient_state() {
                    state.transient_states.update_or_insert(
                        |v| v.0 == response.server_id(),
                        (response.server_id(), transient_state.clone()),
                    );
                }
            }
        }

        if log_enabled!(Debug) {
            debug!(
                "[{}] Vote response has been received: {:?}. Accepted: {}, votes granted: {}, voted servers: {}.",
                self.endpoint,
                utils::to_string(&response, response.server_id(), &server_state, false),
                response.is_accepted(),
                state.votes_granted,
                state.voted_servers.len()
            );
        }

        if state.voted_servers.len() >= server_state.peers.len() + 1 {
            track!(t, "election_completed");

            self.election_completed(state, server_state);
        }

        if state.votes_granted > (server_state.peers.len() + 1) / 2 {
            if state.pre_vote_phase {
                track!(t, "granted_pre_vote");

                self.become_candidate(state, server_state);
            } else {
                track!(t, "granted_vote");

                self.become_leader(state, server_state);
            }
        }

        track!(t, "end");
    }
}

impl ElectionProtocolImpl {
    fn is_vote_request_accepted(
        &self,
        request: &VoteRequest,
        state: &mut State,
        server_state: &mut ServerState,
    ) -> bool {
        track_method!(t, "is_vote_request_accepted");

        let current_time = Time::now();

        if current_time < state.next_voting_time {
            track!(t, "rejected_by_voting_time");
            return false;
        }

        if server_state
            .configuration
            .servers()
            .iter()
            .find(|v| v.id() == request.server_id())
            .is_none()
        {
            track!(t, "rejected_by_unknown_server");
            return false;
        }

        let pre_vote = request.is_pre_vote();
        if !pre_vote {
            if utils::update_term(&self.context, server_state, request.term()) {
                track!(t, "vote_term_updated");

                self.become_follower_state(state, server_state);
            }
        }

        let request_log_newer_or_equal = request.last_log_term() > self.context.log_store().last().term()
            || (request.last_log_term() == self.context.log_store().last().term()
                && request.last_log_index() >= self.context.log_store().end_index() - 1);

        let accepted = request.term() == server_state.persistent.term()
            && request_log_newer_or_equal
            && (pre_vote
                || server_state.persistent.voted_for().is_none()
                || request.server_id() == server_state.persistent.voted_for().unwrap());

        track!(t, "end");

        accepted
    }

    fn become_follower_state(&self, state: &mut State, server_state: &mut ServerState) {
        track_method!(t, "become_follower_state");

        self.clear_election_state(state);

        server_state.role = ServerRole::Follower;

        self.restart_election_timer(state, server_state);

        state.membership_protocol.on_follower(server_state);

        info!(
            "[{}] Server is set as a follower for term {}.",
            self.endpoint,
            server_state.persistent.term()
        );

        track!(t, "end");
    }

    fn restart_election_timer(&self, state: &mut State, server_state: &ServerState) {
        track_method!(t, "restart_election_timer");

        if server_state.catching_up {
            track!(t, "catching_up");
            return;
        }

        let factory_configuration = self.context.server_channel_factory_configuration();
        let mut random = rand::thread_rng();

        let delta = factory_configuration.max_election_timeout() - factory_configuration.min_election_timeout();
        let random_delta = random.gen_range(0, delta.as_millis());

        let election_timeout =
            Duration::from_millis((factory_configuration.min_election_timeout().as_millis() + random_delta) as u64);

        state.election_timer.set_period(election_timeout);
        state.election_timer.restart();

        track!(t, "end");
    }

    fn request_election(&self, state: &mut State, server_state: &mut ServerState) {
        track_method!(t, "request_election");

        assert_ne!(server_state.role, ServerRole::Leader);

        if server_state.catching_up {
            track!(t, "catching_up");
            return;
        }

        if state.election_locked {
            track!(t, "election_locked");
            return;
        }

        self.request_pre_vote_for_self(state, server_state);

        track!(t, "end");
    }

    fn become_leader(&self, state: &mut State, server_state: &mut ServerState) {
        track_method!(t, "become_leader");

        self.election_completed(state, server_state);

        state.election_timer.stop();

        server_state.role = ServerRole::Leader;
        server_state.leader_id = Some(self.context.local_server().id());

        state
            .replication_protocol
            .apply_transient_state(std::mem::replace(&mut state.transient_states, vec![]), server_state);

        state.membership_protocol.on_leader(server_state);

        info!(
            "[{}] Server is elected as a leader for term {}.",
            self.endpoint,
            server_state.persistent.term()
        );

        track!(t, "end");
    }

    fn become_candidate(&self, state: &mut State, server_state: &mut ServerState) {
        track_method!(t, "become_candidate");

        if server_state.role != ServerRole::Candidate {
            track!(t, "state_follower");

            server_state.persistent.increase_term();
            server_state.persistent.set_voted_for(None);
            server_state.role = ServerRole::Candidate;

            let transaction = self.context.state_machine().begin_transaction(false);
            transaction.write_state(server_state.persistent);
            transaction.commit();

            debug!(
                "[{}] Election has been started. Server is set as candidate for term {}.",
                self.endpoint,
                server_state.persistent.term()
            );

            self.request_vote_for_self(state, server_state);
        }

        track!(t, "end");
    }

    fn request_pre_vote_for_self(&self, state: &mut State, server_state: &mut ServerState) {
        track_method!(t, "request_pre_vote_for_self");

        self.restart_election_timer(state, server_state);
        self.clear_election_state(state);

        debug!(
            "[{}] Request pre-vote for self is started for term {}.",
            self.endpoint,
            server_state.persistent.term()
        );

        if server_state.peers.is_empty() {
            track!(t, "single_server");

            self.become_candidate(state, server_state);
            return;
        }

        state.pre_vote_phase = true;

        self.request_votes(state, server_state, true);

        track!(t, "end");
    }

    fn request_vote_for_self(&self, state: &mut State, server_state: &mut ServerState) {
        track_method!(t, "request_vote_for_self");

        self.restart_election_timer(state, server_state);
        self.clear_election_state(state);

        debug!(
            "[{}] Request vote for self is started for term {}.",
            self.endpoint,
            server_state.persistent.term()
        );

        if server_state.peers.is_empty() {
            track!(t, "single_server");

            self.become_leader(state, server_state);
            return;
        }

        server_state
            .persistent
            .set_voted_for(Some(self.context.local_server().id()));

        let transaction = self.context.state_machine().begin_transaction(false);
        transaction.write_state(server_state.persistent);
        transaction.commit();

        self.request_votes(state, server_state, false);

        track!(t, "end");
    }

    fn request_votes(&self, state: &mut State, server_state: &mut ServerState, pre_vote: bool) {
        track_method!(t, "request_votes");

        state.votes_granted = 1;
        state.voted_servers.push(self.context.local_server().id());

        for peer in &mut server_state.peers {
            let request = VoteRequest::new(
                self.context.local_server().id(),
                server_state.configuration.id(),
                server_state.persistent.term(),
                self.context.log_store().last().term(),
                self.context.log_store().end_index() - 1,
                pre_vote,
            );

            self.server.send(peer, Request::Vote(request));
        }

        track!(t, "end");
    }

    fn clear_election_state(&self, state: &mut State) {
        track_method!(t, "clear_election_state");

        state.election_completed = false;
        state.votes_granted = 0;
        state.voted_servers.clear();
        state.transient_states.clear();
        state.pre_vote_phase = false;

        track!(t, "end");
    }

    fn election_completed(&self, state: &mut State, server_state: &ServerState) {
        track_method!(t, "election_completed");

        state.pre_vote_phase = false;

        if !state.election_completed {
            state.election_completed = true;

            track!(t, "election_completed");

            debug!(
                "[{}] Election has been completed for term {}.",
                self.endpoint,
                server_state.persistent.term()
            );
        }

        track!(t, "end");
    }
}
