//! Top-level dispatch + the lowered-action contract types.
//!
//! [`lower_action`] matches an [`ActionBody`] on its **domain** and delegates to
//! that domain's `lower` entrypoint (`super::<domain>::lower`). Each domain owns
//! its own directory and per-action lowerings, which keeps the fan-out across
//! domains conflict-free. The two struct variants (`Multicall` / `Unknown`) are
//! not domain enums, so they get the whole [`ActionBody`].

use serde_json::Value;

use simulation_reducer::action::{ActionBody, ActionMeta};

/// A lowered action ready for the Cedar engine: the `principal` / `action` /
/// `resource` entity uids (as parseable strings) plus the action-context JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoweredAction {
    /// Principal entity uid, e.g. `Wallet::"0xabc…"`.
    pub principal: String,
    /// Action entity uid, namespaced + `PascalCase`, e.g. `Amm::Action::"Swap"`.
    pub action_uid: String,
    /// Resource entity uid, e.g. `Protocol::"0xrouter…"`.
    pub resource: String,
    /// The cedarschema action-context object (conforms to the action's
    /// `*Context` type, e.g. `Amm::SwapContext`).
    pub context: Value,
}

/// Transaction-level fields the lowering needs for the `principal` / `resource`
/// entity uids. `principal = Wallet::"<from>"`, `resource = Protocol::"<to>"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxMeta<'a> {
    /// Transaction sender — becomes the `Wallet` principal.
    pub from: &'a str,
    /// Transaction target — becomes the `Protocol` resource.
    pub to: &'a str,
}

/// Error returned when an [`ActionBody`] variant has no new-model lowering yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LowerError {
    /// The action has no lowering. Carries a `domain/tag` label
    /// (e.g. `"amm/add_liquidity"`, `"unknown"`).
    Unsupported(String),
}

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(label) => write!(f, "unsupported action: {label}"),
        }
    }
}

impl std::error::Error for LowerError {}

/// Per-lowering context threaded to every per-action `lower`.
///
/// `meta` is the [`ActionMeta`] carried by the *outer* `Action` (submitter,
/// submission time, on-chain/off-chain nature) — the new action model keeps it
/// on `Action`, not on `ActionBody`, but every `*Context` requires a
/// `meta: Core::ActionMeta`, so it is threaded in here. `tx` carries the EVM
/// routing addresses (`from` / `to`), which are not part of the action model.
pub(crate) struct LowerCtx<'a> {
    pub(crate) meta: &'a ActionMeta,
    pub(crate) tx: &'a TxMeta<'a>,
}

impl LowerCtx<'_> {
    /// Assemble a [`LoweredAction`] with the standard `Wallet` / `Protocol`
    /// triple and the given namespaced action uid.
    pub(crate) fn lowered(&self, action_uid: &str, context: Value) -> LoweredAction {
        LoweredAction {
            principal: format!(r#"Wallet::"{}""#, self.tx.from),
            action_uid: action_uid.to_owned(),
            resource: format!(r#"Protocol::"{}""#, self.tx.to),
            context,
        }
    }

    /// The `meta` context value every action context embeds.
    pub(crate) fn meta(&self) -> Value {
        super::common::meta::lower_action_meta(self.meta)
    }
}

/// Lower an [`ActionBody`] to a [`LoweredAction`] by delegating to the matching
/// domain entrypoint.
///
/// `meta` is the outer `Action`'s [`ActionMeta`]; `tx` carries the EVM routing
/// addresses. See [`LowerCtx`].
///
/// # Errors
///
/// Returns [`LowerError::Unsupported`] for any action variant whose domain has
/// not yet implemented a lowering.
pub fn lower_action(
    action: &ActionBody,
    meta: &ActionMeta,
    tx: &TxMeta<'_>,
) -> Result<LoweredAction, LowerError> {
    let ctx = LowerCtx { meta, tx };
    match action {
        ActionBody::Token(a) => super::token::lower(a, &ctx),
        ActionBody::Amm(a) => super::amm::lower(a, &ctx),
        ActionBody::Lending(a) => super::lending::lower(a, &ctx),
        ActionBody::Airdrop(a) => super::airdrop::lower(a, &ctx),
        ActionBody::Launchpad(a) => super::launchpad::lower(a, &ctx),
        ActionBody::Perp(a) => super::perp::lower(a, &ctx),
        ActionBody::LiquidStaking(a) => super::liquid_staking::lower(a, &ctx),
        ActionBody::Permission(a) => super::permission::lower(a, &ctx),
        ActionBody::Yield(a) => super::yield_::lower(a, &ctx),
        ActionBody::Restaking(a) => super::restaking::lower(a, &ctx),
        ActionBody::Staking(a) => super::staking::lower(a, &ctx),
        ActionBody::HyperliquidCore(a) => super::hyperliquid_core::lower(a, &ctx),
        ActionBody::Multicall { .. } => super::multicall::lower(action, &ctx),
        ActionBody::Unknown { .. } => super::unknown::lower(action, &ctx),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    use std::str::FromStr;

    use simulation_reducer::action::{ActionMeta, ActionNature};
    use simulation_state::live_field::{DataSource, OracleProvider};
    use simulation_state::primitives::{Address, ChainId, Time, U256};
    use simulation_state::LiveField;

    const FROM: &str = "0x1111111111111111111111111111111111111111";
    const TO: &str = "0x2222222222222222222222222222222222222222";

    /// An `Unknown` action (catch-all for unmatched calldata) routes through
    /// dispatch and lowers to the `Core::Action::"Unknown"` uid — confirming the
    /// `Multicall` / `Unknown` struct-variant arms reach their leaf lowerings.
    #[test]
    fn unknown_action_lowers_to_core_unknown() {
        let now = Time::from_unix(1_738_000_000);
        let body = ActionBody::Unknown {
            target: Address::from_str("0xfeed000000000000000000000000000000000001").unwrap(),
            chain: ChainId::ethereum_mainnet(),
            calldata: "0xdeadbeef".into(),
            value: U256::ZERO,
        };
        let meta = ActionMeta {
            submitted_at: now,
            submitter: Address::from_str("0x000000000000000000000000000000000000a01c").unwrap(),
            nature: ActionNature::OnchainTx {
                chain: ChainId::ethereum_mainnet(),
                nonce: 0,
                gas_limit: U256::from(21_000u64),
                gas_price: LiveField::new(
                    U256::from(1u64),
                    DataSource::OracleFeed {
                        provider: OracleProvider::Pyth,
                        feed_id: "x".into(),
                    },
                    now,
                ),
                value: U256::ZERO,
            },
        };

        let lowered = lower_action(&body, &meta, &TxMeta { from: FROM, to: TO }).unwrap();
        assert_eq!(lowered.action_uid, r#"Core::Action::"Unknown""#);
    }
}
