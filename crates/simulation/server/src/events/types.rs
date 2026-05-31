//! Event payloads streamed to dashboard clients over SSE.
//!
//! Each variant becomes one SSE `event:` block; the payload struct is the
//! `data:` JSON body. Names follow `snake_case` so JS clients can pattern
//! match on `event.type`.

use serde::{Deserialize, Serialize};

use simulation_state::primitives::ChainId;

/// One scopeball event. Tagged externally so the JSON shape matches what
/// the dashboard's `EventSource.addEventListener('tx_confirmed', …)`
/// expects.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// A transaction was just recorded in `predicted` status (the
    /// extension's pre-sign step).
    TxPredicted(TxRef),
    /// User signed in MetaMask — the tx hash is now known and it is
    /// pending in the mempool.
    TxPending(TxRefWithHash),
    /// Receipt arrived; tx is in a block.
    TxConfirmed(TxConfirmed),
    /// Receipt arrived; tx failed (revert / out-of-gas / cancelled).
    TxFailed(TxConfirmed),
    /// Sync orchestrator finished a wallet refresh tick.
    WalletSynced(WalletSync),
    /// Cedar evaluation produced a non-allow verdict the dashboard
    /// should surface in the activity feed.
    PolicyViolated(PolicyViolation),
}

/// Reference to an in-flight tx without a hash yet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxRef {
    pub tx_id: String,
    pub wallet: String,
    pub chain: ChainId,
}

/// Reference to a tx now broadcast (hash known).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxRefWithHash {
    pub tx_id: String,
    pub wallet: String,
    pub chain: ChainId,
    pub tx_hash: String,
}

/// Confirmation / failure payload — same shape works for both because the
/// block info exists in either case.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxConfirmed {
    pub tx_id: String,
    pub wallet: String,
    pub chain: ChainId,
    pub tx_hash: String,
    pub block_number: u64,
    pub success: bool,
}

/// Wallet sync tick summary — enough for the dashboard to flash a
/// "synced N seconds ago" indicator.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WalletSync {
    pub wallet: String,
    pub fields_updated: usize,
    pub fields_failed: usize,
    pub synced_at: i64,
}

/// Policy verdict surfaced to the dashboard. `policy_id` matches the
/// installed manifest; `reasons` is the human-readable list Cedar produced.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyViolation {
    pub policy_id: String,
    pub wallet: String,
    pub verdict: String,
    pub reasons: Vec<String>,
}

impl Event {
    /// Returns the wire `type` discriminator — matches what an SSE client
    /// listens on with `addEventListener(<name>, …)`.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::TxPredicted(_) => "tx_predicted",
            Self::TxPending(_) => "tx_pending",
            Self::TxConfirmed(_) => "tx_confirmed",
            Self::TxFailed(_) => "tx_failed",
            Self::WalletSynced(_) => "wallet_synced",
            Self::PolicyViolated(_) => "policy_violated",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_strings_match_serde_tag() {
        let ev = Event::TxPredicted(TxRef {
            tx_id: "x".into(),
            wallet: "0x".into(),
            chain: ChainId::ethereum_mainnet(),
        });
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["type"], "tx_predicted");
        assert_eq!(ev.kind(), "tx_predicted");
    }

    #[test]
    fn round_trip_tx_confirmed() {
        let ev = Event::TxConfirmed(TxConfirmed {
            tx_id: "t1".into(),
            wallet: "0xabc".into(),
            chain: ChainId::ethereum_mainnet(),
            tx_hash: "0xdeadbeef".into(),
            block_number: 19_000_000,
            success: true,
        });
        let s = serde_json::to_string(&ev).unwrap();
        let back: Event = serde_json::from_str(&s).unwrap();
        assert_eq!(ev, back);
    }
}
