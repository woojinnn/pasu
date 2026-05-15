//! Shared lowering helpers for restaking action submodules.

use crate::action::restaking::StrategyRef;
use crate::context_keys::{ADDRESS, ID, LABEL};
use serde_json::{Map, Value};

/// Serialize a [`StrategyRef`] into the `Pool`-shaped JSON the Cedar schema
/// expects (`address`, optional `id`, optional `label`).
///
/// `address` is technically required by the schema, but every other JSON shape
/// in the workspace silently omits the field when missing — mirrors
/// [`crate::lowering::common::pool::pool_json`] and the lending
/// `market_json` helper.
pub(crate) fn strategy_json(strategy: &StrategyRef) -> Value {
    let mut out = Map::new();
    if let Some(address) = &strategy.address {
        out.insert(ADDRESS.into(), Value::from(address.to_string()));
    }
    if let Some(id) = &strategy.id {
        out.insert(ID.into(), Value::from(id.to_string()));
    }
    if let Some(label) = &strategy.label {
        out.insert(LABEL.into(), Value::from(label.as_str()));
    }
    Value::Object(out)
}
