//! `swapETHForExactTokens(amountOut, path, to, deadline)`. ETH-funded exact-out swap.

use abi_resolver::ids::SWAP_ETH_FOR_EXACT_TOKENS_DECODER_ID;
use abi_resolver::{DecodedCall, DecoderId};
use policy_engine::action::dex::{SwapAction, SwapEnrichment, SwapMode};
use policy_engine::action::{ActionEnvelope, AmountKind};

use crate::mapper::{MapContext, Mapper, MapperError, MapperId};

use super::common::{
    address_arg, address_array_arg, amount_constraint, decimal_amount_constraint,
    native_eth_asset_ref, path_last_asset, swap_envelope, uint_arg, validity,
};

pub const SWAP_ETH_FOR_EXACT_TOKENS_MAPPER_ID: &str = "uniswap-v2/swapETHForExactTokens";

#[derive(Debug, Clone, Copy, Default)]
pub struct SwapETHForExactTokensMapper;

impl SwapETHForExactTokensMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for SwapETHForExactTokensMapper {
    fn id(&self) -> MapperId {
        MapperId::new(SWAP_ETH_FOR_EXACT_TOKENS_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id == DecoderId::new(SWAP_ETH_FOR_EXACT_TOKENS_DECODER_ID)
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let amount_out = uint_arg(decoded, "amountOut")?;
        let path = address_array_arg(decoded, "path")?;
        let recipient = address_arg(decoded, "to")?;
        let deadline = uint_arg(decoded, "deadline")?;

        Ok(vec![swap_envelope(SwapAction {
            mode: SwapMode::ExactOut,
            token_in: native_eth_asset_ref(ctx),
            token_out: path_last_asset(ctx, &path)?,
            amount_in: decimal_amount_constraint(AmountKind::Max, ctx.value_wei),
            amount_out: amount_constraint(AmountKind::Exact, amount_out)?,
            recipient,
            validity: Some(validity(deadline)?),
            fee_bps: Some(30),
            enrichment: SwapEnrichment::default(),
        })])
    }
}
