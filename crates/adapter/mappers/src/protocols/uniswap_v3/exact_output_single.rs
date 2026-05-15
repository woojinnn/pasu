//! Uniswap V3 `exactOutputSingle` mapper.

use abi_resolver::ids::EXACT_OUTPUT_SINGLE_DECODER_ID;
use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::action::ActionEnvelope;

use crate::mapper::{MapContext, Mapper, MapperError, MapperId};

use super::common::{map_exact_output_single, EXACT_OUTPUT_SINGLE_SIGNATURE};

pub const EXACT_OUTPUT_SINGLE_MAPPER_ID: &str = "uniswap-v3/exactOutputSingle";

#[derive(Debug, Clone, Copy, Default)]
pub struct UniswapV3ExactOutputSingleMapper;

impl UniswapV3ExactOutputSingleMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for UniswapV3ExactOutputSingleMapper {
    fn id(&self) -> MapperId {
        MapperId::new(EXACT_OUTPUT_SINGLE_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id == DecoderId::new(EXACT_OUTPUT_SINGLE_DECODER_ID)
            && decoded.function_signature == EXACT_OUTPUT_SINGLE_SIGNATURE
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        Ok(vec![map_exact_output_single(ctx, decoded)?])
    }
}
