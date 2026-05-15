//! Uniswap V3 `exactInputSingle` / `exactInput` mapper.
//!
//! Both `exactInputSingle` and `exactInput` share the same `decoder_id`
//! (`UNISWAP_V3_DECODER_ID`) in the upstream Sourcify-fallback bridge, so the
//! registered `Mapper` must dispatch on `function_signature` internally. The
//! umbrella `UniswapV3Mapper` lives here (alongside the single-hop case) and
//! delegates to the shared per-function `map_*` helpers in `common.rs`.

use abi_resolver::ids::{EXACT_OUTPUT_DECODER_ID, EXACT_OUTPUT_SINGLE_DECODER_ID, UNISWAP_V3_DECODER_ID};
use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::action::ActionEnvelope;

use crate::mapper::{MapContext, Mapper, MapperError, MapperId};

use super::common::{
    argument_mismatch, map_exact_input, map_exact_input_single, map_exact_output,
    map_exact_output_single, EXACT_INPUT_SIGNATURE, EXACT_INPUT_SINGLE_SIGNATURE,
    EXACT_OUTPUT_SIGNATURE, EXACT_OUTPUT_SINGLE_SIGNATURE,
};

pub const UNISWAP_V3_MAPPER_ID: &str = "uniswap_v3";

#[derive(Debug, Clone, Copy, Default)]
pub struct UniswapV3Mapper;

impl UniswapV3Mapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for UniswapV3Mapper {
    fn id(&self) -> MapperId {
        MapperId::new(UNISWAP_V3_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        match decoded.function_signature.as_str() {
            EXACT_INPUT_SINGLE_SIGNATURE | EXACT_INPUT_SIGNATURE => {
                decoded.decoder_id == DecoderId::new(UNISWAP_V3_DECODER_ID)
            }
            EXACT_OUTPUT_SINGLE_SIGNATURE => {
                decoded.decoder_id == DecoderId::new(EXACT_OUTPUT_SINGLE_DECODER_ID)
            }
            EXACT_OUTPUT_SIGNATURE => decoded.decoder_id == DecoderId::new(EXACT_OUTPUT_DECODER_ID),
            _ => false,
        }
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        match decoded.function_signature.as_str() {
            EXACT_INPUT_SINGLE_SIGNATURE => Ok(vec![map_exact_input_single(ctx, decoded)?]),
            EXACT_INPUT_SIGNATURE => Ok(vec![map_exact_input(ctx, decoded)?]),
            EXACT_OUTPUT_SINGLE_SIGNATURE => Ok(vec![map_exact_output_single(ctx, decoded)?]),
            EXACT_OUTPUT_SIGNATURE => Ok(vec![map_exact_output(ctx, decoded)?]),
            other => Err(argument_mismatch(
                "function_signature",
                format!("unsupported Uniswap V3 function {other}"),
            )),
        }
    }
}
