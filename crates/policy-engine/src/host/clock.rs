//! Host clock capability used to stamp time-sensitive policy context.

use std::time::{SystemTime, UNIX_EPOCH};

/// Host-provided clock.
pub trait Clock: Send + Sync {
    /// Return the current Unix timestamp in seconds.
    fn now(&self) -> u64;
}

/// System wall-clock implementation used by default host capabilities.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs())
    }
}

/// Test/playground clock with a fixed Unix timestamp.
#[derive(Debug, Clone)]
pub struct MockClock {
    now_ts: u64,
}

impl MockClock {
    /// Construct a mock clock fixed at `now_ts`.
    #[must_use]
    pub const fn with_fixed(now_ts: u64) -> Self {
        Self { now_ts }
    }
}

impl Clock for MockClock {
    fn now(&self) -> u64 {
        self.now_ts
    }
}

static SYSTEM_CLOCK: SystemClock = SystemClock;

/// Return the process-wide default system clock.
#[must_use]
pub const fn system_clock() -> &'static dyn Clock {
    &SYSTEM_CLOCK
}
