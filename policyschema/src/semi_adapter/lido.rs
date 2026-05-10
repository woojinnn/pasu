//! Lido decoder — submit/requestWithdrawals/claimWithdrawal + wstETH wrap/unwrap.

use serde_json::Value;

use crate::action::fields::{
    HopRef, LiquidStakingFields, SettlementKind, SlippageInfo, SlippageSource, SwapFields,
    SwapMode, SwapRoute,
};
use crate::confidence::Confidence;
use crate::semi_adapter::common::{amount_exact, as_address, as_uint_string, recipients_from};
use crate::semi_adapter::error::SemiAdapterError;
use crate::semi_adapter::registry::token_metadata;
use crate::semi_adapter::BuildContext;
use crate::types::{AmountKind, AmountSpec, DeadlineFields, Token};

pub const SEL_SUBMIT: [u8; 4] = [0xa1, 0x90, 0x3e, 0xab];
pub const SEL_REQUEST_WITHDRAWALS: [u8; 4] = [0x55, 0xed, 0x4a, 0xd9];
pub const SEL_WRAP_WSTETH: [u8; 4] = [0xea, 0x59, 0x8c, 0xb0];
pub const SEL_UNWRAP_WSTETH: [u8; 4] = [0xde, 0x0e, 0x9a, 0x3e];

const ETH: &str = "0x0000000000000000000000000000000000000000";
const STETH: &str = "0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84";
const WSTETH: &str = "0x7f39C581F595B53c5cb19bD0b3f8dA6c935E2Ca0";

/// `submit(_referral)` payable — Stake.
pub fn build_lido_stake_fields(
    args: &Value,
    ctx: &BuildContext,
) -> Result<LiquidStakingFields, SemiAdapterError> {
    let referral_addr = args
        .get("_referral")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok());
    // address(0) → None
    let referral = referral_addr.filter(|a: &crate::types::Address| {
        format!("{a:#x}") != ETH
    });

    Ok(LiquidStakingFields {
        actor: ctx.actor,
        protocol_ids: vec!["lido".into()],
        asset_in: token_metadata(ETH.parse().unwrap(), ctx.chain_id),
        asset_out: Some(token_metadata(STETH.parse().unwrap(), ctx.chain_id)),
        amount: amount_exact(ctx.value_wei.clone()),
        referral,
        withdrawal_request_id: None,
        recipients: recipients_from(None, ctx.actor),
    })
}

/// `requestWithdrawals(amounts[], _owner)`.
pub fn build_lido_request_withdrawal_fields(
    args: &Value,
    ctx: &BuildContext,
) -> Result<LiquidStakingFields, SemiAdapterError> {
    let amounts = args
        .get("_amounts")
        .and_then(|v| v.as_array())
        .ok_or(SemiAdapterError::MissingArg { name: "_amounts" })?;
    let mut sum: u128 = 0;
    for v in amounts {
        if let Some(s) = v.as_str() {
            if let Ok(n) = s.parse::<u128>() {
                sum = sum.saturating_add(n);
            }
        }
    }
    let owner = as_address(args, "_owner")?;

    Ok(LiquidStakingFields {
        actor: ctx.actor,
        protocol_ids: vec!["lido".into()],
        asset_in: token_metadata(STETH.parse().unwrap(), ctx.chain_id),
        asset_out: None,
        amount: amount_exact(sum.to_string()),
        referral: None,
        withdrawal_request_id: None,
        recipients: recipients_from(Some(owner), ctx.actor),
    })
}

/// wstETH `wrap(stETHAmount)` / `unwrap(wstETHAmount)` → SwapFields (1:1).
pub fn build_wsteth_wrap_fields(
    selector: &[u8; 4],
    args: &Value,
    ctx: &BuildContext,
    is_wrap: bool,
) -> Result<SwapFields, SemiAdapterError> {
    let amount_arg = if is_wrap { "_stETHAmount" } else { "_wstETHAmount" };
    let amount = as_uint_string(args, amount_arg)?;

    let (in_addr, out_addr) = if is_wrap {
        (STETH, WSTETH)
    } else {
        (WSTETH, STETH)
    };
    let token_in: Token = token_metadata(in_addr.parse().unwrap(), ctx.chain_id);
    let token_out: Token = token_metadata(out_addr.parse().unwrap(), ctx.chain_id);

    let _ = selector; // selector는 caller가 매칭

    let hop = HopRef {
        id: "h#0".into(),
        protocol: "lido".into(),
        token_in: token_in.clone(),
        token_out: token_out.clone(),
        pool: None,
        fee_bps: Some(0),
        confidence: Confidence::High,
    };

    Ok(SwapFields {
        actor: ctx.actor,
        protocol_ids: vec!["lido".into()],
        input_tokens: vec![token_in],
        output_tokens: vec![token_out],
        mode: SwapMode::ExactIn,
        amount_in: amount_exact(amount.clone()),
        amount_out: AmountSpec {
            raw: amount,
            kind: AmountKind::Exact,
        },
        route: SwapRoute::SingleHop { hop },
        slippage: SlippageInfo {
            source: SlippageSource::Unspecified,
            amount_out_min: None,
        },
        settlement: SettlementKind::Router,
        recipients: recipients_from(None, ctx.actor),
        deadlines: DeadlineFields {
            deadline: None,
            deadline_horizon_seconds: None,
        },
        max_fee_bps: Some(0),
        has_zero_min_output: false,
    })
}
