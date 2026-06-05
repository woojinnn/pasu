//! Top-level dispatch + the lowered-action contract types.
//!
//! [`lower_action`] matches an [`ActionBody`] on its **domain** and delegates to
//! that domain's `lower` entrypoint (`super::<domain>::lower`). Each domain owns
//! its own directory and per-action lowerings, which keeps the fan-out across
//! domains conflict-free. The two struct variants (`Multicall` / `Unknown`) are
//! not domain enums, so they get the whole [`ActionBody`].

use serde_json::Value;

use policy_state::primitives::U256;
use policy_state::token::TokenRef;
use policy_transition::action::{ActionBody, ActionMeta};

use super::common::account::AccountLeverage;
use super::common::amount::TokenDecimals;

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
    /// Host-injected per-token decimals (service-worker registry lookups). Empty
    /// when the host did not / could not resolve them — every `amount_nano*`
    /// call then returns `None` and the lowering omits the optional nano field.
    pub(crate) decimals: &'a TokenDecimals,
    /// Host-injected per-asset venue leverage (service-worker `activeAssetData`
    /// lookups). Empty when the host did not / could not resolve it — every
    /// `leverage_for` call then returns `None` and the lowering omits the
    /// optional `leverage` field. See [`AccountLeverage`].
    pub(crate) leverage: &'a AccountLeverage,
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

    /// Token-native nano sibling for a `(token, raw amount)` pair, or `None`
    /// when the token's decimals were not injected (the lowering then omits the
    /// optional nano field). See [`TokenDecimals`].
    pub(crate) fn amount_nano(&self, token: &TokenRef, raw: U256) -> Option<i64> {
        self.decimals.nano(&token.key, raw)
    }

    /// Nano for an amount denominated in the chain's native 18-decimal asset
    /// (e.g. Lido ETH stake, wrap/unwrap of an 18-decimal LST) — always
    /// resolvable, so it returns `i64` directly (no injected map needed).
    ///
    /// `&self` is unused (native decimals are the constant 18) but kept so call
    /// sites read uniformly as `ctx.amount_nano*`, mirroring the two helpers above.
    #[allow(clippy::unused_self)]
    pub(crate) fn amount_nano_native18(&self, raw: U256) -> i64 {
        super::common::amount::nano_from_decimals(raw, 18)
    }

    /// Host-injected effective leverage for a venue `asset_index`, or `None`
    /// when it was not injected (the lowering then omits the optional
    /// `leverage` field). See [`AccountLeverage`].
    pub(crate) fn leverage_for(&self, asset_index: u32) -> Option<i64> {
        self.leverage.leverage_for(asset_index)
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
    lower_action_enriched(
        action,
        meta,
        tx,
        &TokenDecimals::default(),
        &AccountLeverage::default(),
    )
}

/// Lower an [`ActionBody`] with host-injected per-token `decimals`, so each
/// fungible amount also emits its `amountNano` `Long` sibling (see
/// [`TokenDecimals`]). [`lower_action`] is the decimals-free wrapper — every
/// nano field is then omitted. Leverage is left unresolved (the order
/// `leverage` field is omitted); use [`lower_action_enriched`] to inject it.
///
/// `meta` is the outer `Action`'s [`ActionMeta`]; `tx` carries the EVM routing
/// addresses. See [`LowerCtx`].
///
/// # Errors
///
/// Returns [`LowerError::Unsupported`] for any action variant whose domain has
/// not yet implemented a lowering.
pub fn lower_action_with_decimals(
    action: &ActionBody,
    meta: &ActionMeta,
    tx: &TxMeta<'_>,
    decimals: &TokenDecimals,
) -> Result<LoweredAction, LowerError> {
    lower_action_enriched(action, meta, tx, decimals, &AccountLeverage::default())
}

/// Lower an [`ActionBody`] with both host-injected `decimals` (for `amountNano`
/// siblings) and host-injected `leverage` (for the venue order `leverage`
/// field). [`lower_action`] / [`lower_action_with_decimals`] are the thinner
/// wrappers that default one or both injected maps to empty.
///
/// `meta` is the outer `Action`'s [`ActionMeta`]; `tx` carries the EVM routing
/// addresses. See [`LowerCtx`].
///
/// # Errors
///
/// Returns [`LowerError::Unsupported`] for any action variant whose domain has
/// not yet implemented a lowering.
pub fn lower_action_enriched(
    action: &ActionBody,
    meta: &ActionMeta,
    tx: &TxMeta<'_>,
    decimals: &TokenDecimals,
    leverage: &AccountLeverage,
) -> Result<LoweredAction, LowerError> {
    let ctx = LowerCtx {
        meta,
        tx,
        decimals,
        leverage,
    };
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
        ActionBody::Governance(a) => super::governance::lower(a, &ctx),
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

    use policy_state::live_field::{DataSource, OracleProvider};
    use policy_state::primitives::{Address, ChainId, Time, U256};
    use policy_state::LiveField;
    use policy_transition::action::{ActionMeta, ActionNature};

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
