//! UR opcode `V3_SWAP_EXACT_OUT` → `Action::Swap` (V3 exact-out).
//!
//! Signature: `(address recipient, uint256 amountOut, uint256 amountInMax,
//! bytes path, bool payerIsUser)`.
//!
//! V3 exact-out paths are reversed (token_out first, token_in last) — the
//! parser treats both directions symmetrically and we relabel the endpoints
//! here for the exact-out semantics.

use std::sync::Arc;

use abi_resolver::ids::UR_V3_SWAP_EXACT_OUT_DECODER_ID;
use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::action::common::AmountKind;
use policy_engine::action::dex::{SwapAction, SwapMode};
use policy_engine::action::envelope::{Action, ActionEnvelope, Category};

use crate::mapper::{MapContext, Mapper, MapperError, MapperId, MapperMatchKey};

use super::common::swap_amount_constraint;
use super::common::{
    asset_with_amount, decimal_from_uint, find_address, find_bytes, find_uint, map_recipient,
    parse_v3_path, token_asset_ref,
};

pub const UR_V3_SWAP_EXACT_OUT_MAPPER_ID: &str = "uniswap-ur/V3_SWAP_EXACT_OUT";

#[derive(Debug, Clone, Copy, Default)]
pub struct UrV3SwapExactOutMapper;

impl UrV3SwapExactOutMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for UrV3SwapExactOutMapper {
    fn id(&self) -> MapperId {
        MapperId::new(UR_V3_SWAP_EXACT_OUT_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id.as_str() == UR_V3_SWAP_EXACT_OUT_DECODER_ID
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let recipient = map_recipient(ctx, find_address(decoded, "recipient")?);
        let amount_out = decimal_from_uint(find_uint(decoded, "amountOut")?);
        let amount_in_max = decimal_from_uint(find_uint(decoded, "amountInMax")?);
        let path_bytes = find_bytes(decoded, "path")?;
        // V3 exact-out paths are encoded reversed (output first, input last),
        // so `path.token_in` from the parser is the output and vice versa.
        let path = parse_v3_path(&path_bytes)?;

        let action = SwapAction {
            swap_mode: SwapMode::ExactOut,
            input_token: asset_with_amount(
                token_asset_ref(ctx, &path.token_out),
                swap_amount_constraint(AmountKind::Max, amount_in_max),
            ),
            output_token: asset_with_amount(
                token_asset_ref(ctx, &path.token_in),
                swap_amount_constraint(AmountKind::Exact, amount_out),
            ),
            recipient,
            validity: None,
            fee_bps: path.fee_bps,
        };

        Ok(vec![ActionEnvelope {
            category: Category::Dex,
            action: Action::Swap(action),
        }])
    }
}

#[must_use]
pub fn v3_swap_exact_out_mapper_key() -> MapperMatchKey {
    MapperMatchKey {
        decoder_id: DecoderId::new(UR_V3_SWAP_EXACT_OUT_DECODER_ID),
    }
}

#[must_use]
pub fn v3_swap_exact_out_mapper_arc() -> Arc<dyn Mapper> {
    Arc::new(UrV3SwapExactOutMapper::new())
}
