use crate::artos::common::{FlowController, FlowType};
use std::sync::{Arc, Condvar, Mutex};

//////////////////////////////////////////////////////////
//
//////////////////////////////////////////////////////////

pub struct BlockingFlowController {}

impl BlockingFlowController {
    pub fn new() -> (FlowWaiter, Box<dyn FlowController>) {
        let (publish_wait, publish_lock) = Self::new_per_flow();
        let (query_wait, query_lock) = Self::new_per_flow();
        (
            FlowWaiter {
                publish_waiter: publish_wait,
                query_waiter: query_wait,
            },
            Box::new(FlowLocker {
                publish_locker: publish_lock,
                query_locker: query_lock,
            }),
        )
    }

    fn new_per_flow() -> (SingleFlowWaiter, SingleFlowLocker) {
        let pair = Arc::new((Mutex::new(0), Condvar::new()));
        let pair2 = pair.clone();

        (
            SingleFlowWaiter { cond_var: pair },
            SingleFlowLocker { cond_var: pair2 },
        )
    }
}

//////////////////////////////////////////////////////////
//
//////////////////////////////////////////////////////////

#[derive(Clone)]
pub struct FlowWaiter {
    publish_waiter: SingleFlowWaiter,
    query_waiter: SingleFlowWaiter,
}

impl FlowWaiter {
    pub fn wait(&self, flow: FlowType) {
        match flow {
            FlowType::Publish => self.publish_waiter.wait(),
            FlowType::Query => self.query_waiter.wait(),
        }
    }
}

#[derive(Clone)]
struct SingleFlowWaiter {
    cond_var: Arc<(Mutex<u32>, Condvar)>,
}

impl SingleFlowWaiter {
    fn wait(&self) {
        let (lock, cvar) = &*self.cond_var;
        let mut lock_counter = lock.lock().unwrap();
        while *lock_counter > 0 {
            lock_counter = cvar.wait(lock_counter).unwrap();
        }
    }
}

//////////////////////////////////////////////////////////
//
//////////////////////////////////////////////////////////

struct SingleFlowLocker {
    cond_var: Arc<(Mutex<u32>, Condvar)>,
}

impl SingleFlowLocker {
    fn lock_flow(&self) {
        let (lock, _cvar) = &*self.cond_var;
        let mut lock_counter = lock.lock().unwrap();
        *lock_counter += 1;
    }

    fn unlock_flow(&self) {
        let (lock, cvar) = &*self.cond_var;
        let mut lock_counter = lock.lock().unwrap();
        *lock_counter -= 1;
        cvar.notify_all();
    }
}

struct FlowLocker {
    publish_locker: SingleFlowLocker,
    query_locker: SingleFlowLocker,
}

impl FlowController for FlowLocker {
    fn lock_flow(&self, flow: FlowType) {
        match flow {
            FlowType::Publish => self.publish_locker.lock_flow(),
            FlowType::Query => self.query_locker.lock_flow(),
        }
    }

    fn unlock_flow(&self, flow: FlowType) {
        match flow {
            FlowType::Publish => self.publish_locker.unlock_flow(),
            FlowType::Query => self.query_locker.unlock_flow(),
        }
    }
}
