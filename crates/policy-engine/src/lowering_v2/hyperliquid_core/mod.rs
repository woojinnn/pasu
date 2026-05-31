//! Hyperliquid CORE domain lowering: thin actions with NO live inputs.
//!
//! Mirrors the perp/token fan-out: a per-action `lower` leaf for each variant,
//! plus the shared `hl_venue` / `hl_market` encoders. Every action lowers to a
//! `HyperliquidCore::*Context` whose numeric fields (price / size / amount) are
//! emitted as decimal STRINGS — fractional-safe and free of Cedar's `decimal`
//! 4-dp limit. Policies match on action type + `context.venue.name` +
//! side / destination, not on numeric magnitude.

use serde_json::{Map, Value};

use simulation_reducer::action::hyperliquid_core::HyperliquidCoreAction;

use super::dispatch::{LowerCtx, LowerError, LoweredAction};

mod approve_agent;
mod order;
mod update_leverage;
mod usd_send;
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
