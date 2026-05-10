//! Morpho Blue decoder. `marketParams` 5튜플로 시장 식별.

use serde_json::Value;

use crate::action::fields::LendingFields;
use crate::semi_adapter::common::{amount_with_unlimited_check, as_address, recipients_from};
use crate::semi_adapter::error::SemiAdapterError;
use crate::semi_adapter::registry::token_metadata;
use crate::semi_adapter::BuildContext;
use crate::types::{Address, AmountKind, AmountSpec};

pub const SEL_SUPPLY: [u8; 4] = [0x23, 0x8d, 0x65, 0x79];

#[derive(Debug, Clone)]
pub struct MarketParams {
    pub loan_token: Address,
    pub collateral_token: Address,
    pub oracle: Address,
    pub irm: Address,
    pub lltv: String,
}

pub fn parse_market_params(mp: &Value) -> Result<MarketParams, SemiAdapterError> {
    let loan_token: Address = mp
        .get("loanToken")
        .and_then(|v| v.as_str())
        .ok_or(SemiAdapterError::MissingArg { name: "marketParams.loanToken" })?
        .parse()
        .map_err(|_| SemiAdapterError::BadAddress {
            value: "marketParams.loanToken".into(),
        })?;
    let collateral_token: Address = mp
        .get("collateralToken")
        .and_then(|v| v.as_str())
        .ok_or(SemiAdapterError::MissingArg { name: "marketParams.collateralToken" })?
        .parse()
        .map_err(|_| SemiAdapterError::BadAddress {
            value: "marketParams.collateralToken".into(),
        })?;
    let oracle: Address = mp
        .get("oracle")
        .and_then(|v| v.as_str())
        .ok_or(SemiAdapterError::MissingArg { name: "marketParams.oracle" })?
        .parse()
        .map_err(|_| SemiAdapterError::BadAddress {
            value: "marketParams.oracle".into(),
        })?;
    let irm: Address = mp
        .get("irm")
        .and_then(|v| v.as_str())
        .ok_or(SemiAdapterError::MissingArg { name: "marketParams.irm" })?
        .parse()
        .map_err(|_| SemiAdapterError::BadAddress {
            value: "marketParams.irm".into(),
        })?;
    let lltv = mp
        .get("lltv")
        .and_then(|v| v.as_str())
        .ok_or(SemiAdapterError::MissingArg { name: "marketParams.lltv" })?
        .to_string();
    Ok(MarketParams { loan_token, collateral_token, oracle, irm, lltv })
}

pub fn build_morpho_supply_fields(
    args: &Value,
    ctx: &BuildContext,
) -> Result<LendingFields, SemiAdapterError> {
    let mp = args
        .get("marketParams")
        .ok_or(SemiAdapterError::MissingArg { name: "marketParams" })?;
    let market = parse_market_params(mp)?;

    let assets = args
        .get("assets")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| "0".into());
    let _shares = args
        .get("shares")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| "0".into());

    // assets / shares 중 nonzero 사용
    let amount = if assets != "0" {
        AmountSpec { raw: assets, kind: AmountKind::Exact }
    } else {
        amount_with_unlimited_check(_shares)
    };

    Ok(LendingFields {
        actor: ctx.actor,
        protocol_ids: vec!["morpho.blue".into()],
        asset: token_metadata(market.loan_token, ctx.chain_id),
        amount,
        on_behalf_of: as_address(args, "onBehalf")?,
        interest_rate_mode: None,
        use_as_collateral: None,
        e_mode_category_id: None,
        liquidation_target: None,
        collateral_asset: Some(token_metadata(market.collateral_token, ctx.chain_id)),
        flash_assets: None,
        flash_amounts: None,
        flash_modes: None,
        recipients: recipients_from(None, ctx.actor),
    })
}
