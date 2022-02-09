use crate::common::utils::Time;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::mpsc::TryIter;
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::{Builder, JoinHandle};
use std::time::{Duration, Instant};
use threadpool::ThreadPool;

//////////////////////////////////////////////////////////
// Compartment API
//////////////////////////////////////////////////////////

pub trait HandlerFactory<H, E>
where
    H: Handler<E>,
{
    fn build(self, sender: Sender<E>, delayed: Delayed<E>) -> H;
}

pub trait Handler<E> {
    fn process(&self);

    fn on_timer(&self, current_time: Instant);

    fn handle(&self, event: E);
}

pub struct Compartment<E> {
    holder: Arc<CompartmentHolder<E>>,
}

impl<E> Compartment<E>
where
    E: Send + 'static,
{
    pub fn new<F, H>(handler_factory: F) -> Compartment<E>
    where
        F: HandlerFactory<H, E> + Send + 'static,
        H: Handler<E>,
    {
        Self::with_config(handler_factory, 100, Duration::from_millis(1), num_cpus::get() * 2)
    }

    pub fn with_config<F, H>(
        handler_factory: F,
        queue_size: usize,
        dispatch_period: Duration,
        thread_count: usize,
    ) -> Compartment<E>
    where
        F: HandlerFactory<H, E> + Send + 'static,
        H: Handler<E>,
    {
        let (sender, receiver) = Sender::new(queue_size, dispatch_period, thread_count);
        let in_sender = sender.clone();

        let handle = Builder::new()
            .name(String::from("[main]"))
            .spawn(move || {
                let delayed = Delayed::new();
                let compartment_impl = CompartmentImpl {
                    receiver,
                    handler: handler_factory.build(in_sender, delayed.clone()),
                    delayed,
                };
                compartment_impl.run();
            })
            .unwrap();

        Compartment {
            holder: Arc::new(CompartmentHolder {
                sender,
                handle: Some(handle),
            }),
        }
    }

    pub fn sender(&self) -> &Sender<E> {
        &self.holder.sender
    }

    pub fn send(&self, event: E) {
        self.holder.sender.send(event);
    }

    pub fn execute<F>(&self, job: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.holder.sender.execute(job);
    }
}

impl<E> Clone for Compartment<E> {
    fn clone(&self) -> Self {
        Compartment {
            holder: self.holder.clone(),
        }
    }
}

pub struct Sender<E> {
    tx: mpsc::SyncSender<Option<E>>,
    dispatcher: Dispatcher,
    thread_pool: Arc<Mutex<ThreadPool>>,
}

impl<E> Sender<E> {
    pub fn new(queue_size: usize, dispatch_period: Duration, thread_count: usize) -> (Sender<E>, Receiver<E>) {
        let (tx, rx) = mpsc::sync_channel(queue_size);

        let (dispatcher, dispatcher_impl) = Dispatcher::new(dispatch_period);
        (
            Sender {
                tx,
                dispatcher,
                thread_pool: Arc::new(Mutex::new(
                    threadpool::Builder::new()
                        .thread_name(String::from("[pool]"))
                        .num_threads(thread_count)
                        .build(),
                )),
            },
            Receiver {
                rx,
                dispatcher: dispatcher_impl,
            },
        )
    }

    pub fn send(&self, event: E) {
        self.dispatcher.wakeup();
        self.tx.send(Some(event)).unwrap();
        self.dispatcher.wakeup();
    }

    pub fn execute<F>(&self, job: F)
    where
        F: FnOnce() + Send + 'static,
    {
        if let Ok(thread_pool) = self.thread_pool.lock() {
            thread_pool.execute(job);
        }
    }
}

impl<E> Clone for Sender<E> {
    fn clone(&self) -> Self {
        Sender {
            tx: self.tx.clone(),
            dispatcher: self.dispatcher.clone(),
            thread_pool: self.thread_pool.clone(),
        }
    }
}

pub struct Receiver<E> {
    rx: mpsc::Receiver<Option<E>>,
    dispatcher: DispatcherImpl,
}

impl<E> Receiver<E> {
    pub fn receive(&self) -> TryIter<'_, Option<E>> {
        self.rx.try_iter()
    }

    pub fn wait(&self) {
        self.dispatcher.wait();
    }
}

pub struct Delayed<E> {
    events: Rc<RefCell<VecDeque<E>>>,
}

impl<E> Delayed<E> {
    pub fn new() -> Delayed<E> {
        Delayed {
            events: Rc::new(RefCell::new(VecDeque::new())),
        }
    }

    pub fn send(&self, event: E) {
        let events = &mut self.events.borrow_mut();
        events.push_back(event);
    }
}

impl<E> Clone for Delayed<E> {
    fn clone(&self) -> Self {
        Delayed {
            events: self.events.clone(),
        }
    }
}
//////////////////////////////////////
// Compartment implementation
//////////////////////////////////////

struct CompartmentHolder<E> {
    sender: Sender<E>,
    handle: Option<JoinHandle<()>>,
}

impl<E> Drop for CompartmentHolder<E> {
    fn drop(&mut self) {
        self.sender.tx.send(None).unwrap();
        self.sender.dispatcher.wakeup();
        self.handle.take().unwrap().join().unwrap();
    }
}

struct CompartmentImpl<E, H> {
    receiver: Receiver<E>,
    handler: H,
    delayed: Delayed<E>,
}

impl<E, H> CompartmentImpl<E, H>
where
    H: Handler<E>,
{
    fn run(&self) {
        'outer: loop {
            for event in self.receiver.receive() {
                if let Some(event) = event {
                    self.handler.handle(event);
                } else {
                    break 'outer;
                }
            }

            self.handler.process();

            let current_time = Time::now();
            self.handler.on_timer(current_time);

            let mut events = self.delayed.events.borrow_mut();
            if let Some(event) = events.pop_front() {
                self.handler.handle(event);
            }

            self.receiver.wait();
        }
    }
}

//////////////////////////////////////////////////////////
// Dispatcher & DispatcherImpl
//////////////////////////////////////////////////////////

#[derive(Clone)]
struct Dispatcher {
    cond_var: Arc<(Mutex<bool>, Condvar)>,
}

impl Dispatcher {
    fn new(dispatch_period: Duration) -> (Dispatcher, DispatcherImpl) {
        let pair = Arc::new((Mutex::new(false), Condvar::new()));
        let pair2 = pair.clone();

        (
            Dispatcher { cond_var: pair },
            DispatcherImpl {
                dispatch_period,
                cond_var: pair2,
            },
        )
    }

    fn wakeup(&self) {
        let (_, cvar) = &*self.cond_var;
        cvar.notify_one();
    }
}

struct DispatcherImpl {
    dispatch_period: Duration,
    cond_var: Arc<(Mutex<bool>, Condvar)>,
}

impl DispatcherImpl {
    fn wait(&self) {
        let (lock, cvar) = &*self.cond_var;
        let started = lock.lock().unwrap();
        let _ = cvar.wait_timeout(started, self.dispatch_period).unwrap();
    }
}
