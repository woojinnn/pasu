//! Shared types/helpers for V4 swap action mappers.
//!
//! V4 actions are NOT standalone Solidity functions — each is an entry inside
//! the Universal Router `V4_SWAP` command's `(bytes actions, bytes[] params)`
//! input. A V4 mapper receives `params[i]` (`abi.encode(SomeParams)`) and
//! decodes it via `SolValue::abi_decode`.

use alloy_primitives::Address as AlloyAddress;
use alloy_sol_types::sol;

use crate::context::BuildContext;
use crate::types::actions::SwapAction;
use crate::types::common::AssetRef;
use crate::types::envelope::{ActionEnvelope, ActionFields, Category};

pub const V4_DYNAMIC_FEE_FLAG: u32 = 0x800000;

sol! {
    #[derive(Debug)]
    struct PoolKey {
        address currency0;
        address currency1;
        uint24  fee;
        int24   tickSpacing;
        address hooks;
    }

    #[derive(Debug)]
    struct PathKey {
        address intermediateCurrency;
        uint24  fee;
        int24   tickSpacing;
        address hooks;
        bytes   hookData;
    }
}

/// V4 native currency is address(0).
pub fn currency_to_asset(ctx: &BuildContext, c: AlloyAddress) -> AssetRef {
    if c == AlloyAddress::ZERO {
        ctx.tokens.native(ctx.chain_id)
    } else {
        ctx.tokens.erc20(ctx.chain_id, c)
    }
}

/// `fee` (uint24) → bps. Returns `None` if pool has dynamic fee.
pub fn pool_fee_to_bps(fee: u32) -> Option<u32> {
    if fee == V4_DYNAMIC_FEE_FLAG {
        None
    } else {
        Some(fee / 100)
    }
}

pub fn envelope_swap(action: SwapAction) -> ActionEnvelope {
    ActionEnvelope::new(Category::Dex, ActionFields::Swap(action))
}
