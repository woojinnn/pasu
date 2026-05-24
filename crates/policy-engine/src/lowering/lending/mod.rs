//! Per-action lowering for lending actions.
//!
//! Each submodule provides an `impl Lower for <Action>` so the dispatcher in
//! [`crate::lowering::dispatch`] can call `action.build(&ctx)` uniformly.
//! Mirrors the structure of [`crate::lowering::dex`].

use crate::action::lending::MarketRef;
use crate::context_keys::{ADDRESS, ID, LABEL};
use serde_json::{Map, Value};

pub(crate) mod borrow;
pub(crate) mod liquidate;
pub(crate) mod repay;
pub(crate) mod supply;

/// Render a `MarketRef` as a Cedar `Pool`-shaped sub-record.
///
/// The `borrow` / `repay` / `liquidate` cedarschemas all declare `market?:
/// Pool`, which expects `{ address, id?, label? }`. The schema type was reused
/// from DEX so the rendered JSON shape must stay identical to
/// [`crate::lowering::common::pool::pool_json`].
pub(crate) fn market_json(market: &MarketRef) -> Value {
    let mut out = Map::new();
    if let Some(address) = &market.address {
        out.insert(ADDRESS.into(), Value::from(address.to_string()));
    }
    if let Some(id) = &market.id {
        out.insert(ID.into(), Value::from(id.to_string()));
    }
    if let Some(label) = &market.label {
        out.insert(LABEL.into(), Value::from(label.as_str()));
    }
    Value::Object(out)
}
