//! `LiveField` — 외부/유도 데이터의 wrapper. spec §7.
//!
//! state 안에 박혀 살면서, Sync Orchestrator (또는 `DerivedFrom` 의 경우 reducer)
//! 가 `value` / `synced_at` / `confidence` 를 in-place 로 갱신한다.
//! `source` 와 `ttl` 은 불변 (어디서 가져오는지의 명세).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

/// LiveField 의 신선도 / 품질 메타 (`Confidence`).
pub mod freshness;
/// LiveField 의 출처 (`DataSource`, `FieldRef`, 보조 enum).
pub mod source;

pub use freshness::Confidence;
pub use source::{
    AuthSpec, DataSource, FieldRef, OracleProvider, PendingFieldName, PositionFieldName,
    TokenFieldName,
};

use crate::primitives::{Duration, Time};

/// 외부 / 유도 데이터 wrapper — value + source + 신선도.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct LiveField<T> {
    /// 본 필드의 현재 값.
    pub value: T,
    /// 본 필드의 출처 (어디서 가져오는지의 명세).
    pub source: DataSource,
    /// 본 값이 마지막으로 sync 된 시각.
    pub synced_at: Time,
    /// 권장 갱신 주기. None = orchestrator 기본값.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "Duration")]
    pub ttl: Option<Duration>,
    /// 신선도 / 품질 메타. Sync orchestrator 가 채움.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub confidence: Option<Confidence>,
}

impl<T> LiveField<T> {
    /// value / source / `synced_at` 으로 `LiveField` 생성. ttl / confidence 는 `None`.
    pub fn new(value: T, source: DataSource, synced_at: Time) -> Self {
        Self {
            value,
            source,
            synced_at,
            ttl: None,
            confidence: None,
        }
    }

    /// `ttl` 을 채워 반환하는 builder.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }

    /// `confidence` 를 채워 반환하는 builder.
    pub fn with_confidence(mut self, c: Confidence) -> Self {
        self.confidence = Some(c);
        self
    }

    /// `now` 기준 ttl 안에 갱신됐는지. ttl 이 없으면 항상 true.
    pub fn fresh_within(&self, now: Time, window: Duration) -> bool {
        let age = now.since(self.synced_at);
        age.as_secs() <= window.as_secs()
    }

    /// ttl 이 정의돼 있으면 그것 기준 stale 판정.
    pub fn is_stale(&self, now: Time) -> bool {
        if let Some(ttl) = self.ttl {
            now.since(self.synced_at).as_secs() > ttl.as_secs()
        } else if let Some(c) = &self.confidence {
            c.is_stale
        } else {
            false
        }
    }
}
