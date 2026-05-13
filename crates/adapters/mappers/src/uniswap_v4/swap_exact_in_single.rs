//! V4 action SWAP_EXACT_IN_SINGLE (0x06).
//!
//! Two schemas in production:
//!
//! ```text
//! Latest: (PoolKey, bool, uint128 amountIn, uint128 amountOutMinimum,
//!          uint256 minHopPriceX36, bytes hookData)
//! Older:  (PoolKey, bool, uint128 amountIn, uint128 amountOutMinimum,
//!          bytes hookData)
//! ```

use alloy_sol_types::{sol, SolValue};

use crate::context::{BuildContext, RawTx};
use crate::error::MapError;
use crate::types::actions::{SwapAction, SwapMode};
use crate::types::common::AmountConstraint;
use crate::types::envelope::ActionEnvelope;
use crate::uniswap_v4::common::{currency_to_asset, envelope_swap, pool_fee_to_bps, PoolKey};

sol! {
    struct ExactInSingleParamsV2 {
        PoolKey poolKey;
        bool    zeroForOne;
        uint128 amountIn;
        uint128 amountOutMinimum;
        uint256 minHopPriceX36;
        bytes   hookData;
    }
    struct ExactInSingleParamsV1 {
        PoolKey poolKey;
        bool    zeroForOne;
        uint128 amountIn;
        uint128 amountOutMinimum;
        bytes   hookData;
    }
}

pub fn map_action(
    ctx: &BuildContext,
    _tx: &RawTx,
    params: &[u8],
) -> Result<Vec<ActionEnvelope>, MapError> {
    let (pool_key, zero_for_one, amount_in, amount_out_minimum) =
        if let Ok(p) = ExactInSingleParamsV2::abi_decode(params, true) {
            (p.poolKey, p.zeroForOne, p.amountIn, p.amountOutMinimum)
        } else {
            let p = ExactInSingleParamsV1::abi_decode(params, true)
                .map_err(|e| MapError::AbiDecode(e.to_string()))?;
            (p.poolKey, p.zeroForOne, p.amountIn, p.amountOutMinimum)
        };
    let (in_c, out_c) = if zero_for_one {
        (pool_key.currency0, pool_key.currency1)
    } else {
        (pool_key.currency1, pool_key.currency0)
    };
    Ok(vec![envelope_swap(SwapAction {
        mode: SwapMode::ExactIn,
        token_in: currency_to_asset(ctx, in_c),
        token_out: currency_to_asset(ctx, out_c),
        amount_in: AmountConstraint::exact(amount_in.to_string()),
        amount_out: AmountConstraint::min(amount_out_minimum.to_string()),
        recipient: None,
        deadline_seconds_from_now: None,
        fee_bps: pool_fee_to_bps(pool_key.fee.to::<u32>()),
        slippage_bps: None,
        value_in_usd: None,
        min_value_out_usd: None,
        expected_value_out_usd: None,
    })])
}
