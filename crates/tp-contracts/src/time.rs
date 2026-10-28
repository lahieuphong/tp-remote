//! Monotonic time. The engine never uses wall-clock time for ordering or expiry, and
//! every time-dependent decision takes `now` as an argument so tests can inject it.

use std::time::Duration;

/// A monotonic instant, in microseconds since an arbitrary process-local origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct MonoTime(u64);

impl MonoTime {
    pub const ZERO: MonoTime = MonoTime(0);

    pub const fn from_micros(micros: u64) -> MonoTime {
        MonoTime(micros)
    }

    pub const fn from_secs(secs: u64) -> MonoTime {
        MonoTime(secs.saturating_mul(1_000_000))
    }

    pub const fn as_micros(self) -> u64 {
        self.0
    }

    pub fn saturating_add(self, duration: Duration) -> MonoTime {
        let micros = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
        MonoTime(self.0.saturating_add(micros))
    }

    /// Time elapsed since `earlier`, or zero if `earlier` is later.
    pub fn saturating_duration_since(self, earlier: MonoTime) -> Duration {
        Duration::from_micros(self.0.saturating_sub(earlier.0))
    }
}

/// Source of monotonic time. Production code uses a real clock (tp-core); tests pass
/// explicit `MonoTime` values instead of sleeping.
pub trait Clock: Send + Sync {
    fn now(&self) -> MonoTime;
}
