//! Aave V3 Pool decoder (이자 핵심 4종).

use serde_json::Value;

use crate::action::fields::{InterestRateMode, LendingFields};
use crate::semi_adapter::common::{
    amount_with_unlimited_check, as_address, as_uint_string, recipients_from,
};
use crate::semi_adapter::error::SemiAdapterError;
use crate::semi_adapter::registry::token_metadata;
use crate::semi_adapter::BuildContext;
use crate::types::{AmountKind, AmountSpec, RecipientFields};

pub const SEL_SUPPLY: [u8; 4] = [0x61, 0x7b, 0xa0, 0x37];
pub const SEL_WITHDRAW: [u8; 4] = [0x69, 0x32, 0x8d, 0xec];
pub const SEL_BORROW: [u8; 4] = [0xa4, 0x15, 0xbc, 0xad];
pub const SEL_REPAY: [u8; 4] = [0x57, 0x3a, 0xde, 0x81];

fn rate_mode_from(n: u64) -> Option<InterestRateMode> {
    match n {
        1 => Some(InterestRateMode::Stable),
        2 => Some(InterestRateMode::Variable),
        _ => None,
    }
}

pub fn build_aave_v3_lending_fields(
    selector: &[u8; 4],
    args: &Value,
    ctx: &BuildContext,
) -> Result<LendingFields, SemiAdapterError> {
    let asset_addr = as_address(args, "asset")?;
    let asset = token_metadata(asset_addr, ctx.chain_id);
    let amount_raw = as_uint_string(args, "amount")?;

    match *selector {
        SEL_SUPPLY => Ok(LendingFields {
            actor: ctx.actor,
            protocol_ids: vec!["aave.v3".into()],
            asset,
            amount: AmountSpec { raw: amount_raw, kind: AmountKind::Exact },
            on_behalf_of: as_address(args, "onBehalfOf")?,
            interest_rate_mode: None,
            use_as_collateral: None,
            e_mode_category_id: None,
            liquidation_target: None,
            collateral_asset: None,
            flash_assets: None,
            flash_amounts: None,
            flash_modes: None,
            recipients: recipients_from(None, ctx.actor),
        }),
        SEL_WITHDRAW => {
            let to = as_address(args, "to")?;
            Ok(LendingFields {
                actor: ctx.actor,
                protocol_ids: vec!["aave.v3".into()],
                asset,
                amount: amount_with_unlimited_check(amount_raw),
                on_behalf_of: ctx.actor,
                interest_rate_mode: None,
                use_as_collateral: None,
                e_mode_category_id: None,
                liquidation_target: None,
                collateral_asset: None,
                flash_assets: None,
                flash_amounts: None,
                flash_modes: None,
                recipients: recipients_from(Some(to), ctx.actor),
            })
        }
        SEL_BORROW => {
            let mode_n = args
                .get("interestRateMode")
                .and_then(|v| v.as_u64())
                .ok_or(SemiAdapterError::MissingArg { name: "interestRateMode" })?;
            Ok(LendingFields {
                actor: ctx.actor,
                protocol_ids: vec!["aave.v3".into()],
                asset,
                amount: AmountSpec { raw: amount_raw, kind: AmountKind::Exact },
                on_behalf_of: as_address(args, "onBehalfOf")?,
                interest_rate_mode: rate_mode_from(mode_n),
                use_as_collateral: None,
                e_mode_category_id: None,
                liquidation_target: None,
                collateral_asset: None,
                flash_assets: None,
                flash_amounts: None,
                flash_modes: None,
                recipients: recipients_from(None, ctx.actor),
            })
        }
        SEL_REPAY => {
            let mode_n = args
                .get("rateMode")
                .and_then(|v| v.as_u64())
                .ok_or(SemiAdapterError::MissingArg { name: "rateMode" })?;
            Ok(LendingFields {
                actor: ctx.actor,
                protocol_ids: vec!["aave.v3".into()],
                asset,
                amount: amount_with_unlimited_check(amount_raw),
                on_behalf_of: as_address(args, "onBehalfOf")?,
                interest_rate_mode: rate_mode_from(mode_n),
                use_as_collateral: None,
                e_mode_category_id: None,
                liquidation_target: None,
                collateral_asset: None,
                flash_assets: None,
                flash_amounts: None,
                flash_modes: None,
                recipients: recipients_from(None, ctx.actor),
            })
        }
        _ => Err(SemiAdapterError::BadSelector {
            expected: "Aave V3 selector".into(),
            got: format!("0x{}", hex::encode(selector)),
        }),
    }
}

// 사용 안 하는 import 정리 (build warning 방지용 dummy)
#[allow(dead_code)]
fn _suppress_unused(_r: RecipientFields) {}
