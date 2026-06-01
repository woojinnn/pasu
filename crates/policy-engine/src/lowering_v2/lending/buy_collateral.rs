//! `Lending::BuyCollateral` lowering → `Lending::BuyCollateralContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::lending::BuyCollateralAction;

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::lower_token_ref;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::lower_lending_venue;

/// Lower a `Lending::BuyCollateral` action into the Cedar context shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// lowering contract.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower(
    action: &BuyCollateralAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_lending_venue(&action.venue));
    m.insert(
        "collateralAsset".into(),
        lower_token_ref(&action.collateral_asset),
    );
    m.insert("baseAsset".into(), lower_token_ref(&action.base_asset));
    m.insert(
        "minCollateralAmount".into(),
        Value::String(u256_hex(action.min_collateral_amount)),
    );
    m.insert(
        "baseAmount".into(),
        Value::String(u256_hex(action.base_amount)),
    );
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));

    Ok(ctx.lowered(r#"Lending::Action::"BuyCollateral""#, Value::Object(m)))
}
