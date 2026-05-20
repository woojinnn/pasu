//! UR opcode `V2_SWAP_EXACT_IN` → `Action::Swap` (V2 exact-in).
//!
//! Signature: `(address recipient, uint256 amountIn, uint256 amountOutMin,
//! address[] path, bool payerIsUser)`.
//!
//! V2 fees are universally 30 bps (0.3%) — no fee tiers — so the mapper
//! reports `fee_bps = Some(30)` unconditionally.

use std::sync::Arc;

use abi_resolver::ids::UR_V2_SWAP_EXACT_IN_DECODER_ID;
use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::action::common::AmountKind;
use policy_engine::action::dex::{SwapAction, SwapMode};
use policy_engine::action::envelope::{Action, ActionEnvelope, Category};

use crate::mapper::{MapContext, Mapper, MapperError, MapperId, MapperMatchKey};

use super::common::swap_amount_constraint;
use super::common::{
    asset_with_amount, decimal_from_uint, find_address, find_address_array, find_uint,
    map_recipient, path_endpoints, token_asset_ref,
};

pub const UR_V2_SWAP_EXACT_IN_MAPPER_ID: &str = "uniswap-ur/V2_SWAP_EXACT_IN";

#[derive(Debug, Clone, Copy, Default)]
pub struct UrV2SwapExactInMapper;

impl UrV2SwapExactInMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for UrV2SwapExactInMapper {
    fn id(&self) -> MapperId {
        MapperId::new(UR_V2_SWAP_EXACT_IN_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id.as_str() == UR_V2_SWAP_EXACT_IN_DECODER_ID
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let recipient = map_recipient(ctx, find_address(decoded, "recipient")?);
        let amount_in = decimal_from_uint(find_uint(decoded, "amountIn")?);
        let amount_out_min = decimal_from_uint(find_uint(decoded, "amountOutMin")?);
        let path = find_address_array(decoded, "path")?;
        let (token_in, token_out) = path_endpoints(&path)?;

        let action = SwapAction {
            swap_mode: SwapMode::ExactIn,
            input_token: asset_with_amount(
                token_asset_ref(ctx, token_in),
                swap_amount_constraint(AmountKind::Exact, amount_in),
            ),
            output_token: asset_with_amount(
                token_asset_ref(ctx, token_out),
                swap_amount_constraint(AmountKind::Min, amount_out_min),
            ),
            recipient,
            validity: None,
            fee_bps: Some(30),
        };

        Ok(vec![ActionEnvelope {
            category: Category::Dex,
            action: Action::Swap(action),
        }])
    }
}

#[must_use]
pub fn v2_swap_exact_in_mapper_key() -> MapperMatchKey {
    MapperMatchKey {
        decoder_id: DecoderId::new(UR_V2_SWAP_EXACT_IN_DECODER_ID),
    }
}

#[must_use]
pub fn v2_swap_exact_in_mapper_arc() -> Arc<dyn Mapper> {
    Arc::new(UrV2SwapExactInMapper::new())
}
