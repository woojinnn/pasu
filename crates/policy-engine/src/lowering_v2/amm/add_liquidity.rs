//! `Amm::AddLiquidity` lowering → `Amm::AddLiquidityContext`.

use serde_json::{Map, Value};

use simulation_reducer::action::amm::{AddLiquidityAction, AddLiquidityParams};
use simulation_state::primitives::U256;
use simulation_state::token::{RangeSpec, TokenRef};

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::{lower_token_key, lower_token_ref};
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_amm_venue, lower_amount_pair, lower_token_amount_set};

/// Lower an `Amm::AddLiquidity` action into the `Amm::AddLiquidityContext` shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &AddLiquidityAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_amm_venue(&action.venue));
    m.insert("params".into(), lower_add_liquidity_params(&action.params));
    // `current_price` is a LiveField<Price>; Price is a decimal-string. Inline
    // its inner value as a String.
    m.insert(
        "currentPrice".into(),
        Value::String(action.live_inputs.current_price.value.to_string()),
    );
    // `custom` is OMITTED — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Amm::Action::"AddLiquidity""#, Value::Object(m)))
}

/// Lower [`AddLiquidityParams`] → discriminated `{ kind, … }`
/// (`Amm::AddLiquidityParams`). Only the fields a variant carries are emitted.
fn lower_add_liquidity_params(params: &AddLiquidityParams) -> Value {
    let mut m = Map::new();
    match params {
        AddLiquidityParams::Pooled {
            tokens,
            min_lp_out,
            recipient,
        } => {
            m.insert("kind".into(), Value::String("pooled".into()));
            m.insert("tokens".into(), lower_token_amount_set(tokens));
            m.insert("minLpOut".into(), Value::String(u256_hex(*min_lp_out)));
            m.insert("recipient".into(), Value::String(addr(recipient)));
        }
        AddLiquidityParams::ConcentratedMint {
            pool_pair,
            amount_desired,
            amount_min,
            range,
            recipient,
        } => {
            m.insert("kind".into(), Value::String("concentrated_mint".into()));
            m.insert("poolPair".into(), lower_pool_pair(pool_pair));
            m.insert("amountDesired".into(), lower_amount_pair(amount_desired));
            m.insert("amountMin".into(), lower_amount_pair(amount_min));
            m.insert("range".into(), lower_range_spec(range));
            m.insert("recipient".into(), Value::String(addr(recipient)));
        }
        AddLiquidityParams::ConcentratedIncrease {
            nft_key,
            amount_desired,
            amount_min,
        } => {
            m.insert("kind".into(), Value::String("concentrated_increase".into()));
            m.insert("nftKey".into(), lower_token_key(nft_key));
            m.insert("amountDesired".into(), lower_amount_pair(amount_desired));
            m.insert("amountMin".into(), lower_amount_pair(amount_min));
        }
    }
    Value::Object(m)
}

/// Lower a `(TokenRef, TokenRef)` pool pair → `{ tokenA, tokenB }`.
fn lower_pool_pair(pair: &(TokenRef, TokenRef)) -> Value {
    let mut m = Map::new();
    m.insert("tokenA".into(), lower_token_ref(&pair.0));
    m.insert("tokenB".into(), lower_token_ref(&pair.1));
    Value::Object(m)
}

/// Lower a [`RangeSpec`] → discriminated `{ kind, … }` (`Amm::RangeSpec`). Only
/// the schema-declared fields are emitted; reducer-internal payloads
/// (`distribution`, `raw`) are not exposed at the policy layer.
fn lower_range_spec(range: &RangeSpec) -> Value {
    let mut m = Map::new();
    match range {
        RangeSpec::Tick {
            lower,
            upper,
            liquidity,
        } => {
            m.insert("kind".into(), Value::String("tick".into()));
            m.insert("tickLower".into(), Value::from(i64::from(*lower)));
            m.insert("tickUpper".into(), Value::from(i64::from(*upper)));
            m.insert("liquidity".into(), Value::String(u256_hex(U256::from(*liquidity))));
        }
        RangeSpec::Bin { active_id, .. } => {
            m.insert("kind".into(), Value::String("bin".into()));
            m.insert("activeId".into(), Value::from(i64::from(*active_id)));
        }
        RangeSpec::Custom { protocol, .. } => {
            m.insert("kind".into(), Value::String("custom".into()));
            m.insert("protocol".into(), Value::String(protocol.clone()));
        }
    }
    Value::Object(m)
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
        AddLiquidityAction, AddLiquidityLiveInputs, AddLiquidityParams, AmmAction, AmmVenue,
        PoolState,
    };
    use simulation_reducer::action::ActionBody;
    use simulation_state::primitives::{Address, ChainId, Decimal, U128, U256};
    use simulation_state::token::RangeSpec;
    use simulation_state::LiveField;

    use super::super::test_support::{
        assert_conforms, now, onchain_meta, onchain_source, sample_token_ref, user,
    };

    /// A Uniswap V3 concentrated-mint add-liquidity (Tick range), on-chain meta.
    fn sample_concentrated_mint() -> (ActionBody, simulation_reducer::action::ActionMeta) {
        let chain = ChainId::ethereum_mainnet();
        let pool = Address::from_str("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640").unwrap();
        let venue = AmmVenue::UniswapV3 {
            chain: chain.clone(),
            pool,
            fee_tier_bp: 500,
        };
        let token_a = sample_token_ref(&chain);
        let token_b = sample_token_ref(&chain);

        let pool_state = PoolState::Concentrated {
            sqrt_price_x96: U256::from(1u64),
            tick: 0,
            liquidity: U128::from(0u64),
            ticks: vec![],
        };

        let add = AmmAction::AddLiquidity(AddLiquidityAction {
            venue,
            params: AddLiquidityParams::ConcentratedMint {
                pool_pair: (token_a, token_b),
                amount_desired: (U256::from(1_000_000u64), U256::from(500_000u64)),
                amount_min: (U256::from(990_000u64), U256::from(495_000u64)),
                range: RangeSpec::Tick {
                    lower: -887_220,
                    upper: 887_220,
                    liquidity: U128::from(123_456_789u64),
                },
                recipient: user(),
            },
            live_inputs: AddLiquidityLiveInputs {
                pool_state: LiveField::new(pool_state, onchain_source(), now()),
                current_price: LiveField::new(
                    Decimal::new("1234.5678"),
                    onchain_source(),
                    now(),
                ),
            },
        });

        (ActionBody::Amm(add), onchain_meta())
    }

    /// A Uniswap V2-style pooled deposit, on-chain meta — exercises the
    /// `tokens` set + `minLpOut` branch.
    fn sample_pooled() -> (ActionBody, simulation_reducer::action::ActionMeta) {
        let chain = ChainId::ethereum_mainnet();
        let venue = AmmVenue::UniswapV2 {
            chain: chain.clone(),
            pool: Address::from_str("0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc").unwrap(),
            factory: Address::from_str("0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f").unwrap(),
        };
        let pool_state = PoolState::XyConstant {
            reserve_in: U256::from(1_000_000u64),
            reserve_out: U256::from(2_000_000u64),
            fee_bp: 30,
        };

        let add = AmmAction::AddLiquidity(AddLiquidityAction {
            venue,
            params: AddLiquidityParams::Pooled {
                tokens: vec![
                    (sample_token_ref(&chain), U256::from(1_000_000u64)),
                    (sample_token_ref(&chain), U256::from(2_000_000u64)),
                ],
                min_lp_out: U256::from(900_000u64),
                recipient: user(),
            },
            live_inputs: AddLiquidityLiveInputs {
                pool_state: LiveField::new(pool_state, onchain_source(), now()),
                current_price: LiveField::new(Decimal::new("2.0"), onchain_source(), now()),
            },
        });

        (ActionBody::Amm(add), onchain_meta())
    }

    #[test]
    fn add_liquidity_concentrated_mint_conforms_to_schema() {
        let (body, meta) = sample_concentrated_mint();
        assert_conforms("add_liquidity", &body, &meta);
    }

    #[test]
    fn add_liquidity_pooled_conforms_to_schema() {
        let (body, meta) = sample_pooled();
        assert_conforms("add_liquidity", &body, &meta);
    }
}
