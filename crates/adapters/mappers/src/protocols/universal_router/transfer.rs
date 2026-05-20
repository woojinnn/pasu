//! UR opcode `TRANSFER` → `Action::Transfer` (router → recipient, exact value).
//!
//! Signature: `(address token, address recipient, uint256 value)`.
//! Semantics: pay an exact `value` of `token` from the router to `recipient`.
//! Differs from SWEEP (which drains whatever the router holds, asserting a
//! minimum) by committing to a precise amount.

use std::sync::Arc;

use abi_resolver::ids::UR_TRANSFER_DECODER_ID;
use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::action::common::{AmountConstraint, AmountKind};
use policy_engine::action::envelope::{Action, ActionEnvelope, Category};
use policy_engine::action::misc::TransferAction;

use crate::mapper::{MapContext, Mapper, MapperError, MapperId, MapperMatchKey};

use super::common::{
    asset_with_amount, decimal_from_uint, find_address, find_uint, map_recipient, token_asset_ref,
};

pub const UR_TRANSFER_MAPPER_ID: &str = "uniswap-ur/TRANSFER";

#[derive(Debug, Clone, Copy, Default)]
pub struct UrTransferMapper;

impl UrTransferMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for UrTransferMapper {
    fn id(&self) -> MapperId {
        MapperId::new(UR_TRANSFER_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id.as_str() == UR_TRANSFER_DECODER_ID
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let token = find_address(decoded, "token")?;
        let recipient = map_recipient(ctx, find_address(decoded, "recipient")?);
        let value = decimal_from_uint(find_uint(decoded, "value")?);

        let asset = token_asset_ref(ctx, &token);
        let action = TransferAction {
            token: asset_with_amount(
                asset,
                AmountConstraint {
                    kind: AmountKind::Exact,
                    value: Some(value),
                },
            ),
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
pub fn transfer_mapper_key() -> MapperMatchKey {
    MapperMatchKey {
        decoder_id: DecoderId::new(UR_TRANSFER_DECODER_ID),
    }
}

#[must_use]
pub fn transfer_mapper_arc() -> Arc<dyn Mapper> {
    Arc::new(UrTransferMapper::new())
}
