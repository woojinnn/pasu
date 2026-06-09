//! Hyperliquid CORE domain lowering: thin actions with NO live inputs.
//!
//! Mirrors the perp/token fan-out: a per-action `lower` leaf for each variant,
//! plus the shared `hl_venue` encoder. Every action lowers to a
//! `HyperliquidCore::*Context` whose numeric fields (price / size / amount) are
//! emitted as decimal STRINGS — fractional-safe and free of Cedar's `decimal`
//! 4-dp limit. Policies match on action type + `context.venue.name` +
//! side / destination, not on numeric magnitude.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HyperliquidCoreAction;

use super::dispatch::{LowerCtx, LowerError, LoweredAction};

mod amount;
mod c_deposit;
mod c_withdraw;
mod send_asset;
mod send_to_evm_with_data;
mod spot_send;
mod sub_account_transfer;
mod token_delegate;
mod unknown;
mod usd_class_transfer;
mod usd_send;
mod vault_transfer;
mod withdraw;

/// Dispatch a [`HyperliquidCoreAction`] to its per-action lowering.
///
/// # Errors
///
/// Infallible today: every variant has a leaf lowering. The `Result` matches
/// the shared per-action contract so the dispatch stays uniform.
pub(crate) fn lower(
    action: &HyperliquidCoreAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    match action {
        HyperliquidCoreAction::Withdraw(a) => withdraw::lower(a, ctx),
        HyperliquidCoreAction::UsdSend(a) => usd_send::lower(a, ctx),
        HyperliquidCoreAction::SpotSend(a) => spot_send::lower(a, ctx),
        HyperliquidCoreAction::UsdClassTransfer(a) => usd_class_transfer::lower(a, ctx),
        HyperliquidCoreAction::SendAsset(a) => send_asset::lower(a, ctx),
        HyperliquidCoreAction::SendToEvmWithData(a) => send_to_evm_with_data::lower(a, ctx),
        HyperliquidCoreAction::CDeposit(a) => c_deposit::lower(a, ctx),
        HyperliquidCoreAction::CWithdraw(a) => c_withdraw::lower(a, ctx),
        HyperliquidCoreAction::VaultTransfer(a) => vault_transfer::lower(a, ctx),
        HyperliquidCoreAction::SubAccountTransfer(a) => sub_account_transfer::lower(a, ctx),
        HyperliquidCoreAction::TokenDelegate(a) => token_delegate::lower(a, ctx),
        HyperliquidCoreAction::Unknown(a) => unknown::lower(a, ctx),
    }
}

/// Lower the venue identifier → `{ name: "hyperliquid" }` (`HyperliquidCore::HlVenue`).
/// Every CORE action is on the Hyperliquid venue, so a policy can scope on
/// `context.venue.name == "hyperliquid"` uniformly across all five actions.
pub(crate) fn hl_venue() -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String("hyperliquid".into()));
    Value::Object(m)
}

/// USDC decimals on Hyperliquid (the implied token of `usd_send` / `vault_transfer`
/// / `sub_account_transfer`, all USDC-denominated).
pub(crate) const HL_USDC_DECIMALS: u32 = 6;

/// HL `Core::TokenRef` for a spot token id (`"USDC"` / `"USDC:0x.."`). The
/// `standard = "hyperliquid"` arm of `Core::TokenKey` carries the raw HL token
/// id in `hlToken` (no on-chain ERC-20 address exists for an HL spot balance).
pub(crate) fn hl_token_ref(hl_token: &str) -> Value {
    let mut key = Map::new();
    key.insert("standard".into(), Value::String("hyperliquid".into()));
    key.insert("chain".into(), Value::String("hyperliquid:mainnet".into()));
    key.insert("hlToken".into(), Value::String(hl_token.to_owned()));
    let mut m = Map::new();
    m.insert("key".into(), Value::Object(key));
    Value::Object(m)
}

/// HL `Core::TokenRef` for USDC — the implied token of `usd_send` /
/// `vault_transfer` / `sub_account_transfer`.
pub(crate) fn hl_usdc_token_ref() -> Value {
    hl_token_ref("USDC")
}

/// HL `Staking::StakeVenue` (HYPE staking — `cDeposit` / `cWithdraw`). `name` is
/// a free string in `StakeVenue`, so no schema change is needed; only the
/// required `name` + `chain` are emitted (all contract-address arms are
/// Curve/Aave-specific and absent here).
pub(crate) fn hl_stake_venue() -> Value {
    let mut m = Map::new();
    m.insert("name".into(), Value::String("hyperliquid".into()));
    m.insert("chain".into(), Value::String("hyperliquid:mainnet".into()));
    Value::Object(m)
}
