//! `Amm::CollectFees` lowering → `Amm::CollectFeesContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::amm::CollectFeesAction;

use super::super::common::cedar::addr;
use super::super::common::token::lower_token_key;
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_amm_venue, lower_token_amount_set};

/// Lower an `Amm::CollectFees` action into the `Amm::CollectFeesContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &CollectFeesAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_amm_venue(&action.venue));
    m.insert("nftKey".into(), lower_token_key(&action.nft_key));
    m.insert("recipient".into(), Value::String(addr(&action.recipient)));
    // `fees_owed` is LiveField<Vec<(TokenRef, U256)>>; inline to a set.
    m.insert(
        "feesOwed".into(),
        lower_token_amount_set(&action.live_inputs.fees_owed.value),
    );
    // `custom` is OMITTED — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Amm::Action::"CollectFees""#, Value::Object(m)))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::doc_markdown
)]
mod tests {
    use std::str::FromStr;

    use simulation_reducer::action::amm::{
        AmmAction, AmmVenue, CollectFeesAction, CollectFeesLiveInputs,
    };
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::{Address, ChainId, U256};
    use simulation_state::token::TokenKey;
    use simulation_state::LiveField;

    use super::super::test_support::{
        assert_conforms, now, onchain_meta, onchain_source, sample_token_ref, user,
    };

    /// A Uniswap V3 collect-fees on a position NFT, on-chain meta.
    fn sample_collect_fees() -> (ActionBody, simulation_reducer::action::ActionMeta) {
        let chain = ChainId::ethereum_mainnet();
        let venue = AmmVenue::UniswapV3 {
            chain: chain.clone(),
            pool: Address::from_str("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640").unwrap(),
            fee_tier_bp: 500,
        };
        let nft_key = TokenKey::Erc721 {
            chain: chain.clone(),
            contract: Address::from_str("0xc36442b4a4522e871399cd717abdd847ab11fe88").unwrap(),
            token_id: U256::from(98765u64),
        };

        let collect = AmmAction::CollectFees(CollectFeesAction {
            venue,
            nft_key,
            recipient: user(),
            live_inputs: CollectFeesLiveInputs {
                fees_owed: LiveField::new(
                    vec![
                        (sample_token_ref(&chain), U256::from(123_456u64)),
                        (sample_token_ref(&chain), U256::from(789_012u64)),
                    ],
                    onchain_source(),
                    now(),
                ),
            },
        });

        (ActionBody::Amm(collect), onchain_meta())
    }

    /// A collect-fees with an EMPTY `feesOwed` set — exercises the empty-Set
    /// branch of `lower_token_amount_set` (the populated branch is covered
    /// above). A V4 venue (poolId/poolManager/hooks) also widens venue cover
    /// here.
    fn sample_collect_fees_empty() -> (ActionBody, simulation_reducer::action::ActionMeta) {
        let chain = ChainId::ethereum_mainnet();
        let venue = AmmVenue::UniswapV4 {
            chain: chain.clone(),
            pool_id: "0xabc0000000000000000000000000000000000000000000000000000000000000".into(),
            pool_manager: Address::from_str("0x000000000004444c5dc75cb358380d2e3de08a90").unwrap(),
            hooks: Address::from_str("0x0000000000000000000000000000000000000000").unwrap(),
        };
        let nft_key = TokenKey::Erc721 {
            chain,
            contract: Address::from_str("0xc36442b4a4522e871399cd717abdd847ab11fe88").unwrap(),
            token_id: U256::from(42u64),
        };

        let collect = AmmAction::CollectFees(CollectFeesAction {
            venue,
            nft_key,
            recipient: user(),
            live_inputs: CollectFeesLiveInputs {
                fees_owed: LiveField::new(vec![], onchain_source(), now()),
            },
        });

        (ActionBody::Amm(collect), onchain_meta())
    }

    #[test]
    fn collect_fees_lowering_conforms_to_schema() {
        let (body, meta) = sample_collect_fees();
        assert_conforms("collect_fees", &body, &meta);
    }

    #[test]
    fn collect_fees_empty_fees_owed_conforms_to_schema() {
        let (body, meta) = sample_collect_fees_empty();
        assert_conforms("collect_fees", &body, &meta);
    }
}
