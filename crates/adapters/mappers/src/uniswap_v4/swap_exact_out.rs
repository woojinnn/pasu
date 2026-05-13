//! V4 action SWAP_EXACT_OUT (0x09).

use alloy_primitives::U256;
use alloy_sol_types::{sol, SolValue};

use crate::context::{BuildContext, RawTx};
use crate::error::MapError;
use crate::types::actions::{SwapAction, SwapMode};
use crate::types::common::AmountConstraint;
use crate::types::envelope::ActionEnvelope;
use crate::uniswap_v4::common::{currency_to_asset, envelope_swap, pool_fee_to_bps, PathKey};

sol! {
    struct ExactOutParamsV2 {
        address     currencyOut;
        PathKey[]   path;
        uint256[]   minHopPriceX36;
        uint128     amountOut;
        uint128     amountInMaximum;
    }
    struct ExactOutParamsV1 {
        address     currencyOut;
        PathKey[]   path;
        uint128     amountOut;
        uint128     amountInMaximum;
    }
}

pub fn map_action(
    ctx: &BuildContext,
    _tx: &RawTx,
    params: &[u8],
) -> Result<Vec<ActionEnvelope>, MapError> {
    let (currency_out, path, amount_out, amount_in_maximum): (_, Vec<PathKey>, _, _) =
        if let Ok(p) = ExactOutParamsV2::abi_decode(params, true) {
            let _: Vec<U256> = p.minHopPriceX36;
            (p.currencyOut, p.path, p.amountOut, p.amountInMaximum)
        } else {
            let p = ExactOutParamsV1::abi_decode(params, true)
                .map_err(|e| MapError::AbiDecode(e.to_string()))?;
            (p.currencyOut, p.path, p.amountOut, p.amountInMaximum)
        };
    let token_out = currency_to_asset(ctx, currency_out);
    let first_hop_curr = path
        .first()
        .map(|k| k.intermediateCurrency)
        .ok_or(MapError::EmptyPath(0))?;
    let fee_bps = if path.len() == 1 {
        pool_fee_to_bps(path[0].fee.to::<u32>())
    } else {
        None
    };
    Ok(vec![envelope_swap(SwapAction {
        mode: SwapMode::ExactOut,
        token_in: currency_to_asset(ctx, first_hop_curr),
        token_out,
        amount_in: AmountConstraint::max(amount_in_maximum.to_string()),
        amount_out: AmountConstraint::exact(amount_out.to_string()),
        recipient: None,
        deadline_seconds_from_now: None,
        fee_bps,
        slippage_bps: None,
        value_in_usd: None,
        min_value_out_usd: None,
        expected_value_out_usd: None,
    })])
}
