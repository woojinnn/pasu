use crate::action::lending::{LiquidateAction, LiquidateMode, LiquidationKind};
use crate::context_keys::RECIPIENT;
use crate::lowering::common::amount::amount_constraint_json;
use crate::lowering::common::asset::{asset_ref_json, LoweringError};
use crate::lowering::dispatch::{Lower, LoweringCtx};
use crate::lowering::lending::market_json;
use crate::policy::PolicyRequest;
use serde_json::{Map, Value};

const ACTION_ID: &str = "liquidate";
const MARKET: &str = "market";
const BORROWER: &str = "borrower";
const COLLATERAL_ASSET: &str = "collateralAsset";
const DEBT_ASSET: &str = "debtAsset";
const DEBT_TO_COVER: &str = "debtToCover";
const SEIZED_COLLATERAL_AMOUNT: &str = "seizedCollateralAmount";
const LIQUIDATION_KIND: &str = "liquidationKind";
const LIQUIDATE_MODE: &str = "liquidateMode";
const RECEIVE_A_TOKEN: &str = "receiveAToken";

impl Lower for LiquidateAction {
    fn build(&self, ctx: &LoweringCtx<'_>) -> Result<PolicyRequest, LoweringError> {
        Ok(ctx.request(ACTION_ID, context(self)?))
    }
}

fn context(action: &LiquidateAction) -> Result<Value, LoweringError> {
    let mut context = Map::new();
    if let Some(market) = &action.market {
        context.insert(MARKET.into(), market_json(market));
    }
    context.insert(BORROWER.into(), Value::from(action.borrower.to_string()));
    if let Some(collateral_asset) = &action.collateral_asset {
        context.insert(COLLATERAL_ASSET.into(), asset_ref_json(collateral_asset)?);
    }
    context.insert(DEBT_ASSET.into(), asset_ref_json(&action.debt_asset)?);
    if let Some(debt_to_cover) = &action.debt_to_cover {
        context.insert(DEBT_TO_COVER.into(), amount_constraint_json(debt_to_cover));
    }
    if let Some(seized) = &action.seized_collateral_amount {
        context.insert(
            SEIZED_COLLATERAL_AMOUNT.into(),
            amount_constraint_json(seized),
        );
    }
    context.insert(
        LIQUIDATION_KIND.into(),
        Value::from(liquidation_kind_str(&action.liquidation_kind)),
    );
    if let Some(mode) = &action.liquidate_mode {
        context.insert(
            LIQUIDATE_MODE.into(),
            Value::from(liquidate_mode_str(mode)),
        );
    }
    if let Some(recipient) = &action.recipient {
        context.insert(RECIPIENT.into(), Value::from(recipient.to_string()));
    }
    if let Some(receive_a_token) = action.receive_a_token {
        context.insert(RECEIVE_A_TOKEN.into(), Value::from(receive_a_token));
    }
    Ok(Value::Object(context))
}

const fn liquidation_kind_str(kind: &LiquidationKind) -> &'static str {
    match kind {
        LiquidationKind::PoolShare => "pool_share",
        LiquidationKind::ProtocolAbsorb => "protocol_absorb",
        LiquidationKind::Socializable => "socializable",
        LiquidationKind::SingleAsset => "single_asset",
    }
}

const fn liquidate_mode_str(mode: &LiquidateMode) -> &'static str {
    match mode {
        LiquidateMode::SingleStep => "single_step",
        LiquidateMode::Seize => "seize",
        LiquidateMode::Repay => "repay",
    }
}
