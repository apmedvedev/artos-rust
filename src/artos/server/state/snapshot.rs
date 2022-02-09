use std::path::PathBuf;
use std::time::Instant;

//////////////////////////////////////////////////////////
// Snapshot
//////////////////////////////////////////////////////////

pub struct Snapshot {
    path: PathBuf,
    log_index: u64,
    creation_time: Instant,
}

impl Snapshot {
    pub fn new(path: PathBuf, log_index: u64, creation_time: Instant) -> Snapshot {
        Snapshot {
            path,
            log_index,
            creation_time,
        }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn log_index(&self) -> u64 {
        self.log_index
    }

    pub fn creation_time(&self) -> Instant {
        self.creation_time
    }
}

//////////////////////////////////////////////////////////
// SnapshotManagerFactory
//////////////////////////////////////////////////////////

pub struct SnapshotManagerFactory {
    // endpoint: String,
// context: Rc<Context>,
// state: Mutex<FactoryState>,
}

struct FactoryState {
    //    replication_protocol: Rc<dyn ReplicationProtocol>,
}

impl SnapshotManagerFactory {}

//////////////////////////////////////////////////////////
// SnapshotManager
//////////////////////////////////////////////////////////

pub struct SnapshotManager {}

impl SnapshotManager {}
