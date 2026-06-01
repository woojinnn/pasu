//! `Amm::RemoveLiquidity` lowering → `Amm::RemoveLiquidityContext`.

use serde_json::{Map, Value};

use policy_state::primitives::U256;
use policy_transition::action::amm::{RemoveLiquidityAction, RemoveLiquidityParams};

use super::super::common::cedar::{addr, u256_hex};
use super::super::common::token::{lower_token_key, lower_token_ref};
use super::super::dispatch::{LowerCtx, LowerError, LoweredAction};
use super::{lower_amm_venue, lower_amount_pair, lower_token_amount_set};

/// Lower an `Amm::RemoveLiquidity` action into the `Amm::RemoveLiquidityContext`
/// shape.
///
/// # Errors
///
/// Infallible today (returns `Ok`); the `Result` matches the per-action
/// `lower` contract so callers stay uniform across the fan-out.
#[allow(clippy::unnecessary_wraps)] // infallible; Result is the shared per-action contract
pub(crate) fn lower(
    action: &RemoveLiquidityAction,
    ctx: &LowerCtx<'_>,
) -> Result<LoweredAction, LowerError> {
    let mut m = Map::new();
    m.insert("meta".into(), ctx.meta());
    m.insert("venue".into(), lower_amm_venue(&action.venue));
    m.insert(
        "params".into(),
        lower_remove_liquidity_params(&action.params),
    );
    // `fees_owed` is LiveField<Vec<(TokenRef, U256)>>; inline to a set.
    m.insert(
        "feesOwed".into(),
        lower_token_amount_set(&action.live_inputs.fees_owed.value),
    );
    // `custom` is OMITTED — it is filled later by enrichment.

    Ok(ctx.lowered(r#"Amm::Action::"RemoveLiquidity""#, Value::Object(m)))
}

/// Lower [`RemoveLiquidityParams`] → discriminated `{ kind, … }`
/// (`Amm::RemoveLiquidityParams`). Only the fields a variant carries are emitted.
fn lower_remove_liquidity_params(params: &RemoveLiquidityParams) -> Value {
    let mut m = Map::new();
    match params {
        RemoveLiquidityParams::PooledBurn {
            lp_token,
            lp_amount,
            min_out,
            recipient,
        } => {
            m.insert("kind".into(), Value::String("pooled_burn".into()));
            m.insert("lpToken".into(), lower_token_ref(lp_token));
            m.insert("lpAmount".into(), Value::String(u256_hex(*lp_amount)));
            m.insert("minOut".into(), lower_token_amount_set(min_out));
            m.insert("recipient".into(), Value::String(addr(recipient)));
        }
        RemoveLiquidityParams::PooledBurnOneCoin {
            lp_token,
            lp_amount,
            token_out,
            min_out,
            recipient,
        } => {
            m.insert("kind".into(), Value::String("pooled_burn_one_coin".into()));
            m.insert("lpToken".into(), lower_token_ref(lp_token));
            m.insert("lpAmount".into(), Value::String(u256_hex(*lp_amount)));
            m.insert("tokenOut".into(), lower_token_ref(token_out));
            // Scalar single-coin floor — distinct Cedar key from the `minOut`
            // set used by PooledBurn (which is `Set<{token,amount}>`).
            m.insert("minOutOne".into(), Value::String(u256_hex(*min_out)));
            m.insert("recipient".into(), Value::String(addr(recipient)));
        }
        RemoveLiquidityParams::PooledBurnImbalance {
            lp_token,
            max_lp_burn,
            amounts_out,
            recipient,
        } => {
            m.insert("kind".into(), Value::String("pooled_burn_imbalance".into()));
            m.insert("lpToken".into(), lower_token_ref(lp_token));
            m.insert("maxLpBurn".into(), Value::String(u256_hex(*max_lp_burn)));
            m.insert("amountsOut".into(), lower_token_amount_set(amounts_out));
            m.insert("recipient".into(), Value::String(addr(recipient)));
        }
        RemoveLiquidityParams::ConcentratedDecrease {
            nft_key,
            liquidity_burn,
            amount_min,
        } => {
            m.insert("kind".into(), Value::String("concentrated_decrease".into()));
            m.insert("nftKey".into(), lower_token_key(nft_key));
            m.insert(
                "liquidityBurn".into(),
                Value::String(u256_hex(U256::from(*liquidity_burn))),
            );
            m.insert("amountMin".into(), lower_amount_pair(amount_min));
        }
        RemoveLiquidityParams::ConcentratedBurn { nft_key } => {
            m.insert("kind".into(), Value::String("concentrated_burn".into()));
            m.insert("nftKey".into(), lower_token_key(nft_key));
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

    use policy_state::primitives::{Address, ChainId, U128, U256};
    use policy_state::token::TokenKey;
    use policy_state::LiveField;
    use policy_transition::action::amm::{
        AmmAction, AmmVenue, PoolState, RemoveLiquidityAction, RemoveLiquidityLiveInputs,
        RemoveLiquidityParams,
    };
    use policy_transition::action::ActionBody;

    use super::super::test_support::{
        assert_conforms, now, onchain_meta, onchain_source, sample_token_ref, user,
    };

    /// A Uniswap V2-style pooled burn, on-chain meta — exercises lpToken,
    /// lpAmount, minOut set, recipient.
    fn sample_pooled_burn() -> (ActionBody, policy_transition::action::ActionMeta) {
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

        let remove = AmmAction::RemoveLiquidity(RemoveLiquidityAction {
            venue,
            params: RemoveLiquidityParams::PooledBurn {
                lp_token: sample_token_ref(&chain),
                lp_amount: U256::from(500_000u64),
                min_out: vec![
                    (sample_token_ref(&chain), U256::from(450_000u64)),
                    (sample_token_ref(&chain), U256::from(900_000u64)),
                ],
                recipient: user(),
            },
            live_inputs: RemoveLiquidityLiveInputs {
                pool_state: LiveField::new(pool_state, onchain_source(), now()),
                fees_owed: LiveField::new(
                    vec![(sample_token_ref(&chain), U256::from(123u64))],
                    onchain_source(),
                    now(),
                ),
            },
        });

        (ActionBody::Amm(remove), onchain_meta())
    }

    /// A Uniswap V3 concentrated-decrease, on-chain meta — exercises nftKey,
    /// liquidityBurn (U128), amountMin.
    fn sample_concentrated_decrease() -> (ActionBody, policy_transition::action::ActionMeta) {
        let chain = ChainId::ethereum_mainnet();
        let venue = AmmVenue::UniswapV3 {
            chain: chain.clone(),
            pool: Address::from_str("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640").unwrap(),
            fee_tier_bp: 500,
        };
        let pool_state = PoolState::Concentrated {
            sqrt_price_x96: U256::from(1u64),
            tick: 0,
            liquidity: U128::from(0u64),
            ticks: vec![],
        };
        let nft_key = TokenKey::Erc721 {
            chain,
            contract: Address::from_str("0xc36442b4a4522e871399cd717abdd847ab11fe88").unwrap(),
            token_id: U256::from(98765u64),
        };

        let remove = AmmAction::RemoveLiquidity(RemoveLiquidityAction {
            venue,
            params: RemoveLiquidityParams::ConcentratedDecrease {
                nft_key,
                liquidity_burn: U128::from(123_456_789u64),
                amount_min: (U256::from(990_000u64), U256::from(495_000u64)),
            },
            live_inputs: RemoveLiquidityLiveInputs {
                pool_state: LiveField::new(pool_state, onchain_source(), now()),
                fees_owed: LiveField::new(vec![], onchain_source(), now()),
            },
        });

        (ActionBody::Amm(remove), onchain_meta())
    }

    /// A Uniswap V3 ConcentratedBurn (burn an empty position NFT) — exercises
    /// the third `RemoveLiquidityParams` arm (nftKey only; no lpToken /
    /// liquidityBurn / amountMin / recipient).
    fn sample_concentrated_burn() -> (ActionBody, policy_transition::action::ActionMeta) {
        let chain = ChainId::ethereum_mainnet();
        let venue = AmmVenue::UniswapV3 {
            chain: chain.clone(),
            pool: Address::from_str("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640").unwrap(),
            fee_tier_bp: 500,
        };
        let pool_state = PoolState::Concentrated {
            sqrt_price_x96: U256::from(1u64),
            tick: 0,
            liquidity: U128::from(0u64),
            ticks: vec![],
        };
        let nft_key = TokenKey::Erc721 {
            chain,
            contract: Address::from_str("0xc36442b4a4522e871399cd717abdd847ab11fe88").unwrap(),
            token_id: U256::from(98765u64),
        };

        let remove = AmmAction::RemoveLiquidity(RemoveLiquidityAction {
            venue,
            params: RemoveLiquidityParams::ConcentratedBurn { nft_key },
            live_inputs: RemoveLiquidityLiveInputs {
                pool_state: LiveField::new(pool_state, onchain_source(), now()),
                fees_owed: LiveField::new(vec![], onchain_source(), now()),
            },
        });

        (ActionBody::Amm(remove), onchain_meta())
    }

    /// Curve StableSwap-NG remove_liquidity_one_coin — exercises tokenOut +
    /// the scalar `minOutOne` floor (PooledBurnOneCoin), on-chain meta.
    fn sample_pooled_burn_one_coin() -> (ActionBody, policy_transition::action::ActionMeta) {
        let chain = ChainId::ethereum_mainnet();
        let venue = AmmVenue::CurveV1 {
            chain: chain.clone(),
            pool: Address::from_str("0x4dece678ceceb27446b35c672dc7d61f30bad69e").unwrap(),
            n_coins: 2,
            is_meta: false,
        };
        let pool_state = PoolState::StableV1 {
            balances: vec![U256::from(1_000_000u64), U256::from(2_000_000u64)],
            a: 100,
            fee_bp: 4,
        };
        let remove = AmmAction::RemoveLiquidity(RemoveLiquidityAction {
            venue,
            params: RemoveLiquidityParams::PooledBurnOneCoin {
                lp_token: sample_token_ref(&chain),
                lp_amount: U256::from(500_000u64),
                token_out: sample_token_ref(&chain),
                min_out: U256::from(495_000u64),
                recipient: user(),
            },
            live_inputs: RemoveLiquidityLiveInputs {
                pool_state: LiveField::new(pool_state, onchain_source(), now()),
                fees_owed: LiveField::new(vec![], onchain_source(), now()),
            },
        });
        (ActionBody::Amm(remove), onchain_meta())
    }

    /// Curve StableSwap-NG remove_liquidity_imbalance — exercises maxLpBurn +
    /// the `amountsOut` set (PooledBurnImbalance), on-chain meta.
    fn sample_pooled_burn_imbalance() -> (ActionBody, policy_transition::action::ActionMeta) {
        let chain = ChainId::ethereum_mainnet();
        let venue = AmmVenue::CurveV1 {
            chain: chain.clone(),
            pool: Address::from_str("0x4dece678ceceb27446b35c672dc7d61f30bad69e").unwrap(),
            n_coins: 2,
            is_meta: false,
        };
        let pool_state = PoolState::StableV1 {
            balances: vec![U256::from(1_000_000u64), U256::from(2_000_000u64)],
            a: 100,
            fee_bp: 4,
        };
        let remove = AmmAction::RemoveLiquidity(RemoveLiquidityAction {
            venue,
            params: RemoveLiquidityParams::PooledBurnImbalance {
                lp_token: sample_token_ref(&chain),
                max_lp_burn: U256::from(510_000u64),
                amounts_out: vec![
                    (sample_token_ref(&chain), U256::from(200_000u64)),
                    (sample_token_ref(&chain), U256::from(300_000u64)),
                ],
                recipient: user(),
            },
            live_inputs: RemoveLiquidityLiveInputs {
                pool_state: LiveField::new(pool_state, onchain_source(), now()),
                fees_owed: LiveField::new(vec![], onchain_source(), now()),
            },
        });
        (ActionBody::Amm(remove), onchain_meta())
    }

    #[test]
    fn remove_liquidity_pooled_burn_conforms_to_schema() {
        let (body, meta) = sample_pooled_burn();
        assert_conforms("remove_liquidity", &body, &meta);
    }

    #[test]
    fn remove_liquidity_concentrated_decrease_conforms_to_schema() {
        let (body, meta) = sample_concentrated_decrease();
        assert_conforms("remove_liquidity", &body, &meta);
    }

    /// `ConcentratedBurn` arm: conforms AND emits `kind = "concentrated_burn"`
    /// with only `nftKey` present (no lpToken / liquidityBurn / amountMin /
    /// recipient).
    #[test]
    fn remove_liquidity_concentrated_burn_conforms_and_pins_kind() {
        let (body, meta) = sample_concentrated_burn();
        assert_conforms("remove_liquidity", &body, &meta);

        let lowered = crate::lowering_v2::lower_action(
            &body,
            &meta,
            &crate::lowering_v2::TxMeta {
                from: "0x1111111111111111111111111111111111111111",
                to: "0x2222222222222222222222222222222222222222",
            },
        )
        .unwrap();
        let params = &lowered.context["params"];
        assert_eq!(params["kind"], serde_json::json!("concentrated_burn"));
        assert!(params.get("nftKey").is_some());
        assert!(params.get("lpToken").is_none());
        assert!(params.get("liquidityBurn").is_none());
        assert!(params.get("amountMin").is_none());
        assert!(params.get("recipient").is_none());
    }

    #[test]
    fn remove_liquidity_pooled_burn_one_coin_conforms_to_schema() {
        let (body, meta) = sample_pooled_burn_one_coin();
        assert_conforms("remove_liquidity", &body, &meta);
    }

    #[test]
    fn remove_liquidity_pooled_burn_imbalance_conforms_to_schema() {
        let (body, meta) = sample_pooled_burn_imbalance();
        assert_conforms("remove_liquidity", &body, &meta);
    }
}
