use crate::artos::common::Error;
use crate::artos::server::channel::{Event, ServerState};
use crate::artos::server::utils::CompletionHandler;
use crate::compartment::Sender;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::Path;
use std::rc::Rc;
use uuid::Uuid;

//////////////////////////////////////////////////////////
// StateTransfer acquire/apply
//////////////////////////////////////////////////////////

pub trait StateTransferApplier {
    fn apply_snapshot(&self, state_file: &File, log_index: u64, compressed: bool) -> Result<(), Error>;

    fn apply_log(&self, state_file: &File, compressed: bool) -> Result<(), Error>;
}

pub struct LogState {
    file: File,
    compressed: bool,
    log_index: u64,
}

impl LogState {
    pub fn new(file: File, compressed: bool, log_index: u64) -> LogState {
        LogState {
            file,
            compressed,
            log_index,
        }
    }

    pub fn file(&self) -> &File {
        &self.file
    }

    pub fn is_compressed(&self) -> bool {
        self.compressed
    }

    pub fn log_index(&self) -> u64 {
        self.log_index
    }
}

pub struct TransferState {
    snapshot_state: Option<LogState>,
    log_states: Vec<LogState>,
}

impl TransferState {
    pub fn new(snapshot_state: Option<LogState>, log_states: Vec<LogState>) -> TransferState {
        TransferState {
            snapshot_state,
            log_states,
        }
    }

    pub fn snapshot_state(&self) -> &Option<LogState> {
        &self.snapshot_state
    }

    pub fn log_states(&self) -> &Vec<LogState> {
        &self.log_states
    }
}

pub trait StateTransferAcquirerFactory: Send + 'static {
    fn create_acquirer(&self) -> Box<dyn StateTransferAcquirer>;
}

pub trait StateTransferAcquirer: Send + 'static {
    fn acquire_state(&self, group_id: Uuid, log_index: u64, work_dir: &Path) -> Result<TransferState, Error>;
}

//////////////////////////////////////////////////////////
// StateTransfer messages
//////////////////////////////////////////////////////////

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct StateTransferRequest {
    group_id: Uuid,
    log_index: u64,
}

impl StateTransferRequest {
    pub fn new(group_id: Uuid, log_index: u64) -> StateTransferRequest {
        StateTransferRequest { group_id, log_index }
    }

    pub fn group_id(&self) -> Uuid {
        self.group_id
    }

    pub fn log_index(&self) -> u64 {
        self.log_index
    }
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct StateTransferResponseHeader {
    file_count: u32,
}

impl StateTransferResponseHeader {
    pub fn new(file_count: u32) -> StateTransferResponseHeader {
        StateTransferResponseHeader { file_count }
    }

    pub fn file_count(&self) -> u32 {
        self.file_count
    }
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct StateTransferFileHeader {
    snapshot: bool,
    compressed: bool,
    log_index: u64,
    file_size: u64,
}

impl StateTransferFileHeader {
    pub fn new(snapshot: bool, compressed: bool, log_index: u64, file_size: u64) -> StateTransferFileHeader {
        StateTransferFileHeader {
            snapshot,
            compressed,
            log_index,
            file_size,
        }
    }

    pub fn is_snapshot(&self) -> bool {
        self.snapshot
    }

    pub fn is_compressed(&self) -> bool {
        self.compressed
    }

    pub fn log_index(&self) -> u64 {
        self.log_index
    }

    pub fn file_size(&self) -> u64 {
        self.file_size
    }
}

//////////////////////////////////////////////////////////
// StateTransferProtocol API
//////////////////////////////////////////////////////////

#[cfg_attr(test, mockall::automock)]
pub trait StateTransferProtocol {
    fn start(&self) {
        unimplemented!()
    }

    fn stop(&self) {
        unimplemented!()
    }

    fn request_state_transfer(
        &self,
        _host: &String,
        _port: u16,
        _start_log_index: u64,
        _completion_handler: Box<dyn CompletionHandler<(), Error>>,
    ) {
        unimplemented!()
    }

    fn on_leader(&self, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn on_follower(&self, _server_state: &mut ServerState) {
        unimplemented!()
    }

    fn host(&self) -> &String {
        unimplemented!()
    }

    fn port(&self) -> u16 {
        unimplemented!()
    }
}

pub struct NullStateTransferProtocol {}

impl NullStateTransferProtocol {
    pub fn new() -> Rc<dyn StateTransferProtocol> {
        Rc::new(NullStateTransferProtocol {})
    }
}

impl StateTransferProtocol for NullStateTransferProtocol {
    fn start(&self) {}

    fn stop(&self) {}

    fn request_state_transfer(
        &self,
        _host: &String,
        _port: u16,
        _start_log_index: u64,
        _completion_handler: Box<dyn CompletionHandler<(), Error>>,
    ) {
        unimplemented!()
    }

    fn on_leader(&self, _server_state: &mut ServerState) {}

    fn on_follower(&self, _server_state: &mut ServerState) {}

    fn host(&self) -> &String {
        unimplemented!()
    }

    fn port(&self) -> u16 {
        unimplemented!()
    }
}

//////////////////////////////////////////////////////////
// SingleServerStateTransferProtocol API
//////////////////////////////////////////////////////////

pub struct SingleServerStateTransferProtocol {}

impl StateTransferProtocol for SingleServerStateTransferProtocol {}

//////////////////////////////////////////////////////////
// StateTransferProtocolImpl
//////////////////////////////////////////////////////////

pub struct StateTransferProtocolImpl {}

impl StateTransferProtocolImpl {}

//////////////////////////////////////////////////////////
// StateTransferCompletionHandler
//////////////////////////////////////////////////////////

#[derive(Copy, Clone)]
pub enum StateTransferCompletionType {
    Join,
    Replication,
}

pub struct StateTransferCompletionHandler {
    completion_type: StateTransferCompletionType,
    sender: Sender<Event>,
}

impl StateTransferCompletionHandler {
    pub fn new(
        completion_type: StateTransferCompletionType,
        sender: Sender<Event>,
    ) -> Box<StateTransferCompletionHandler> {
        Box::new(StateTransferCompletionHandler {
            completion_type,
            sender,
        })
    }
}

impl CompletionHandler<(), Error> for StateTransferCompletionHandler {
    fn on_succeeded(&self, _result: ()) {
        self.sender.send(Event::StateTransferCompleted(self.completion_type))
    }

    fn on_failed(&self, _error: Error) {
        self.sender.send(Event::StateTransferCompleted(self.completion_type))
    }
}
