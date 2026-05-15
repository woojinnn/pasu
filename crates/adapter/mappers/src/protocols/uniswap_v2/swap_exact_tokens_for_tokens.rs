//! `swapExactTokensForTokens(amountIn, amountOutMin, path, to, deadline)`.

use abi_resolver::ids::SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID;
use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::action::dex::{SwapAction, SwapEnrichment, SwapMode};
use policy_engine::action::{ActionEnvelope, AmountKind};

use crate::mapper::{MapContext, Mapper, MapperError, MapperId};

use super::common::{
    address_arg, address_array_arg, amount_constraint, path_assets, swap_envelope, uint_arg,
    validity,
};

pub const SWAP_EXACT_TOKENS_FOR_TOKENS_MAPPER_ID: &str = "uniswap-v2/swapExactTokensForTokens";

#[derive(Debug, Clone, Copy, Default)]
pub struct SwapExactTokensForTokensMapper;

impl SwapExactTokensForTokensMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for SwapExactTokensForTokensMapper {
    fn id(&self) -> MapperId {
        MapperId::new(SWAP_EXACT_TOKENS_FOR_TOKENS_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id == DecoderId::new(SWAP_EXACT_TOKENS_FOR_TOKENS_DECODER_ID)
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let amount_in = uint_arg(decoded, "amountIn")?;
        let amount_out_min = uint_arg(decoded, "amountOutMin")?;
        let path = address_array_arg(decoded, "path")?;
        let recipient = address_arg(decoded, "to")?;
        let deadline = uint_arg(decoded, "deadline")?;
        let (token_in, token_out) = path_assets(ctx, &path)?;

        Ok(vec![swap_envelope(SwapAction {
            mode: SwapMode::ExactIn,
            token_in,
            token_out,
            amount_in: amount_constraint(AmountKind::Exact, amount_in)?,
            amount_out: amount_constraint(AmountKind::Min, amount_out_min)?,
            recipient,
            validity: Some(validity(deadline)?),
            fee_bps: Some(30),
            enrichment: SwapEnrichment::default(),
        })])
    }
}
