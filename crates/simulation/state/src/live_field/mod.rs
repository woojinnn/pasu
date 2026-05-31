//! `LiveField` — wrapper for externally sourced or derived data. See spec §7.
//!
//! It lives embedded inside the state; the Sync orchestrator (or a reducer in
//! the `DerivedFrom` case) updates `value` / `synced_at` / `confidence` in place.
//! `source` and `ttl` are immutable (the specification of where the data comes from).

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

pub mod freshness;
pub mod source;

pub use freshness::Confidence;
pub use source::{
    AuthSpec, DataSource, FieldRef, OracleProvider, PendingFieldName, PositionFieldName,
    RegistryResource, TokenFieldName,
};

use crate::primitives::{Duration, Time};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
/// Wrapper holding an externally sourced or derived value together with its
/// provenance and freshness metadata.
pub struct LiveField<T> {
    /// The current value, updated in place by the Sync orchestrator or reducer.
    pub value: T,
    /// Immutable specification of where this value is fetched/derived from.
    pub source: DataSource,
    /// Timestamp of the most recent successful sync of `value`.
    pub synced_at: Time,
    /// Recommended refresh interval. `None` means use the orchestrator default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "Duration")]
    pub ttl: Option<Duration>,
    /// Optional freshness/quality metadata for the current value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub confidence: Option<Confidence>,
}

impl<T> LiveField<T> {
    /// Builds a `LiveField` from a value, its source, and a sync timestamp,
    /// leaving `ttl` and `confidence` unset.
    pub const fn new(value: T, source: DataSource, synced_at: Time) -> Self {
        Self {
            value,
            source,
            synced_at,
            ttl: None,
            confidence: None,
        }
    }

    /// Returns the field with its recommended refresh interval (`ttl`) set.
    pub const fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = Some(ttl);
        self
    }

    /// Returns the field with its `confidence` metadata attached.
    pub const fn with_confidence(mut self, c: Confidence) -> Self {
        self.confidence = Some(c);
        self
    }

    /// Whether the value was synced within `window` of `now` (by age in seconds).
    pub const fn fresh_within(&self, now: Time, window: Duration) -> bool {
        let age = now.since(self.synced_at);
        age.as_secs() <= window.as_secs()
    }

    /// Whether the value is stale: judged against `ttl` if set, otherwise against
    /// the `confidence` staleness flag, defaulting to fresh when neither is present.
    pub const fn is_stale(&self, now: Time) -> bool {
        if let Some(ttl) = self.ttl {
            now.since(self.synced_at).as_secs() > ttl.as_secs()
        } else if let Some(c) = &self.confidence {
            c.is_stale
        } else {
            false
        }
    }
}
