use artos::compartment::{Compartment, Delayed, Handler, HandlerFactory, Sender, Timer};
use std::cell::RefCell;
use std::thread;
use std::time::{Duration, Instant};

enum TestEvent {
    Send(Instant),
    DelayedSource(Instant),
    Delayed(Instant),
    ExecuteSource(Instant),
    Execute(Instant),
    Check,
}

const COUNT: usize = 10;

struct TestHandler {
    sender: Sender<TestEvent>,
    delayed: Delayed<TestEvent>,
    state: RefCell<State>,
}

struct State {
    send_counter: Counter,
    timer_counter: Counter,
    execute_counter: Counter,
    delayed_counter: Counter,
    process_counter: Counter,
    timer: Timer,
}

struct Counter {
    count: usize,
    sum: Duration,
}

impl Counter {
    fn new() -> Counter {
        Counter {
            count: 0,
            sum: Duration::from_millis(0),
        }
    }

    fn measure(&mut self, since: Instant) {
        let delta = Instant::now().duration_since(since);
        self.count(delta);
    }

    fn count(&mut self, value: Duration) {
        self.count += 1;
        self.sum += value;
    }
}

impl TestHandler {
    fn new(sender: Sender<TestEvent>, delayed: Delayed<TestEvent>) -> TestHandler {
        TestHandler {
            sender,
            delayed,
            state: RefCell::new(State {
                send_counter: Counter::new(),
                timer_counter: Counter::new(),
                execute_counter: Counter::new(),
                delayed_counter: Counter::new(),
                process_counter: Counter::new(),
                timer: Timer::new(true, Duration::from_millis(1)),
            }),
        }
    }

    fn check_state(&self, state: &State) {
        assert!(state.send_counter.count == COUNT);
        assert!(state.execute_counter.count == COUNT);
        assert!(state.delayed_counter.count == COUNT);
        assert!(state.timer_counter.count != 0);
        assert!(state.process_counter.count != 0);
    }
}

impl Handler<TestEvent> for TestHandler {
    fn process(&self) {
        let mut state = self.state.borrow_mut();
        state.process_counter.count(Duration::from_millis(0));
    }

    fn on_timer(&self, current_time: Instant) {
        let mut state = self.state.borrow_mut();
        if state.timer.is_fired(current_time) {
            let period = state.timer.period();
            state.timer_counter.count(period);
        }
    }

    fn handle(&self, event: TestEvent) {
        let mut state = self.state.borrow_mut();
        match event {
            TestEvent::Send(i) => state.send_counter.measure(i),
            TestEvent::DelayedSource(i) => self.delayed.send(TestEvent::Delayed(i)),
            TestEvent::Delayed(i) => state.delayed_counter.measure(i),
            TestEvent::ExecuteSource(i) => {
                let sender = self.sender.clone();
                self.sender.execute(move || sender.send(TestEvent::Execute(i)))
            }
            TestEvent::Execute(i) => state.execute_counter.measure(i),
            TestEvent::Check => self.check_state(&state),
        };
    }
}

struct TestHandlerFactory {}

impl TestHandlerFactory {
    fn new() -> TestHandlerFactory {
        TestHandlerFactory {}
    }
}

impl HandlerFactory<TestHandler, TestEvent> for TestHandlerFactory {
    fn build(self, sender: Sender<TestEvent>, delayed: Delayed<TestEvent>) -> TestHandler {
        TestHandler::new(sender, delayed)
    }
}

#[test]
fn test_compartment() {
    let test = Compartment::with_config(TestHandlerFactory::new(), 100, Duration::from_millis(100), 4);

    for _ in 0..COUNT {
        test.send(TestEvent::Send(Instant::now()));
        test.send(TestEvent::DelayedSource(Instant::now()));
        test.send(TestEvent::ExecuteSource(Instant::now()));

        thread::sleep(Duration::from_millis(100));
    }

    test.send(TestEvent::Check);
}
