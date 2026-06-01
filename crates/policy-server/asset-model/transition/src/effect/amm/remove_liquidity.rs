//! `RemoveLiquidityAction` reducer — withdraw liquidity / burn LP or position.
//!
//! ## Batch 3 — lifecycle activation
//!
//! Three withdrawal shapes are dispatched off the `RemoveLiquidityParams`
//! variant:
//!
//! * [`PooledBurn`](crate::action::amm::RemoveLiquidityParams::PooledBurn) —
//!   `Uniswap V2` / `SushiV2` / `Curve` / `Balancer V2` LP burn. The LP token
//!   is debited at `lp_amount`, and each `(token_ref, amount)` in `min_out`
//!   is credited (the floor the user signed for — actual out ≥ floor, so
//!   the floor is the conservative under-estimate). `live_inputs.fees_owed`
//!   is also credited if non-empty (matches the convention where the pool
//!   socialises any fees accrued to the position on burn).
//! * [`ConcentratedDecrease`](crate::action::amm::RemoveLiquidityParams::ConcentratedDecrease)
//!   — `Uniswap V3` / `V4` partial decrease of an existing position NFT.
//!   Per the V3 spec the withdrawn token amounts accrue to the NFT's
//!   `tokensOwed0` / `tokensOwed1` rather than transferring to the user, so
//!   the reducer emits **no token credit** here — the user must call
//!   [`CollectFeesAction`](crate::action::amm::CollectFeesAction) (or a
//!   bundled `decrease+collect` multicall) to actually move tokens. The
//!   NFT's internal liquidity change is not representable in today's
//!   [`TokenChange`](policy_state::delta::TokenChange) variants and is
//!   out-of-scope for this sub-agent.
//! * [`ConcentratedBurn`](crate::action::amm::RemoveLiquidityParams::ConcentratedBurn)
//!   — `Uniswap V3` / `V4` burn of an empty position NFT. The NFT ownership
//!   is removed via [`helpers::balance::transfer_nft`] (`-1` `SignedI256`
//!   on the `Owned` holding). Per spec the NFT must have `liquidity == 0`
//!   first; the reducer enforces it defensively by inspecting the
//!   `LpShape::Concentrated.range.liquidity` field on the NFT holding.
//!
//! ## Slippage convention
//!
//! `PooledBurn` credits `min_out` and debits `lp_amount` (the user signed
//! for exactly `lp_amount` LP to burn; the actual mint redemption returns ≥
//! `min_out`). `ConcentratedDecrease` makes no balance change — the V3
//! contract accrues tokens to internal storage and the user collects later.
//!
//! ## Venue mismatch
//!
//! `PooledBurn` against `UniswapV3` / `UniswapV4` / `TraderJoeLB` and the
//! concentrated variants against non-concentrated venues are both rejected
//! as [`ReducerError::UnsupportedProtocol`].

use policy_state::primitives::{Address, U128, U256};
use policy_state::token::{LpShape, RangeSpec, TokenKind};
use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::amm::{AmmVenue, RemoveLiquidityAction, RemoveLiquidityParams};
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

use super::add_liquidity::concentrated_underlyings;

/// Short tag used in `UnsupportedProtocol.protocol` for a venue.
const fn venue_tag(venue: &AmmVenue) -> &'static str {
    match venue {
        AmmVenue::UniswapV2 { .. } => "uniswap_v2",
        AmmVenue::UniswapV3 { .. } => "uniswap_v3",
        AmmVenue::UniswapV4 { .. } => "uniswap_v4",
        AmmVenue::SushiV2 { .. } => "sushi_v2",
        AmmVenue::CurveV1 { .. } => "curve_v1",
        AmmVenue::CurveV2 { .. } => "curve_v2",
        AmmVenue::BalancerV2 { .. } => "balancer_v2",
        AmmVenue::BalancerV3 { .. } => "balancer_v3",
        AmmVenue::TraderJoeLB { .. } => "trader_joe_lb",
        AmmVenue::MaverickV2 { .. } => "maverick_v2",
        AmmVenue::AggregatorRoute { .. } => "aggregator_route",
    }
}

/// Validate that `venue` is pooled-shaped (LP token at a fungible Erc20).
fn require_pooled_venue(venue: &AmmVenue, action: &str) -> ReducerResult<()> {
    match venue {
        AmmVenue::UniswapV2 { .. }
        | AmmVenue::SushiV2 { .. }
        | AmmVenue::CurveV1 { .. }
        | AmmVenue::CurveV2 { .. }
        | AmmVenue::BalancerV2 { .. }
        | AmmVenue::BalancerV3 { .. }
        | AmmVenue::MaverickV2 { .. } => Ok(()),
        _ => Err(ReducerError::UnsupportedProtocol {
            action: action.into(),
            protocol: venue_tag(venue).into(),
        }),
    }
}

/// Validate that `venue` is concentrated-shaped (position NFT).
fn require_concentrated_venue(venue: &AmmVenue, action: &str) -> ReducerResult<()> {
    match venue {
        AmmVenue::UniswapV3 { .. } | AmmVenue::UniswapV4 { .. } | AmmVenue::TraderJoeLB { .. } => {
            Ok(())
        }
        _ => Err(ReducerError::UnsupportedProtocol {
            action: action.into(),
            protocol: venue_tag(venue).into(),
        }),
    }
}

/// Returns `true` if a concentrated-LP `RangeSpec` reports zero in-range
/// liquidity. Non-tick variants (Bin / Custom) conservatively return
/// `false` so the burn is rejected unless the NFT clearly has no active
/// liquidity.
fn concentrated_liquidity_is_zero(range: &RangeSpec) -> bool {
    match range {
        RangeSpec::Tick { liquidity, .. } => *liquidity == U128::ZERO,
        RangeSpec::Bin { .. } | RangeSpec::Custom { .. } => false,
    }
}

impl Reducer for RemoveLiquidityAction {
    fn apply(&self, state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();

        match &self.params {
            // ---------- Pooled burn (V2 / Curve / Balancer) ----------
            RemoveLiquidityParams::PooledBurn {
                lp_token,
                lp_amount,
                min_out,
                recipient: _,
            } => {
                require_pooled_venue(&self.venue, "remove_liquidity")?;
                helpers::balance::debit(state, &mut delta, &lp_token.key, *lp_amount)?;
                for (token_ref, amount) in min_out {
                    helpers::balance::credit(state, &mut delta, &token_ref.key, *amount)?;
                }
                // Pool burn also socialises any position-side fees that have
                // accrued (the LP share of pool fees is auto-compounded into
                // the underlying redemption ratio, so we credit `fees_owed`
                // as an extra rider on top). Zero-amount entries are skipped.
                for (token_ref, amount) in &self.live_inputs.fees_owed.value {
                    if *amount != U256::ZERO {
                        helpers::balance::credit(state, &mut delta, &token_ref.key, *amount)?;
                    }
                }
            }

            // ---------- Pooled burn, single coin (Curve NG) ----------
            RemoveLiquidityParams::PooledBurnOneCoin {
                lp_token,
                lp_amount,
                token_out,
                min_out,
                recipient: _,
            } => {
                require_pooled_venue(&self.venue, "remove_liquidity")?;
                helpers::balance::debit(state, &mut delta, &lp_token.key, *lp_amount)?;
                // Single underlying coin credited at the signed floor (actual
                // out >= min_out, so the floor is the conservative under-estimate).
                helpers::balance::credit(state, &mut delta, &token_out.key, *min_out)?;
                for (token_ref, amount) in &self.live_inputs.fees_owed.value {
                    if *amount != U256::ZERO {
                        helpers::balance::credit(state, &mut delta, &token_ref.key, *amount)?;
                    }
                }
            }

            // ---------- Pooled burn, imbalanced (Curve NG) ----------
            RemoveLiquidityParams::PooledBurnImbalance {
                lp_token,
                max_lp_burn,
                amounts_out,
                recipient: _,
            } => {
                require_pooled_venue(&self.venue, "remove_liquidity")?;
                // The user signed an LP CAP; the actual burn is <= max_lp_burn.
                // Debiting the cap is the conservative over-estimate of LP
                // outflow (mirrors PooledBurn trusting the signed lp_amount).
                helpers::balance::debit(state, &mut delta, &lp_token.key, *max_lp_burn)?;
                for (token_ref, amount) in amounts_out {
                    helpers::balance::credit(state, &mut delta, &token_ref.key, *amount)?;
                }
                for (token_ref, amount) in &self.live_inputs.fees_owed.value {
                    if *amount != U256::ZERO {
                        helpers::balance::credit(state, &mut delta, &token_ref.key, *amount)?;
                    }
                }
            }

            // ---------- Concentrated decrease (V3 / V4 partial) ----------
            RemoveLiquidityParams::ConcentratedDecrease {
                nft_key,
                liquidity_burn,
                amount_min: _,
            } => {
                require_concentrated_venue(&self.venue, "remove_liquidity")?;
                // Defensive existence + kind check — surfaces clean errors
                // before the no-op return.
                let _ = concentrated_underlyings(state, nft_key)?;
                if *liquidity_burn == U128::ZERO {
                    return Err(ReducerError::Invariant(
                        "remove_liquidity concentrated_decrease: liquidity_burn is zero".into(),
                    ));
                }
                // V3 spec: tokens accrue to NFT's tokensOwed0/1 — NOT to the
                // user. The user must `collect` to actually receive them.
                // Reducer therefore emits no token-side change here. NFT's
                // internal liquidity decrease is not representable today.
            }

            // ---------- Concentrated burn (V3 / V4 empty NFT close) ----------
            RemoveLiquidityParams::ConcentratedBurn { nft_key } => {
                require_concentrated_venue(&self.venue, "remove_liquidity")?;
                let holding = state
                    .tokens
                    .get(nft_key)
                    .ok_or_else(|| ReducerError::TokenNotFound(nft_key.clone()))?;
                // Spec: burn requires liquidity == 0.
                if let TokenKind::LpShare {
                    shape: LpShape::Concentrated { range, .. },
                    ..
                } = &holding.kind
                {
                    if !concentrated_liquidity_is_zero(range) {
                        return Err(ReducerError::Invariant(format!(
                            "remove_liquidity concentrated_burn: NFT {nft_key:?} \
                             has non-zero liquidity — call decrease+collect first"
                        )));
                    }
                } else {
                    return Err(ReducerError::Invariant(format!(
                        "concentrated_burn targeted {nft_key:?} but holding is \
                         not LpShare(Concentrated)"
                    )));
                }
                // Drop ownership of the NFT.
                helpers::balance::transfer_nft(state, &mut delta, nft_key, Address::ZERO, None)?;
            }
        }

        Ok(delta)
    }
}

// ===========================================================================
// Inline tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::amm::{
        AmmVenue, RemoveLiquidityAction, RemoveLiquidityLiveInputs, RemoveLiquidityParams,
    };
    use policy_state::delta::TokenChange;
    use policy_state::eval_context::RequestKind;
    use policy_state::live_field::{DataSource, LiveField};
    use policy_state::primitives::{
        Address, ChainId, PoolRef, ProtocolRef, SignedI256, Time, U128, U256,
    };
    use policy_state::token::{
        Balance, BaseCategory, FiatCurrency, LpShape, PegTarget, RangeSpec, ShareForm,
        TokenHolding, TokenKey, TokenKind, TokenRef,
    };
    use policy_state::wallet::WalletId;
    use std::str::FromStr;

    use crate::action::amm::PoolState;

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    fn ctx() -> EvalContext {
        EvalContext::new(ChainId::ethereum_mainnet(), now(), RequestKind::Transaction)
    }

    fn usdc_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        }
    }

    fn weth_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap(),
            },
        }
    }

    fn v2_pool_addr() -> Address {
        Address::from_str("0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc").unwrap()
    }

    fn lp_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: v2_pool_addr(),
            },
        }
    }

    fn fungible_holding(token: &TokenRef, balance: U256) -> TokenHolding {
        let contract = token
            .key
            .contract()
            .copied()
            .unwrap_or_else(|| Address::from([0u8; 20]));
        TokenHolding {
            key: token.key.clone(),
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: "USDC".into(),
            decimals: 18,
            balance: Balance::fungible(balance),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
            last_synced_at: Time::from_unix(1_000_000),
            primitives_source: DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract,
                function: "balanceOf(address)".into(),
                decoder_id: "erc20_balance".into(),
            },
            metadata: None,
            value_usd: None,
        }
    }

    fn v2_venue() -> AmmVenue {
        AmmVenue::UniswapV2 {
            chain: ChainId::ethereum_mainnet(),
            pool: v2_pool_addr(),
            factory: Address::from_str("0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f").unwrap(),
        }
    }

    fn v3_venue() -> AmmVenue {
        AmmVenue::UniswapV3 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640").unwrap(),
            fee_tier_bp: 500,
        }
    }

    fn curve_venue() -> AmmVenue {
        // Curve NG pool == LP token, so `lp_ref` (the v2_pool_addr-keyed token)
        // doubles as both pool and LP here.
        AmmVenue::CurveV1 {
            chain: ChainId::ethereum_mainnet(),
            pool: v2_pool_addr(),
            n_coins: 2,
            is_meta: false,
        }
    }

    fn empty_pool_state() -> PoolState {
        PoolState::XyConstant {
            reserve_in: U256::ZERO,
            reserve_out: U256::ZERO,
            fee_bp: 30,
        }
    }

    fn make_live_inputs(fees: Vec<(TokenRef, U256)>) -> RemoveLiquidityLiveInputs {
        RemoveLiquidityLiveInputs {
            pool_state: LiveField::new(empty_pool_state(), DataSource::UserSupplied, now()),
            fees_owed: LiveField::new(fees, DataSource::UserSupplied, now()),
        }
    }

    fn state_with_lp_and_pair() -> WalletState {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens
            .insert(usdc_ref().key, fungible_holding(&usdc_ref(), U256::ZERO));
        s.tokens
            .insert(weth_ref().key, fungible_holding(&weth_ref(), U256::ZERO));
        s.tokens.insert(
            lp_ref().key,
            fungible_holding(&lp_ref(), U256::from(1_000u64)),
        );
        s
    }

    // ----------------------------------------------------------------------
    // PooledBurn happy path
    // ----------------------------------------------------------------------

    /// Pooled LP burn debits LP and credits each `min_out` entry. With empty
    /// `fees_owed`, the delta is exactly `1 + min_out.len()` changes.
    #[test]
    fn pooled_burn_v2_emits_lp_debit_then_min_out_credits() {
        let state = state_with_lp_and_pair();
        let action = RemoveLiquidityAction {
            venue: v2_venue(),
            params: RemoveLiquidityParams::PooledBurn {
                lp_token: lp_ref(),
                lp_amount: U256::from(100u64),
                min_out: vec![
                    (usdc_ref(), U256::from(500u64)),
                    (weth_ref(), U256::from(800u64)),
                ],
                recipient: user(),
            },
            live_inputs: make_live_inputs(vec![]),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 3);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, lp_ref().key);
                assert!(d.is_negative());
                assert_eq!(d.unsigned_abs().to_string(), "100");
            }
            other => panic!("expected LP debit, got {other:?}"),
        }
        match &delta.token_changes[1] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, usdc_ref().key);
                assert_eq!(*d, SignedI256::try_from(500i64).unwrap());
            }
            other => panic!("expected USDC credit, got {other:?}"),
        }
        match &delta.token_changes[2] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, weth_ref().key);
                assert_eq!(*d, SignedI256::try_from(800i64).unwrap());
            }
            other => panic!("expected WETH credit, got {other:?}"),
        }
    }

    /// Non-zero `fees_owed` adds extra credit rows after the `min_out` ones.
    #[test]
    fn pooled_burn_v2_emits_fees_owed_credits_on_top() {
        let state = state_with_lp_and_pair();
        let action = RemoveLiquidityAction {
            venue: v2_venue(),
            params: RemoveLiquidityParams::PooledBurn {
                lp_token: lp_ref(),
                lp_amount: U256::from(100u64),
                min_out: vec![(usdc_ref(), U256::from(500u64))],
                recipient: user(),
            },
            live_inputs: make_live_inputs(vec![(weth_ref(), U256::from(15u64))]),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        // 1 LP debit + 1 min_out credit + 1 fees_owed credit = 3.
        assert_eq!(delta.token_changes.len(), 3);
        match &delta.token_changes[2] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, weth_ref().key);
                assert_eq!(*d, SignedI256::try_from(15i64).unwrap());
            }
            other => panic!("expected fees_owed credit, got {other:?}"),
        }
    }

    /// `fees_owed` entries with amount = 0 are skipped (no zero-delta noise).
    #[test]
    fn pooled_burn_skips_zero_amount_fees_owed() {
        let state = state_with_lp_and_pair();
        let action = RemoveLiquidityAction {
            venue: v2_venue(),
            params: RemoveLiquidityParams::PooledBurn {
                lp_token: lp_ref(),
                lp_amount: U256::from(100u64),
                min_out: vec![(usdc_ref(), U256::from(500u64))],
                recipient: user(),
            },
            live_inputs: make_live_inputs(vec![(weth_ref(), U256::ZERO)]),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        // 1 LP debit + 1 min_out credit + 0 fees_owed credit = 2.
        assert_eq!(delta.token_changes.len(), 2);
    }

    /// LP underflow surfaces as `Invariant`.
    #[test]
    fn pooled_burn_lp_underflow_returns_invariant() {
        let state = state_with_lp_and_pair();
        let action = RemoveLiquidityAction {
            venue: v2_venue(),
            params: RemoveLiquidityParams::PooledBurn {
                lp_token: lp_ref(),
                lp_amount: U256::from(100_000u64),
                min_out: vec![(usdc_ref(), U256::ZERO)],
                recipient: user(),
            },
            live_inputs: make_live_inputs(vec![]),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("underflow")));
    }

    /// `PooledBurn` against a `UniswapV3` venue rejects with
    /// `UnsupportedProtocol`.
    #[test]
    fn pooled_burn_against_v3_venue_returns_unsupported_protocol() {
        let state = state_with_lp_and_pair();
        let action = RemoveLiquidityAction {
            venue: v3_venue(),
            params: RemoveLiquidityParams::PooledBurn {
                lp_token: lp_ref(),
                lp_amount: U256::from(100u64),
                min_out: vec![(usdc_ref(), U256::from(500u64))],
                recipient: user(),
            },
            live_inputs: make_live_inputs(vec![]),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        match err {
            ReducerError::UnsupportedProtocol { action, protocol } => {
                assert_eq!(action, "remove_liquidity");
                assert_eq!(protocol, "uniswap_v3");
            }
            other => panic!("expected UnsupportedProtocol, got {other:?}"),
        }
    }

    // ----------------------------------------------------------------------
    // PooledBurnOneCoin / PooledBurnImbalance (Curve NG)
    // ----------------------------------------------------------------------

    /// `PooledBurnOneCoin` debits LP and credits the single `token_out` at the
    /// signed floor. Empty `fees_owed` → exactly 2 changes.
    #[test]
    fn pooled_burn_one_coin_debits_lp_credits_single_coin() {
        let state = state_with_lp_and_pair();
        let action = RemoveLiquidityAction {
            venue: curve_venue(),
            params: RemoveLiquidityParams::PooledBurnOneCoin {
                lp_token: lp_ref(),
                lp_amount: U256::from(100u64),
                token_out: usdc_ref(),
                min_out: U256::from(500u64),
                recipient: user(),
            },
            live_inputs: make_live_inputs(vec![]),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 2);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, lp_ref().key);
                assert!(d.is_negative());
                assert_eq!(d.unsigned_abs().to_string(), "100");
            }
            other => panic!("expected LP debit, got {other:?}"),
        }
        match &delta.token_changes[1] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, usdc_ref().key);
                assert_eq!(*d, SignedI256::try_from(500i64).unwrap());
            }
            other => panic!("expected single-coin credit, got {other:?}"),
        }
    }

    /// `PooledBurnImbalance` debits the LP CAP (`max_lp_burn`, conservative
    /// over-estimate) and credits each `amounts_out`. → 1 + 2 = 3 changes.
    #[test]
    fn pooled_burn_imbalance_debits_max_lp_credits_each_coin() {
        let state = state_with_lp_and_pair();
        let action = RemoveLiquidityAction {
            venue: curve_venue(),
            params: RemoveLiquidityParams::PooledBurnImbalance {
                lp_token: lp_ref(),
                max_lp_burn: U256::from(200u64),
                amounts_out: vec![
                    (usdc_ref(), U256::from(300u64)),
                    (weth_ref(), U256::from(400u64)),
                ],
                recipient: user(),
            },
            live_inputs: make_live_inputs(vec![]),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 3);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, lp_ref().key);
                assert!(d.is_negative());
                assert_eq!(d.unsigned_abs().to_string(), "200");
            }
            other => panic!("expected LP cap debit, got {other:?}"),
        }
        match &delta.token_changes[1] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, usdc_ref().key);
                assert_eq!(*d, SignedI256::try_from(300i64).unwrap());
            }
            other => panic!("expected USDC credit, got {other:?}"),
        }
    }

    // ----------------------------------------------------------------------
    // ConcentratedDecrease — V3 reality: no token credit
    // ----------------------------------------------------------------------

    fn concentrated_nft_key() -> TokenKey {
        TokenKey::Erc721 {
            chain: ChainId::ethereum_mainnet(),
            contract: Address::from_str("0xc36442b4a4522e871399cd717abdd847ab11fe88").unwrap(),
            token_id: U256::from(42u64),
        }
    }

    fn concentrated_nft_holding(liquidity: U128) -> TokenHolding {
        TokenHolding {
            key: concentrated_nft_key(),
            kind: TokenKind::LpShare {
                pool: PoolRef {
                    protocol: ProtocolRef::new("uniswap_v3"),
                    pool_addr: None,
                    pool_id: None,
                    fee_tier: None,
                },
                underlyings: vec![usdc_ref(), weth_ref()],
                share_form: ShareForm::NonFungible,
                shape: LpShape::Concentrated {
                    range: RangeSpec::Tick {
                        lower: -100,
                        upper: 100,
                        liquidity,
                    },
                    fees_owed: vec![],
                },
            },
            symbol: "UNI-V3-POS".into(),
            decimals: 0,
            balance: Balance::Owned,
            committed: Balance::Owned,
            approved_to: None,
            price_usd: None,
            last_synced_at: Time::from_unix(1_000_000),
            primitives_source: DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract: Address::from_str("0xc36442b4a4522e871399cd717abdd847ab11fe88").unwrap(),
                function: "positions(uint256)".into(),
                decoder_id: "uniswap_v3_position".into(),
            },
            metadata: None,
            value_usd: None,
        }
    }

    /// `ConcentratedDecrease` matches V3 reality — tokens accrue to the
    /// NFT's internal `tokensOwed`, not to the user. The reducer therefore
    /// emits **no** token-side change. A subsequent `collect` is what
    /// credits the user.
    #[test]
    fn concentrated_decrease_v3_emits_no_token_change() {
        let mut state = state_with_lp_and_pair();
        state.tokens.insert(
            concentrated_nft_key(),
            concentrated_nft_holding(U128::from(1_000u64)),
        );

        let action = RemoveLiquidityAction {
            venue: v3_venue(),
            params: RemoveLiquidityParams::ConcentratedDecrease {
                nft_key: concentrated_nft_key(),
                liquidity_burn: U128::from(500u64),
                amount_min: (U256::from(450u64), U256::from(900u64)),
            },
            live_inputs: make_live_inputs(vec![]),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert!(delta.token_changes.is_empty());
        assert!(delta.position_changes.is_empty());
    }

    /// `ConcentratedDecrease` with zero `liquidity_burn` is rejected.
    #[test]
    fn concentrated_decrease_zero_burn_returns_invariant() {
        let mut state = state_with_lp_and_pair();
        state.tokens.insert(
            concentrated_nft_key(),
            concentrated_nft_holding(U128::from(1_000u64)),
        );
        let action = RemoveLiquidityAction {
            venue: v3_venue(),
            params: RemoveLiquidityParams::ConcentratedDecrease {
                nft_key: concentrated_nft_key(),
                liquidity_burn: U128::from(0u64),
                amount_min: (U256::ZERO, U256::ZERO),
            },
            live_inputs: make_live_inputs(vec![]),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("liquidity_burn")));
    }

    /// Concentrated variant against pooled venue (`UniswapV2`) returns
    /// `UnsupportedProtocol`.
    #[test]
    fn concentrated_decrease_against_v2_venue_returns_unsupported_protocol() {
        let mut state = state_with_lp_and_pair();
        state.tokens.insert(
            concentrated_nft_key(),
            concentrated_nft_holding(U128::from(1_000u64)),
        );
        let action = RemoveLiquidityAction {
            venue: v2_venue(),
            params: RemoveLiquidityParams::ConcentratedDecrease {
                nft_key: concentrated_nft_key(),
                liquidity_burn: U128::from(500u64),
                amount_min: (U256::ZERO, U256::ZERO),
            },
            live_inputs: make_live_inputs(vec![]),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::UnsupportedProtocol { .. }));
    }

    // ----------------------------------------------------------------------
    // ConcentratedBurn — close empty NFT
    // ----------------------------------------------------------------------

    /// `ConcentratedBurn` on an empty (liquidity == 0) NFT emits a -1
    /// `BalanceDelta` on the `Owned` ERC721 holding (`transfer_nft` convention).
    #[test]
    fn concentrated_burn_empty_nft_drops_ownership() {
        let mut state = state_with_lp_and_pair();
        state.tokens.insert(
            concentrated_nft_key(),
            concentrated_nft_holding(U128::from(0u64)),
        );
        let action = RemoveLiquidityAction {
            venue: v3_venue(),
            params: RemoveLiquidityParams::ConcentratedBurn {
                nft_key: concentrated_nft_key(),
            },
            live_inputs: make_live_inputs(vec![]),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 1);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, concentrated_nft_key());
                assert_eq!(*d, -SignedI256::ONE);
            }
            other => panic!("expected -1 BalanceDelta on NFT, got {other:?}"),
        }
    }

    /// `ConcentratedBurn` on an NFT with non-zero liquidity is rejected.
    #[test]
    fn concentrated_burn_with_liquidity_returns_invariant() {
        let mut state = state_with_lp_and_pair();
        state.tokens.insert(
            concentrated_nft_key(),
            concentrated_nft_holding(U128::from(1_000u64)),
        );
        let action = RemoveLiquidityAction {
            venue: v3_venue(),
            params: RemoveLiquidityParams::ConcentratedBurn {
                nft_key: concentrated_nft_key(),
            },
            live_inputs: make_live_inputs(vec![]),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("non-zero liquidity")));
    }

    /// `ConcentratedBurn` against a missing NFT returns `TokenNotFound`.
    #[test]
    fn concentrated_burn_missing_nft_returns_token_not_found() {
        let state = state_with_lp_and_pair();
        let action = RemoveLiquidityAction {
            venue: v3_venue(),
            params: RemoveLiquidityParams::ConcentratedBurn {
                nft_key: concentrated_nft_key(),
            },
            live_inputs: make_live_inputs(vec![]),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::TokenNotFound(_)));
    }
}
