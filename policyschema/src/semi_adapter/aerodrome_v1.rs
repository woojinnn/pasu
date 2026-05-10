//! Aerodrome V1 (Base 체인의 Solidly fork) decoder.
//!
//! V2와 달리 path entry가 **3-tuple `(from, to, stable)`**.

use serde_json::Value;

use crate::action::fields::{
    HopRef, SettlementKind, SlippageInfo, SlippageSource, SwapFields, SwapMode, SwapRoute,
};
use crate::confidence::Confidence;
use crate::semi_adapter::common::{
    amount_exact, amount_min, as_address, as_u64, as_uint_string, deadline_horizon, recipients_from,
};
use crate::semi_adapter::error::SemiAdapterError;
use crate::semi_adapter::registry::token_metadata;
use crate::semi_adapter::BuildContext;
use crate::types::{Address, DeadlineFields};

pub const SEL_SWAP_EXACT_TOKENS_FOR_TOKENS: [u8; 4] = [0xca, 0xc8, 0x8e, 0xa9];

/// stable 풀이면 1bps, volatile이면 30bps.
fn stable_to_fee_bps(stable: bool) -> u32 {
    if stable {
        1
    } else {
        30
    }
}

/// Aerodrome V1 `swapExactTokensForTokens(amountIn, amountOutMin, routes, to, deadline)`.
///
/// `routes`는 `[{from, to, stable, factory}, ...]` 객체 배열.
pub fn build_aerodrome_v1_swap_fields(
    selector: &[u8; 4],
    args: &Value,
    ctx: &BuildContext,
) -> Result<SwapFields, SemiAdapterError> {
    if *selector != SEL_SWAP_EXACT_TOKENS_FOR_TOKENS {
        return Err(SemiAdapterError::BadSelector {
            expected: format!("0x{}", hex::encode(SEL_SWAP_EXACT_TOKENS_FOR_TOKENS)),
            got: format!("0x{}", hex::encode(selector)),
        });
    }

    let amount_in = as_uint_string(args, "amountIn")?;
    let amount_out_min = as_uint_string(args, "amountOutMin")?;
    let recipient = as_address(args, "to")?;
    let deadline = as_u64(args, "deadline")?;

    let routes_arr = args
        .get("routes")
        .and_then(|v| v.as_array())
        .ok_or(SemiAdapterError::MissingArg { name: "routes" })?;
    if routes_arr.is_empty() {
        return Err(SemiAdapterError::AbiDecode {
            reason: "routes empty".into(),
        });
    }

    let mut hops: Vec<HopRef> = Vec::with_capacity(routes_arr.len());
    let mut max_fee: u32 = 0;
    for (i, r) in routes_arr.iter().enumerate() {
        let from: Address = r
            .get("from")
            .and_then(|v| v.as_str())
            .ok_or(SemiAdapterError::MissingArg { name: "routes.from" })?
            .parse()
            .map_err(|_| SemiAdapterError::BadAddress {
                value: "routes.from".into(),
            })?;
        let to: Address = r
            .get("to")
            .and_then(|v| v.as_str())
            .ok_or(SemiAdapterError::MissingArg { name: "routes.to" })?
            .parse()
            .map_err(|_| SemiAdapterError::BadAddress {
                value: "routes.to".into(),
            })?;
        let stable = r.get("stable").and_then(|v| v.as_bool()).unwrap_or(false);
        let fee_bps = stable_to_fee_bps(stable);
        max_fee = max_fee.max(fee_bps);
        hops.push(HopRef {
            id: format!("h#{i}"),
            protocol: "aerodrome.v1".into(),
            token_in: token_metadata(from, ctx.chain_id),
            token_out: token_metadata(to, ctx.chain_id),
            pool: None,
            fee_bps: Some(fee_bps),
            confidence: Confidence::High,
        });
    }

    let input_token = hops.first().unwrap().token_in.clone();
    let output_token = hops.last().unwrap().token_out.clone();

    let route = if hops.len() == 1 {
        SwapRoute::SingleHop {
            hop: hops.into_iter().next().unwrap(),
        }
    } else {
        SwapRoute::MultiHop { hops }
    };

    let amount_out = amount_min(amount_out_min);
    let has_zero_min_output = amount_out.raw == "0";

    Ok(SwapFields {
        actor: ctx.actor,
        protocol_ids: vec!["aerodrome.v1".into()],
        input_tokens: vec![input_token],
        output_tokens: vec![output_token],
        mode: SwapMode::ExactIn,
        amount_in: amount_exact(amount_in),
        amount_out: amount_out.clone(),
        route,
        slippage: SlippageInfo {
            source: SlippageSource::Calldata,
            amount_out_min: Some(amount_out),
        },
        settlement: SettlementKind::Router,
        recipients: recipients_from(Some(recipient), ctx.actor),
        deadlines: DeadlineFields {
            deadline: Some(deadline),
            deadline_horizon_seconds: deadline_horizon(deadline, ctx.block_timestamp),
        },
        max_fee_bps: Some(max_fee),
        has_zero_min_output,
    })
}
