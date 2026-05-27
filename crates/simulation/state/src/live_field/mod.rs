//! LiveField — 외부/유도 데이터의 wrapper. spec §7.
//!
//! state 안에 박혀 살면서, Sync Orchestrator (또는 DerivedFrom 의 경우 reducer)
//! 가 `value` / `synced_at` / `confidence` 를 in-place 로 갱신한다.
//! `source` 와 `ttl` 은 불변 (어디서 가져오는지의 명세).

use serde::{Deserialize, Serialize};

pub mod freshness;
pub mod source;

pub use freshness::Confidence;
pub use source::{
    AuthSpec, DataSource, FieldRef, OracleProvider, PendingFieldName, PositionFieldName,
    TokenFieldName,
};

use crate::primitives::{Duration, Time};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveField<T> {
    pub value: T,
    pub source: DataSource,
    pub synced_at: Time,
    /// 권장 갱신 주기. None = orchestrator 기본값.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl: Option<Duration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
}

impl<T> LiveField<T> {
    pub fn new(value: T, source: DataSource, synced_at: Time) -> Self {
        Self {
            value,
            source,
            synced_at,
            ttl: None,
            confidence: None,
        }
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }

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
