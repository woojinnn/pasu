//! 시간 표기 — unix epoch 초.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

/// Unix epoch 초 단위의 시각.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(transparent)]
pub struct Time(
    /// Unix epoch 으로부터 경과한 초.
    pub u64,
);

impl Time {
    /// Unix epoch 초로부터 `Time` 생성.
    pub const fn from_unix(secs: u64) -> Self {
        Self(secs)
    }

    /// Unix epoch 초로 환원.
    pub fn as_unix(&self) -> u64 {
        self.0
    }

    /// `Duration` 만큼 더한 시각 (overflow 시 `u64::MAX` 로 saturate).
    pub fn saturating_add(&self, dur: Duration) -> Self {
        Self(self.0.saturating_add(dur.0))
    }

    /// `Duration` 만큼 뺀 시각 (underflow 시 0 으로 saturate).
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

/// 초 단위 기간.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(transparent)]
pub struct Duration(
    /// 초 단위 기간 값.
    pub u64,
);

impl Duration {
    /// 초로부터 `Duration` 생성.
    pub const fn from_secs(s: u64) -> Self {
        Self(s)
    }

    /// 초 단위로 환원.
    pub fn as_secs(&self) -> u64 {
        self.0
    }
}

impl From<u64> for Duration {
    fn from(v: u64) -> Self {
        Self(v)
    }
}
