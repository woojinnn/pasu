//! UR opcode `UNWRAP_WETH` → `Action::Unwrap`.
//!
//! Opcode signature: `(address recipient, uint256 amountMin)`.
//! Semantics: take WETH from the router's balance (typically the output of
//! a preceding SWAP that landed on the router) and unwrap to native ETH
//! credited to `recipient` (usually the original user via `0x...01`).

use std::sync::Arc;

use abi_resolver::ids::UR_UNWRAP_WETH_DECODER_ID;
use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::action::common::{AmountConstraint, AmountKind};
use policy_engine::action::envelope::{Action, ActionEnvelope, Category};
use policy_engine::action::misc::UnwrapAction;

use crate::mapper::{MapContext, Mapper, MapperError, MapperId, MapperMatchKey};

use super::common::{
    asset_with_amount, decimal_from_uint, find_address, find_uint, map_recipient, native_eth,
    wrapped_weth,
};

pub const UR_UNWRAP_WETH_MAPPER_ID: &str = "uniswap-ur/UNWRAP_WETH";

#[derive(Debug, Clone, Copy, Default)]
pub struct UrUnwrapWethMapper;

impl UrUnwrapWethMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for UrUnwrapWethMapper {
    fn id(&self) -> MapperId {
        MapperId::new(UR_UNWRAP_WETH_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id.as_str() == UR_UNWRAP_WETH_DECODER_ID
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let recipient = map_recipient(ctx, find_address(decoded, "recipient")?);
        let amount_min = decimal_from_uint(find_uint(decoded, "amountMin")?);

        // Like WRAP_ETH, the opcode only constrains a minimum. The actual
        // unwrap can drain more if the router's WETH balance exceeds it.
        let amount = AmountConstraint {
            kind: AmountKind::Min,
            value: Some(amount_min),
        };

        let action = UnwrapAction {
            wrapped_asset: asset_with_amount(wrapped_weth(), amount.clone()),
            native_asset: asset_with_amount(native_eth(), amount),
            recipient,
        };

        Ok(vec![ActionEnvelope {
            category: Category::Misc,
            action: Action::Unwrap(action),
        }])
    }
}

#[must_use]
pub fn unwrap_weth_mapper_key() -> MapperMatchKey {
    MapperMatchKey {
        decoder_id: DecoderId::new(UR_UNWRAP_WETH_DECODER_ID),
    }
}

#[must_use]
pub fn unwrap_weth_mapper_arc() -> Arc<dyn Mapper> {
    Arc::new(UrUnwrapWethMapper::new())
}
