//! 시간 표기 — unix epoch 초.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Time(pub u64);

impl Time {
    pub const fn from_unix(secs: u64) -> Self {
        Self(secs)
    }

    pub fn as_unix(&self) -> u64 {
        self.0
    }

    pub fn saturating_add(&self, dur: Duration) -> Self {
        Self(self.0.saturating_add(dur.0))
    }

    pub fn saturating_sub(&self, dur: Duration) -> Self {
        Self(self.0.saturating_sub(dur.0))
    }

    /// 두 시점의 차이를 초 단위 `Duration` 으로. self >= other 가정.
    pub fn since(&self, earlier: Time) -> Duration {
        Duration(self.0.saturating_sub(earlier.0))
    }
}

impl From<u64> for Time {
    fn from(v: u64) -> Self {
        Self(v)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Duration(pub u64);

impl Duration {
    pub const fn from_secs(s: u64) -> Self {
        Self(s)
    }

    pub fn as_secs(&self) -> u64 {
        self.0
    }
}

impl From<u64> for Duration {
    fn from(v: u64) -> Self {
        Self(v)
    }
}
