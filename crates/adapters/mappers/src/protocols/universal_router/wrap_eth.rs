//! UR opcode `WRAP_ETH` → `Action::Wrap`.
//!
//! Opcode signature: `(address recipient, uint256 amountMin)`.
//! Semantics: take native ETH from the router's balance (or the user's
//! `msg.value` staged on the router earlier in the same tx) and credit
//! `amountMin` WETH to `recipient` (which is usually the router itself —
//! the `0x...02` sentinel — when the WRAP is feeding a subsequent swap).

use std::sync::Arc;

use abi_resolver::ids::UR_WRAP_ETH_DECODER_ID;
use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::action::common::{AmountConstraint, AmountKind};
use policy_engine::action::envelope::{Action, ActionEnvelope, Category};
use policy_engine::action::misc::WrapAction;

use crate::mapper::{MapContext, Mapper, MapperError, MapperId, MapperMatchKey};

use super::common::{
    asset_with_amount, decimal_from_uint, find_address, find_uint, map_recipient, native_eth,
    wrapped_weth,
};

pub const UR_WRAP_ETH_MAPPER_ID: &str = "uniswap-ur/WRAP_ETH";

#[derive(Debug, Clone, Copy, Default)]
pub struct UrWrapEthMapper;

impl UrWrapEthMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for UrWrapEthMapper {
    fn id(&self) -> MapperId {
        MapperId::new(UR_WRAP_ETH_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id.as_str() == UR_WRAP_ETH_DECODER_ID
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let recipient = map_recipient(ctx, find_address(decoded, "recipient")?);
        let amount_min = decimal_from_uint(find_uint(decoded, "amountMin")?);

        // The opcode only guarantees a *minimum* — actual wrap amount can be
        // larger (e.g. when the router wraps everything it holds rather
        // than exactly amountMin). Surface as AmountKind::Min so policy
        // evaluators don't read it as an exact balance.
        let amount = AmountConstraint {
            kind: AmountKind::Min,
            value: Some(amount_min),
        };

        let action = WrapAction {
            native_asset: asset_with_amount(native_eth(), amount.clone()),
            wrapped_asset: asset_with_amount(wrapped_weth(), amount),
            recipient,
        };

        Ok(vec![ActionEnvelope {
            category: Category::Misc,
            action: Action::Wrap(action),
        }])
    }
}

#[must_use]
pub fn wrap_eth_mapper_key() -> MapperMatchKey {
    MapperMatchKey {
        decoder_id: DecoderId::new(UR_WRAP_ETH_DECODER_ID),
    }
}

#[must_use]
pub fn wrap_eth_mapper_arc() -> Arc<dyn Mapper> {
    Arc::new(UrWrapEthMapper::new())
}
