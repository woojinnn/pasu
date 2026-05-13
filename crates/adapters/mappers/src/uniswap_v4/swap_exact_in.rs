//! V4 action SWAP_EXACT_IN (0x07).
//!
//! Latest schema (with minHopPriceX36): (currencyIn, PathKey[], uint256[] minHopPriceX36, uint128 amountIn, uint128 amountOutMinimum).
//! Older schema (no minHop): (currencyIn, PathKey[], uint128 amountIn, uint128 amountOutMinimum).

use alloy_primitives::U256;
use alloy_sol_types::{sol, SolValue};

use crate::context::{BuildContext, RawTx};
use crate::error::MapError;
use crate::types::actions::{SwapAction, SwapMode};
use crate::types::common::AmountConstraint;
use crate::types::envelope::ActionEnvelope;
use crate::uniswap_v4::common::{currency_to_asset, envelope_swap, pool_fee_to_bps, PathKey};

sol! {
    struct ExactInParamsV2 {
        address     currencyIn;
        PathKey[]   path;
        uint256[]   minHopPriceX36;
        uint128     amountIn;
        uint128     amountOutMinimum;
    }
    struct ExactInParamsV1 {
        address     currencyIn;
        PathKey[]   path;
        uint128     amountIn;
        uint128     amountOutMinimum;
    }
}

pub fn map_action(
    ctx: &BuildContext,
    _tx: &RawTx,
    params: &[u8],
) -> Result<Vec<ActionEnvelope>, MapError> {
    let (currency_in, path, amount_in, amount_out_minimum): (_, Vec<PathKey>, _, _) =
        if let Ok(p) = ExactInParamsV2::abi_decode(params, true) {
            let _: Vec<U256> = p.minHopPriceX36;
            (p.currencyIn, p.path, p.amountIn, p.amountOutMinimum)
        } else {
            let p = ExactInParamsV1::abi_decode(params, true)
                .map_err(|e| MapError::AbiDecode(e.to_string()))?;
            (p.currencyIn, p.path, p.amountIn, p.amountOutMinimum)
        };
    let token_in = currency_to_asset(ctx, currency_in);
    let last_hop_curr = path
        .last()
        .map(|k| k.intermediateCurrency)
        .ok_or(MapError::EmptyPath(0))?;
    let fee_bps = if path.len() == 1 {
        pool_fee_to_bps(path[0].fee.to::<u32>())
    } else {
        None
    };
    Ok(vec![envelope_swap(SwapAction {
        mode: SwapMode::ExactIn,
        token_in,
        token_out: currency_to_asset(ctx, last_hop_curr),
        amount_in: AmountConstraint::exact(amount_in.to_string()),
        amount_out: AmountConstraint::min(amount_out_minimum.to_string()),
        recipient: None,
        deadline_seconds_from_now: None,
        fee_bps,
        slippage_bps: None,
        value_in_usd: None,
        min_value_out_usd: None,
        expected_value_out_usd: None,
    })])
}
