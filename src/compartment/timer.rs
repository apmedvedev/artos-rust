use crate::common::utils::Time;
use std::time::{Duration, Instant};

include!("timer_tests.rs");

//////////////////////////////////////////////////////////
// Timer
//////////////////////////////////////////////////////////

pub struct Timer {
    started: bool,
    period: Duration,
    next_timer_time: Instant,
    attempt: u32,
}

impl Timer {
    pub fn new(started: bool, period: Duration) -> Timer {
        Timer {
            started,
            period,
            attempt: 0,
            next_timer_time: Time::now(),
        }
    }

    pub fn is_started(&self) -> bool {
        self.started
    }

    pub fn period(&self) -> Duration {
        self.period
    }

    pub fn set_period(&mut self, period: Duration) {
        self.period = period
    }

    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    pub fn start(&mut self) {
        if self.started {
            return;
        }

        self.restart();
    }

    pub fn restart(&mut self) {
        self.started = true;
        self.next_timer_time = Time::now() + self.period;
        self.attempt = 0;
    }

    pub fn delayed_fire(&mut self) {
        self.started = true;
        self.next_timer_time = Time::now();
    }

    pub fn stop(&mut self) {
        self.started = false;
    }

    pub fn is_fired(&mut self, current_time: Instant) -> bool {
        if self.started && current_time > self.next_timer_time {
            self.next_timer_time = current_time + self.period;
            self.attempt += 1;
            true
        } else {
            false
        }
    }
}
