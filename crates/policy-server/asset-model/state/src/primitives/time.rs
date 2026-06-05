//! Time representation in Unix epoch seconds.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Tsify,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(transparent)]
/// A point in time expressed as unix epoch seconds.
pub struct Time(
    /// Seconds elapsed since the Unix epoch.
    pub u64,
);

impl Time {
    /// Builds a `Time` from a unix epoch timestamp in seconds.
    #[must_use]
    pub const fn from_unix(secs: u64) -> Self {
        Self(secs)
    }

    /// Returns the underlying unix epoch timestamp in seconds.
    #[must_use]
    pub const fn as_unix(&self) -> u64 {
        self.0
    }

    /// Adds a `Duration`, saturating at `u64::MAX` instead of overflowing.
    #[must_use]
    pub const fn saturating_add(&self, dur: Duration) -> Self {
        Self(self.0.saturating_add(dur.0))
    }

    /// Subtracts a `Duration`, saturating at zero instead of underflowing.
    #[must_use]
    pub const fn saturating_sub(&self, dur: Duration) -> Self {
        Self(self.0.saturating_sub(dur.0))
    }

    /// Returns the elapsed `Duration` since `earlier` in seconds, assuming `self >= earlier`.
    #[must_use]
    pub const fn since(&self, earlier: Self) -> Duration {
        Duration(self.0.saturating_sub(earlier.0))
    }
}

impl From<u64> for Time {
    fn from(v: u64) -> Self {
        Self(v)
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Tsify,
)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(transparent)]
/// A span of time expressed in whole seconds.
pub struct Duration(
    /// Duration in seconds.
    pub u64,
);

impl Duration {
    /// Builds a `Duration` from a count of seconds.
    #[must_use]
    pub const fn from_secs(s: u64) -> Self {
        Self(s)
    }

    /// Returns the duration as a count of seconds.
    #[must_use]
    pub const fn as_secs(&self) -> u64 {
        self.0
    }
}

impl From<u64> for Duration {
    fn from(v: u64) -> Self {
        Self(v)
    }
}
