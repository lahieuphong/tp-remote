//! Production monotonic clock.

use std::time::Instant;

use tp_contracts::time::{Clock, MonoTime};

/// Monotonic time since the clock was created, from `std::time::Instant`.
#[derive(Debug, Clone, Copy)]
pub struct MonotonicClock {
    origin: Instant,
}

impl MonotonicClock {
    pub fn new() -> MonotonicClock {
        MonotonicClock {
            origin: Instant::now(),
        }
    }
}

impl Default for MonotonicClock {
    fn default() -> Self {
        MonotonicClock::new()
    }
}

impl Clock for MonotonicClock {
    fn now(&self) -> MonoTime {
        let micros = u64::try_from(self.origin.elapsed().as_micros()).unwrap_or(u64::MAX);
        MonoTime::from_micros(micros)
    }
}
