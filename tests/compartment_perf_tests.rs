use artos::compartment::{Compartment, Delayed, Handler, HandlerFactory, Sender};
use std::cell::RefCell;
use std::fmt;
use std::time::{Duration, Instant};

enum TestEvent {
    Send(Instant),
}

const COUNT: u64 = 10_000_000;

struct TestHandler {
    state: RefCell<State>,
}

struct State {
    send_counter: Counter,
}

struct Counter {
    start: Instant,
    count: usize,
    sum: Duration,
}

impl Counter {
    fn new() -> Counter {
        Counter {
            start: Instant::now(),
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

impl fmt::Display for Counter {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let delta = Instant::now() - self.start;
        write!(
            fmt,
            "count: {}, throughput (msg/s): {}, latency (ns): {}",
            self.count,
            self.count as f64 * 1000.0 / delta.as_millis() as f64,
            self.sum.as_nanos() as f64 / self.count as f64
        )
    }
}

impl TestHandler {
    fn new(_sender: Sender<TestEvent>, _delayed: Delayed<TestEvent>) -> TestHandler {
        TestHandler {
            state: RefCell::new(State {
                send_counter: Counter::new(),
            }),
        }
    }
}

impl Handler<TestEvent> for TestHandler {
    fn process(&self) {}

    fn on_timer(&self, _current_time: Instant) {}

    fn handle(&self, event: TestEvent) {
        let mut state = self.state.borrow_mut();
        match event {
            TestEvent::Send(i) => state.send_counter.measure(i),
        };
    }
}

impl Drop for TestHandler {
    fn drop(&mut self) {
        let state = self.state.borrow_mut();
        println!("Sends - {}", state.send_counter)
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

#[ignore]
#[test]
fn perf_test_compartment() {
    let test = Compartment::with_config(TestHandlerFactory::new(), 100, Duration::from_millis(100), 4);

    for _ in 0..COUNT {
        test.send(TestEvent::Send(Instant::now()));
    }
}
