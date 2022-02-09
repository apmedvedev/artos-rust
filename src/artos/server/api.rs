use crate::artos::common::Result;
use crate::artos::common::{GroupConfiguration, MessageSenderConfiguration, ResponseHandler, ServerConfiguration};
use crate::artos::message::server::LogEntry;
use serde::{Deserialize, Serialize};
use std::cmp::max;
use std::fmt::Debug;
use std::io::{Read, Write};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

//////////////////////////////////////////////////////////
// ServerChannelFactoryConfiguration
//////////////////////////////////////////////////////////

#[derive(PartialEq, Debug, Clone)]
pub struct ServerChannelFactoryConfiguration {
    timer_period: Duration,
    max_election_timeout: Duration,
    min_election_timeout: Duration,
    heartbeat_period: Duration,
    peer_timeout: Duration,
    send_failure_backoff: Duration,
    max_publishing_log_entry_count: u64,
    min_state_transfer_gap_log_entry_count: u64,
    client_session_timeout: Duration,
    join_group_period: Duration,
    leave_group_period: Duration,
    leave_group_attempt_count: u32,
    auto_leave_group_timeout: Duration,
    max_value_size: usize,
    lock_queue_capacity: usize,
    unlock_queue_capacity: usize,
    commit_period: Duration,
    subscription_timeout: Duration,
    state_transfer_use_compression: bool,
    state_transfer_max_size: u64,
}

impl ServerChannelFactoryConfiguration {
    pub fn new() -> ServerChannelFactoryConfiguration {
        Default::default()
    }

    pub fn timer_period(&self) -> Duration {
        self.timer_period
    }

    pub fn set_timer_period(&mut self, value: Duration) -> &mut ServerChannelFactoryConfiguration {
        self.timer_period = value;
        self
    }

    pub fn max_election_timeout(&self) -> Duration {
        self.max_election_timeout
    }

    pub fn set_max_election_timeout(&mut self, value: Duration) -> &mut ServerChannelFactoryConfiguration {
        self.max_election_timeout = value;
        self
    }

    pub fn min_election_timeout(&self) -> Duration {
        self.min_election_timeout
    }

    pub fn set_min_election_timeout(&mut self, value: Duration) -> &mut ServerChannelFactoryConfiguration {
        self.min_election_timeout = value;
        self
    }

    pub fn heartbeat_period(&self) -> Duration {
        self.heartbeat_period
    }

    pub fn set_heartbeat_period(&mut self, value: Duration) -> &mut ServerChannelFactoryConfiguration {
        self.heartbeat_period = value;
        self
    }

    pub fn max_heartbeat_period(&self) -> Duration {
        max(
            self.heartbeat_period,
            self.min_election_timeout - self.heartbeat_period / 2,
        )
    }

    pub fn peer_timeout(&self) -> Duration {
        self.peer_timeout
    }

    pub fn set_peer_timeout(&mut self, value: Duration) -> &mut ServerChannelFactoryConfiguration {
        self.peer_timeout = value;
        self
    }

    pub fn send_failure_backoff(&self) -> Duration {
        self.send_failure_backoff
    }

    pub fn set_send_failure_backoff(&mut self, value: Duration) -> &mut ServerChannelFactoryConfiguration {
        self.send_failure_backoff = value;
        self
    }

    pub fn max_publishing_log_entry_count(&self) -> u64 {
        self.max_publishing_log_entry_count
    }

    pub fn set_max_publishing_log_entry_count(&mut self, value: u64) -> &mut ServerChannelFactoryConfiguration {
        self.max_publishing_log_entry_count = value;
        self
    }

    pub fn min_state_transfer_gap_log_entry_count(&self) -> u64 {
        self.min_state_transfer_gap_log_entry_count
    }

    pub fn set_min_state_transfer_gap_log_entry_count(&mut self, value: u64) -> &mut ServerChannelFactoryConfiguration {
        self.min_state_transfer_gap_log_entry_count = value;
        self
    }

    pub fn client_session_timeout(&self) -> Duration {
        self.client_session_timeout
    }

    pub fn set_client_session_timeout(&mut self, value: Duration) -> &mut ServerChannelFactoryConfiguration {
        self.client_session_timeout = value;
        self
    }

    pub fn join_group_period(&self) -> Duration {
        self.join_group_period
    }

    pub fn set_join_group_period(&mut self, value: Duration) -> &mut ServerChannelFactoryConfiguration {
        self.join_group_period = value;
        self
    }

    pub fn leave_group_period(&self) -> Duration {
        self.leave_group_period
    }

    pub fn set_leave_group_period(&mut self, value: Duration) -> &mut ServerChannelFactoryConfiguration {
        self.leave_group_period = value;
        self
    }

    pub fn leave_group_attempt_count(&self) -> u32 {
        self.leave_group_attempt_count
    }

    pub fn set_leave_group_attempt_count(&mut self, value: u32) -> &mut ServerChannelFactoryConfiguration {
        self.leave_group_attempt_count = value;
        self
    }

    pub fn auto_leave_group_timeout(&self) -> Duration {
        self.auto_leave_group_timeout
    }

    pub fn set_auto_leave_group_timeout(&mut self, value: Duration) -> &mut ServerChannelFactoryConfiguration {
        self.auto_leave_group_timeout = value;
        self
    }

    pub fn max_value_size(&self) -> usize {
        self.max_value_size
    }

    pub fn set_max_value_size(&mut self, value: usize) -> &mut ServerChannelFactoryConfiguration {
        self.max_value_size = value;
        self
    }

    pub fn lock_queue_capacity(&self) -> usize {
        self.lock_queue_capacity
    }

    pub fn set_lock_queue_capacity(&mut self, value: usize) -> &mut ServerChannelFactoryConfiguration {
        self.lock_queue_capacity = value;
        self
    }

    pub fn unlock_queue_capacity(&self) -> usize {
        self.unlock_queue_capacity
    }

    pub fn set_unlock_queue_capacity(&mut self, value: usize) -> &mut ServerChannelFactoryConfiguration {
        self.unlock_queue_capacity = value;
        self
    }

    pub fn commit_period(&self) -> Duration {
        self.commit_period
    }

    pub fn set_commit_period(&mut self, value: Duration) -> &mut ServerChannelFactoryConfiguration {
        self.commit_period = value;
        self
    }

    pub fn subscription_timeout(&self) -> Duration {
        self.subscription_timeout
    }

    pub fn set_subscription_timeout(&mut self, value: Duration) -> &mut ServerChannelFactoryConfiguration {
        self.subscription_timeout = value;
        self
    }

    pub fn state_transfer_use_compression(&self) -> bool {
        self.state_transfer_use_compression
    }

    pub fn set_state_transfer_use_compression(&mut self, value: bool) -> &mut ServerChannelFactoryConfiguration {
        self.state_transfer_use_compression = value;
        self
    }

    pub fn state_transfer_max_size(&self) -> u64 {
        self.state_transfer_max_size
    }

    pub fn set_state_transfer_max_size(&mut self, value: u64) -> &mut ServerChannelFactoryConfiguration {
        self.state_transfer_max_size = value;
        self
    }
}

impl Default for ServerChannelFactoryConfiguration {
    fn default() -> Self {
        ServerChannelFactoryConfiguration {
            timer_period: Duration::from_millis(50),
            max_election_timeout: Duration::from_millis(2000),
            min_election_timeout: Duration::from_millis(1000),
            heartbeat_period: Duration::from_millis(150),
            peer_timeout: Duration::from_millis(2000),
            send_failure_backoff: Duration::from_millis(50),
            max_publishing_log_entry_count: 1000,
            min_state_transfer_gap_log_entry_count: 1000_000,
            client_session_timeout: Duration::from_secs(600),
            join_group_period: Duration::from_millis(1000),
            leave_group_period: Duration::from_millis(1000),
            leave_group_attempt_count: 10,
            auto_leave_group_timeout: Duration::from_millis(0),
            max_value_size: 1000_000,
            lock_queue_capacity: 200,
            unlock_queue_capacity: 140,
            commit_period: Duration::from_millis(100),
            subscription_timeout: Duration::from_secs(600),
            state_transfer_use_compression: true,
            state_transfer_max_size: 1000_000_000,
        }
    }
}

//////////////////////////////////////////////////////////
// ServerChannelConfiguration
//////////////////////////////////////////////////////////

pub struct ServerChannelConfigurationBuilder {
    server: Option<ServerConfiguration>,
    group: Option<GroupConfiguration>,
    log_store: Option<Box<dyn LogStoreConfiguration>>,
    message_listener: Option<Box<dyn MessageListenerConfiguration>>,
    message_sender: Option<Box<dyn MessageSenderConfiguration>>,
    state_transfer_port_range_start: Option<u16>,
    state_transfer_port_range_end: Option<u16>,
    state_transfer_bind_address: Option<String>,
    work_dir: Option<String>,
}

impl ServerChannelConfigurationBuilder {
    pub fn new() -> ServerChannelConfigurationBuilder {
        ServerChannelConfigurationBuilder {
            server: None,
            group: None,
            log_store: None,
            message_listener: None,
            message_sender: None,
            state_transfer_port_range_start: Some(17000),
            state_transfer_port_range_end: Some(19000),
            state_transfer_bind_address: Some(String::from("localhost")),
            work_dir: None,
        }
    }

    pub fn set_server(mut self, value: ServerConfiguration) -> ServerChannelConfigurationBuilder {
        self.server = Some(value);
        self
    }

    pub fn set_group(mut self, value: GroupConfiguration) -> ServerChannelConfigurationBuilder {
        self.group = Some(value);
        self
    }

    pub fn set_log_store(mut self, value: Box<dyn LogStoreConfiguration>) -> ServerChannelConfigurationBuilder {
        self.log_store = Some(value);
        self
    }

    pub fn set_message_listener(
        mut self,
        value: Box<dyn MessageListenerConfiguration>,
    ) -> ServerChannelConfigurationBuilder {
        self.message_listener = Some(value);
        self
    }

    pub fn set_message_sender(
        mut self,
        value: Box<dyn MessageSenderConfiguration>,
    ) -> ServerChannelConfigurationBuilder {
        self.message_sender = Some(value);
        self
    }

    pub fn set_state_transfer_port_range_start(mut self, value: u16) -> ServerChannelConfigurationBuilder {
        self.state_transfer_port_range_start = Some(value);
        self
    }

    pub fn set_state_transfer_port_range_end(mut self, value: u16) -> ServerChannelConfigurationBuilder {
        self.state_transfer_port_range_end = Some(value);
        self
    }

    pub fn set_state_transfer_bind_address(mut self, value: String) -> ServerChannelConfigurationBuilder {
        self.state_transfer_bind_address = Some(value);
        self
    }

    pub fn set_work_dir(mut self, value: String) -> ServerChannelConfigurationBuilder {
        self.work_dir = Some(value);
        self
    }

    pub fn build(self) -> ServerChannelConfiguration {
        ServerChannelConfiguration {
            server: self.server.unwrap(),
            group: self.group.unwrap(),
            log_store: self.log_store.unwrap(),
            message_listener: self.message_listener.unwrap(),
            message_sender: self.message_sender.unwrap(),
            state_transfer_port_range_start: self.state_transfer_port_range_start.unwrap(),
            state_transfer_port_range_end: self.state_transfer_port_range_end.unwrap(),
            state_transfer_bind_address: self.state_transfer_bind_address.unwrap(),
            work_dir: self.work_dir.unwrap(),
        }
    }
}

pub struct ServerChannelConfiguration {
    server: ServerConfiguration,
    group: GroupConfiguration,
    log_store: Box<dyn LogStoreConfiguration>,
    message_listener: Box<dyn MessageListenerConfiguration>,
    message_sender: Box<dyn MessageSenderConfiguration>,
    state_transfer_port_range_start: u16,
    state_transfer_port_range_end: u16,
    state_transfer_bind_address: String,
    work_dir: String,
}

impl ServerChannelConfiguration {
    pub fn new(
        server: ServerConfiguration,
        group: GroupConfiguration,
        log_store: Box<dyn LogStoreConfiguration>,
        message_listener: Box<dyn MessageListenerConfiguration>,
        message_sender: Box<dyn MessageSenderConfiguration>,
        state_transfer_port_range_start: u16,
        state_transfer_port_range_end: u16,
        state_transfer_bind_address: String,
        work_dir: String,
    ) -> ServerChannelConfiguration {
        ServerChannelConfiguration {
            server,
            group,
            log_store,
            message_listener,
            message_sender,
            state_transfer_port_range_start,
            state_transfer_port_range_end,
            state_transfer_bind_address,
            work_dir,
        }
    }

    pub fn server(&self) -> &ServerConfiguration {
        &self.server
    }

    pub fn group(&self) -> &GroupConfiguration {
        &self.group
    }

    pub fn log_store(&self) -> &Box<dyn LogStoreConfiguration> {
        &self.log_store
    }

    pub fn message_listener(&self) -> &Box<dyn MessageListenerConfiguration> {
        &self.message_listener
    }

    pub fn message_sender(&self) -> &Box<dyn MessageSenderConfiguration> {
        &self.message_sender
    }

    pub fn state_transfer_port_range_start(&self) -> u16 {
        self.state_transfer_port_range_start
    }

    pub fn state_transfer_port_range_end(&self) -> u16 {
        self.state_transfer_port_range_end
    }

    pub fn state_transfer_bind_address(&self) -> &String {
        &self.state_transfer_bind_address
    }

    pub fn work_dir(&self) -> &String {
        &self.work_dir
    }
}

//////////////////////////////////////////////////////////
// Server SPI
//////////////////////////////////////////////////////////

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PersistentServerState {
    term: u64,
    commit_index: u64,
    voted_for: Option<Uuid>,
}

impl PersistentServerState {
    pub fn new() -> PersistentServerState {
        Default::default()
    }

    pub fn term(&self) -> u64 {
        self.term
    }

    pub fn set_term(&mut self, value: u64) {
        self.term = value;
    }

    pub fn voted_for(&self) -> Option<Uuid> {
        self.voted_for
    }

    pub fn set_voted_for(&mut self, value: Option<Uuid>) {
        self.voted_for = value;
    }

    pub fn increase_term(&mut self) {
        self.term += 1;
    }

    pub fn commit_index(&self) -> u64 {
        self.commit_index
    }

    pub fn set_commit_index(&mut self, value: u64) {
        if self.commit_index < value {
            self.commit_index = value;
        }
    }
}

impl Default for PersistentServerState {
    fn default() -> Self {
        PersistentServerState {
            term: 0,
            commit_index: 0,
            voted_for: None,
        }
    }
}

#[cfg_attr(test, mockall::automock)]
pub trait MessageListenerConfiguration: Send + 'static {
    fn build(&self) -> Box<dyn MessageListenerFactory>;
}

#[cfg_attr(test, mockall::automock)]
pub trait MessageListenerFactory: Send + 'static {
    fn create_listener(&self, message_receiver: Box<dyn MessageReceiver>) -> Rc<dyn MessageListener>;
}

pub trait MessageListener: Send + 'static {}

pub trait MessageReceiver: Send + 'static {
    fn receive(&self, request: Vec<u8>, response: Arc<dyn ResponseHandler>);
}

#[cfg_attr(test, mockall::automock)]
pub trait LogStoreConfiguration: Send + 'static {
    fn build(&self) -> Box<dyn LogStore>;
}

#[cfg_attr(test, mockall::automock)]
pub trait LogStore: Send + 'static {
    fn start_index(&self) -> u64;

    fn end_index(&self) -> u64;

    fn last(&self) -> LogEntry;

    fn get(&self, start_log_index: u64, end_log_index: u64) -> Vec<LogEntry>;

    fn get_at(&self, log_index: u64) -> LogEntry;

    fn commit_index(&self) -> u64;

    fn append(&self, log_entry: LogEntry) -> u64;

    fn set_at(&self, log_index: u64, log_entry: LogEntry);

    fn clear(&self, start_log_index: u64);

    fn commit(&self, log_index: u64);

    fn read(&self, start_log_index: u64, stream: &mut dyn Write, max_size: usize) -> Result<Option<u64>>;

    fn write(&self, stream: &mut dyn Read) -> Result<()>;

    fn is_compaction_locked(&self) -> bool;

    fn lock_compaction(&self);

    fn unlock_compaction(&self);
}

#[cfg_attr(test, mockall::automock)]
pub trait StateMachine: Send + 'static {
    fn begin_transaction(&self, read_only: bool) -> Box<dyn StateMachineTransaction>;
    // TODO:
}

#[cfg_attr(test, mockall::automock)]
pub trait StateMachineTransaction {
    fn is_read_only(&self) -> bool;

    fn read_configuration(&self) -> Option<GroupConfiguration>;

    fn write_configuration(&self, configuration: GroupConfiguration);

    fn read_state(&self) -> Option<PersistentServerState>;

    fn write_state(&self, server_state: PersistentServerState);

    fn publish(&self, log_index: u64, data: &Vec<u8>);

    fn query(&self, request: &Vec<u8>, response_handler: Box<dyn ResponseHandler>);

    fn subscribe(&self, subscription_id: Uuid, request: &Vec<u8>, response_handler: Box<dyn ResponseHandler>);

    fn unsubscribe(&self, subscription_id: Uuid);

    fn commit(&self);
}

#[derive(Clone, Debug)]
pub struct MembershipChange {
    added_servers: Vec<ServerConfiguration>,
    removed_servers: Vec<Uuid>,
}

impl MembershipChange {
    pub fn new(added_servers: Vec<ServerConfiguration>, removed_servers: Vec<Uuid>) -> MembershipChange {
        MembershipChange {
            added_servers,
            removed_servers,
        }
    }

    pub fn added_servers(&self) -> &Vec<ServerConfiguration> {
        &self.added_servers
    }

    pub fn removed_servers(&self) -> &Vec<Uuid> {
        &self.removed_servers
    }
}

#[derive(Clone, Debug)]
pub struct MembershipChangeEvent {
    old_membership: GroupConfiguration,
    new_membership: GroupConfiguration,
    membership_change: MembershipChange,
}

impl MembershipChangeEvent {
    pub fn new(
        old_membership: GroupConfiguration,
        new_membership: GroupConfiguration,
        membership_change: MembershipChange,
    ) -> MembershipChangeEvent {
        MembershipChangeEvent {
            old_membership,
            new_membership,
            membership_change,
        }
    }

    pub fn old_membership(&self) -> &GroupConfiguration {
        &self.old_membership
    }

    pub fn new_membership(&self) -> &GroupConfiguration {
        &self.new_membership
    }

    pub fn membership_change(&self) -> &MembershipChange {
        &self.membership_change
    }
}

pub trait ServerChannelListener: Send + 'static {
    fn on_started();

    fn on_stopped();
}

#[cfg_attr(test, mockall::automock)]
pub trait MembershipListener: Send + 'static {
    fn on_leader(&self);

    fn on_follower(&self);

    fn on_joined(&self);

    fn on_left(&self);

    fn on_membership_changed(&self, event: &MembershipChangeEvent);
}

pub trait MembershipService {
    fn add_listener(&self, listener: Arc<dyn MembershipListener>);

    fn remove_all_listeners(&self);
}
