//! SwapRouter02 `exactInputSingle` mapper.

use abi_resolver::ids::SR02_EXACT_INPUT_SINGLE_DECODER_ID;
use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::action::dex::{SwapAction, SwapEnrichment, SwapMode};
use policy_engine::action::{ActionEnvelope, AmountKind};

use crate::mapper::{MapContext, Mapper, MapperError, MapperId};

use super::common::{
    address_arg, amount_constraint, asset_ref, fee_bps, swap_envelope, uint_arg,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct Sr02ExactInputSingleMapper;

impl Sr02ExactInputSingleMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for Sr02ExactInputSingleMapper {
    fn id(&self) -> MapperId {
        MapperId::new(SR02_EXACT_INPUT_SINGLE_DECODER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id == DecoderId::new(SR02_EXACT_INPUT_SINGLE_DECODER_ID)
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let token_in = address_arg(decoded, "tokenIn")?;
        let token_out = address_arg(decoded, "tokenOut")?;
        let fee = uint_arg(decoded, "fee")?;
        let recipient = address_arg(decoded, "recipient")?;
        let amount_in = uint_arg(decoded, "amountIn")?;
        let amount_out_minimum = uint_arg(decoded, "amountOutMinimum")?;

        Ok(vec![swap_envelope(SwapAction {
            mode: SwapMode::ExactIn,
            token_in: asset_ref(ctx, &token_in),
            token_out: asset_ref(ctx, &token_out),
            amount_in: amount_constraint(AmountKind::Exact, amount_in)?,
            amount_out: amount_constraint(AmountKind::Min, amount_out_minimum)?,
            recipient,
            validity: None,
            fee_bps: Some(fee_bps(fee)?),
            enrichment: SwapEnrichment::default(),
        })])
    }
}
