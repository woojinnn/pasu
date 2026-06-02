//! Hyperliquid CORE domain lowering: thin actions with NO live inputs.
//!
//! Mirrors the perp/token fan-out: a per-action `lower` leaf for each variant,
//! plus the shared `hl_venue` / `hl_market` encoders. Every action lowers to a
//! `HyperliquidCore::*Context` whose numeric fields (price / size / amount) are
//! emitted as decimal STRINGS — fractional-safe and free of Cedar's `decimal`
//! 4-dp limit. Policies match on action type + `context.venue.name` +
//! side / destination, not on numeric magnitude.

use serde_json::{Map, Value};

use policy_transition::action::hyperliquid_core::HyperliquidCoreAction;

use super::dispatch::{LowerCtx, LowerError, LoweredAction};

mod approve_agent;
mod approve_builder_fee;
mod c_deposit;
mod c_withdraw;
mod order;
mod send_asset;
mod send_to_evm_with_data;
mod spot_send;
mod sub_account_transfer;
mod token_delegate;
mod twap_order;
mod unknown;
mod update_isolated_margin;
mod update_leverage;
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
        HyperliquidCoreAction::Order(a) => order::lower(a, ctx),
        HyperliquidCoreAction::UpdateLeverage(a) => update_leverage::lower(a, ctx),
        HyperliquidCoreAction::Withdraw(a) => withdraw::lower(a, ctx),
        HyperliquidCoreAction::UsdSend(a) => usd_send::lower(a, ctx),
        HyperliquidCoreAction::ApproveAgent(a) => approve_agent::lower(a, ctx),
        HyperliquidCoreAction::SpotSend(a) => spot_send::lower(a, ctx),
        HyperliquidCoreAction::UsdClassTransfer(a) => usd_class_transfer::lower(a, ctx),
        HyperliquidCoreAction::SendAsset(a) => send_asset::lower(a, ctx),
        HyperliquidCoreAction::SendToEvmWithData(a) => send_to_evm_with_data::lower(a, ctx),
        HyperliquidCoreAction::CDeposit(a) => c_deposit::lower(a, ctx),
        HyperliquidCoreAction::CWithdraw(a) => c_withdraw::lower(a, ctx),
        HyperliquidCoreAction::VaultTransfer(a) => vault_transfer::lower(a, ctx),
        HyperliquidCoreAction::SubAccountTransfer(a) => sub_account_transfer::lower(a, ctx),
        HyperliquidCoreAction::ApproveBuilderFee(a) => approve_builder_fee::lower(a, ctx),
        HyperliquidCoreAction::TokenDelegate(a) => token_delegate::lower(a, ctx),
        HyperliquidCoreAction::TwapOrder(a) => twap_order::lower(a, ctx),
        HyperliquidCoreAction::UpdateIsolatedMargin(a) => update_isolated_margin::lower(a, ctx),
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

/// Lower a market reference → `{ symbol, assetIndex }` (`HyperliquidCore::HlMarket`).
/// `symbol` falls back to `ASSET-<index>` when the venue meta cache has not yet
/// resolved the numeric `assetIndex` to a human symbol.
pub(crate) fn hl_market(asset_index: u32, symbol: Option<&str>) -> Value {
    let mut m = Map::new();
    let sym = symbol.map_or_else(|| format!("ASSET-{asset_index}"), str::to_owned);
    m.insert("symbol".into(), Value::String(sym));
    m.insert("assetIndex".into(), Value::from(i64::from(asset_index)));
    Value::Object(m)
}
