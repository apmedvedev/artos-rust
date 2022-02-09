use crate::artos::common::{Error, ErrorKind};
use crate::artos::common::{FlowController, FlowType};
use crate::artos::message::server::{LogEntry, LogValueType, NULL_UUID};
use crate::artos::server::channel::{Context, ServerRole, ServerState};
use crate::artos::server::replication::{NullReplicationProtocol, ReplicationProtocol};
use crate::artos::server::utils::CompletionHandler;
use crate::compartment::Timer;
use crate::track;
use crate::track_method;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Instant;

include!("leader_tests.rs");

//////////////////////////////////////////////////////////
// LeaderClientProtocol API
//////////////////////////////////////////////////////////

#[cfg_attr(test, mockall::automock)]
pub trait LeaderClientProtocol {
    fn on_timer(&self, _current_time: Instant, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn publish(&self, _value: Vec<u8>, _user_data: usize, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn on_leader(&self, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn on_follower(&self) {
        unimplemented!()
    }
}

pub struct NullLeaderClientProtocol {}

impl NullLeaderClientProtocol {
    pub fn new() -> Rc<dyn LeaderClientProtocol> {
        Rc::new(NullLeaderClientProtocol {})
    }
}

impl LeaderClientProtocol for NullLeaderClientProtocol {}

//////////////////////////////////////////////////////////
// LeaderClientProtocolImpl
//////////////////////////////////////////////////////////

pub struct LeaderClientProtocolImpl {
    endpoint: String,
    max_value_size: usize,
    lock_queue_capacity: usize,
    unlock_queue_capacity: usize,
    context: Rc<Context>,
    handler: Rc<dyn CompletionHandler<usize, Error>>,
    flow_controller: Rc<dyn FlowController>,
    state: RefCell<State>,
}

struct State {
    replication_protocol: Rc<dyn ReplicationProtocol>,
    commit_timer: Timer,
    queue: VecDeque<EntryInfo>,
    next_message_id: u64,
    flow_locked: bool,
}

struct EntryInfo {
    log_entry: LogEntry,
    user_data: usize,
}

impl LeaderClientProtocolImpl {
    pub fn new(
        context: Rc<Context>,
        handler: Rc<dyn CompletionHandler<usize, Error>>,
        flow_controller: Rc<dyn FlowController>,
    ) -> LeaderClientProtocolImpl {
        let commit_period = context.server_channel_factory_configuration().commit_period();
        LeaderClientProtocolImpl {
            endpoint: context.local_server().endpoint().clone(),
            max_value_size: context.server_channel_factory_configuration().max_value_size(),
            lock_queue_capacity: context.server_channel_factory_configuration().lock_queue_capacity(),
            unlock_queue_capacity: context.server_channel_factory_configuration().unlock_queue_capacity(),
            context,
            handler,
            flow_controller,
            state: RefCell::new(State {
                replication_protocol: NullReplicationProtocol::new(),
                commit_timer: Timer::new(false, commit_period),
                queue: VecDeque::new(),
                next_message_id: 1,
                flow_locked: false,
            }),
        }
    }

    pub fn set_replication_protocol(&self, value: Rc<dyn ReplicationProtocol>) {
        let state = &mut self.state.borrow_mut();
        state.replication_protocol = value;
    }
}

impl LeaderClientProtocol for LeaderClientProtocolImpl {
    fn on_timer(&self, current_time: Instant, server_state: &mut ServerState) {
        track_method!(t, "on_timer");

        let state = &mut self.state.borrow_mut();
        if state.commit_timer.is_fired(current_time) {
            track!(t, "commit_timer_fired");

            self.handle_commit(state, server_state);
        }

        track!(t, "end");
    }

    fn publish(&self, value: Vec<u8>, user_data: usize, server_state: &mut ServerState) {
        track_method!(t, "publish");

        if value.len() > self.max_value_size {
            track!(t, "max_value_exceeded");

            let error = Box::new(ErrorKind::MaxValueSizeExceeded(value.len()));
            self.handler.on_failed(error);
            return;
        }

        let state = &mut self.state.borrow_mut();
        if server_state.role != ServerRole::Leader {
            track!(t, "not_leader");
            let error = Box::new(ErrorKind::ServerNotLeader(self.context.local_server().id()));
            self.handler.on_failed(error);
            return;
        }

        let log_entry = LogEntry::new(0, value, LogValueType::Application, NULL_UUID, state.next_message_id);
        state.next_message_id += 1;

        state.queue.push_back(EntryInfo {
            log_entry: log_entry.clone(),
            user_data,
        });

        self.check_flow(state);

        let last_committed_message_id = state.replication_protocol.publish(log_entry, server_state);
        if last_committed_message_id != 0 {
            track!(t, "remove_committed");

            self.remove_committed_entries(last_committed_message_id, state);
        }

        track!(t, "end");
    }

    fn on_leader(&self, server_state: &mut ServerState) {
        track_method!(t, "on_leader");

        let state = &mut self.state.borrow_mut();
        state.commit_timer.start();
        state.next_message_id = state.replication_protocol.last_received_message_id(server_state) + 1;

        track!(t, "end");
    }

    fn on_follower(&self) {
        track_method!(t, "on_follower");

        let state = &mut self.state.borrow_mut();
        state.commit_timer.stop();
        self.clear(state);

        track!(t, "end");
    }
}

impl LeaderClientProtocolImpl {
    fn handle_commit(&self, state: &mut State, server_state: &mut ServerState) {
        track_method!(t, "handle_commit");

        let last_committed_message_id = state.replication_protocol.last_committed_message_id(server_state);
        if last_committed_message_id != 0 {
            track!(t, "remove_committed");

            self.remove_committed_entries(last_committed_message_id, state);
        }

        track!(t, "end");
    }

    fn remove_committed_entries(&self, last_committed_message_id: u64, state: &mut State) {
        track_method!(t, "remove_committed_entries");

        loop {
            if let Some(info) = state.queue.front() {
                if info.log_entry.message_id() <= last_committed_message_id {
                    let user_data = info.user_data;
                    state.queue.pop_front();

                    self.handler.on_succeeded(user_data);

                    self.check_flow(state);
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        track!(t, "end");
    }

    fn check_flow(&self, state: &mut State) {
        track_method!(t, "check_flow");

        if !state.flow_locked && state.queue.len() >= self.lock_queue_capacity {
            track!(t, "flow_locked");

            state.flow_locked = true;
            self.flow_controller.lock_flow(FlowType::Publish);
        }

        if state.flow_locked && state.queue.len() <= self.unlock_queue_capacity {
            track!(t, "flow_unlocked");

            state.flow_locked = false;
            self.flow_controller.unlock_flow(FlowType::Publish);
        }

        track!(t, "end");
    }

    fn clear(&self, state: &mut State) {
        track_method!(t, "clear");

        state.next_message_id = 1;

        loop {
            if let Some(_) = state.queue.pop_front() {
                let error = Box::new(ErrorKind::ServerNotLeader(self.context.local_server().id()));
                self.handler.on_failed(error);
            } else {
                break;
            }
        }

        self.check_flow(state);

        track!(t, "end");
    }
}
