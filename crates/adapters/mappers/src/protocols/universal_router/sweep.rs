//! UR opcode `SWEEP` → `Action::Transfer` (router → recipient, exact-min).
//!
//! Signature: `(address token, address recipient, uint256 amountMin)`.
//! Semantics: drain the router's balance of `token` to `recipient`, asserting
//! the amount is at least `amountMin`. Used as the final step of a swap
//! flow that landed output on the router (`ACTION_ADDRESS_THIS`). The
//! compactor uses this transfer to fold `[WRAP, SWAP, SWEEP]` back into a
//! single `Swap(ETH → output)` envelope.

use std::sync::Arc;

use abi_resolver::ids::UR_SWEEP_DECODER_ID;
use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::action::common::{AmountConstraint, AmountKind};
use policy_engine::action::envelope::{Action, ActionEnvelope, Category};
use policy_engine::action::misc::TransferAction;

use crate::mapper::{MapContext, Mapper, MapperError, MapperId, MapperMatchKey};

use super::common::{
    asset_with_amount, decimal_from_uint, find_address, find_uint, map_recipient, token_asset_ref,
};

pub const UR_SWEEP_MAPPER_ID: &str = "uniswap-ur/SWEEP";

#[derive(Debug, Clone, Copy, Default)]
pub struct UrSweepMapper;

impl UrSweepMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for UrSweepMapper {
    fn id(&self) -> MapperId {
        MapperId::new(UR_SWEEP_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id.as_str() == UR_SWEEP_DECODER_ID
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let token = find_address(decoded, "token")?;
        let recipient = map_recipient(ctx, find_address(decoded, "recipient")?);
        let amount_min = decimal_from_uint(find_uint(decoded, "amountMin")?);

        let asset = token_asset_ref(&token);
        let action = TransferAction {
            token: asset_with_amount(
                asset,
                AmountConstraint {
                    kind: AmountKind::Min,
                    value: Some(amount_min),
                },
            ),
            // SWEEP always drains the router contract (ctx.to).
            from: ctx.to.clone(),
            recipient,
        };

        Ok(vec![ActionEnvelope {
            category: Category::Misc,
            action: Action::Transfer(action),
        }])
    }
}

#[must_use]
pub fn sweep_mapper_key() -> MapperMatchKey {
    MapperMatchKey {
        decoder_id: DecoderId::new(UR_SWEEP_DECODER_ID),
    }
}

#[must_use]
pub fn sweep_mapper_arc() -> Arc<dyn Mapper> {
    Arc::new(UrSweepMapper::new())
}
