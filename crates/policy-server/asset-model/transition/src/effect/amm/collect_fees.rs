//! `CollectFeesAction` reducer — `Uniswap V3`-style fee collection.
//!
//! ## Batch 3 — lifecycle activation
//!
//! `collect` on a `Uniswap V3` / `V4` / `TraderJoeLB` position NFT withdraws
//! the `tokensOwed0` / `tokensOwed1` (and any newly-accrued protocol fees
//! since the last `decreaseLiquidity`) to the `recipient` address. Reducer-
//! side we credit `live_inputs.fees_owed` (a snapshot of what the contract
//! reports as owed at simulation time) to the user's holdings of the two
//! underlying tokens.
//!
//! ## Slippage convention
//!
//! `CollectFeesAction` carries no slippage floor — the user signs for
//! exactly "collect whatever is owed". We credit the snapshot value as-is.
//! Stale snapshots are out-of-scope for the reducer; the sync orchestrator
//! is responsible for ensuring `fees_owed` is fresh before signing.
//!
//! ## Venue mismatch
//!
//! Collect targets a concentrated venue. Pooled / aggregator venues are
//! rejected as [`ReducerError::UnsupportedProtocol`] since they have no
//! `collect`-equivalent (V2 / Curve / Balancer auto-compound fees into the
//! LP token's redemption ratio).
//!
//! ## NFT-side state mutation
//!
//! The contract zeros `tokensOwed0` / `tokensOwed1` on the NFT after a
//! successful `collect`. This internal mutation is not representable in
//! today's [`TokenChange`](policy_state::delta::TokenChange) variants
//! (no mechanism for mutating `TokenKind::LpShare.fees_owed`) and is
//! out-of-scope for this sub-agent (state crate is read-only). The NFT
//! lands in `state.tokens` with refreshed `fees_owed` on the next sync.

use policy_state::primitives::U256;
use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::amm::{AmmVenue, CollectFeesAction};
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
        AmmVenue::AaveGsm { .. } => "aave_gsm",
        AmmVenue::AggregatorRoute { .. } => "aggregator_route",
    }
}

/// Validate that `venue` is concentrated-shaped — `collect` is only
/// defined for position NFTs (V3 / V4 / `TraderJoeLB`). Pooled venues
/// (V2 / Curve / Balancer) auto-compound fees and have no `collect`.
fn require_concentrated_venue(venue: &AmmVenue) -> ReducerResult<()> {
    match venue {
        AmmVenue::UniswapV3 { .. } | AmmVenue::UniswapV4 { .. } | AmmVenue::TraderJoeLB { .. } => {
            Ok(())
        }
        _ => Err(ReducerError::UnsupportedProtocol {
            action: "collect_fees".into(),
            protocol: venue_tag(venue).into(),
        }),
    }
}

impl Reducer for CollectFeesAction {
    fn apply(&self, state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        require_concentrated_venue(&self.venue)?;
        // Defensive lookup — the NFT must exist and be `LpShare(Concentrated)`.
        // `concentrated_underlyings` enforces both; we don't need the tokens
        // themselves here (the `fees_owed` snapshot carries explicit
        // `TokenRef`s per fee row).
        let _ = concentrated_underlyings(state, &self.nft_key)?;

        let mut delta = StateDelta::new();

        // Credit each `fees_owed` entry to the recipient (the user). Stale-
        // snapshot validation is the sync orchestrator's responsibility —
        // reducer-side we trust `live_inputs.fees_owed.value`. Zero-amount
        // entries are skipped to avoid no-op deltas.
        for (token_ref, amount) in &self.live_inputs.fees_owed.value {
            if *amount != U256::ZERO {
                helpers::balance::credit(state, &mut delta, &token_ref.key, *amount)?;
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
    use crate::action::amm::{AmmVenue, CollectFeesAction, CollectFeesLiveInputs};
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

    fn concentrated_nft_key() -> TokenKey {
        TokenKey::Erc721 {
            chain: ChainId::ethereum_mainnet(),
            contract: Address::from_str("0xc36442b4a4522e871399cd717abdd847ab11fe88").unwrap(),
            token_id: U256::from(42u64),
        }
    }

    fn v3_venue() -> AmmVenue {
        AmmVenue::UniswapV3 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640").unwrap(),
            fee_tier_bp: 500,
        }
    }

    fn v2_venue() -> AmmVenue {
        AmmVenue::UniswapV2 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc").unwrap(),
            factory: Address::from_str("0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f").unwrap(),
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

    fn concentrated_nft_holding() -> TokenHolding {
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
                        liquidity: U128::from(1_000u64),
                    },
                    fees_owed: vec![
                        (usdc_ref(), U256::from(5u64)),
                        (weth_ref(), U256::from(7u64)),
                    ],
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

    fn make_live_inputs(fees: Vec<(TokenRef, U256)>) -> CollectFeesLiveInputs {
        CollectFeesLiveInputs {
            fees_owed: LiveField::new(fees, DataSource::UserSupplied, now()),
        }
    }

    fn state_with_nft_and_pair() -> WalletState {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens
            .insert(usdc_ref().key, fungible_holding(&usdc_ref(), U256::ZERO));
        s.tokens
            .insert(weth_ref().key, fungible_holding(&weth_ref(), U256::ZERO));
        s.tokens
            .insert(concentrated_nft_key(), concentrated_nft_holding());
        s
    }

    // ----------------------------------------------------------------------
    // Happy path: collect both fee tokens
    // ----------------------------------------------------------------------

    /// Happy path — both fee entries (USDC `5`, WETH `7`) are credited to
    /// the user. The reducer emits exactly two positive `BalanceDelta`
    /// entries in the order of `live_inputs.fees_owed.value`.
    #[test]
    fn collect_fees_v3_credits_both_underlyings() {
        let state = state_with_nft_and_pair();
        let action = CollectFeesAction {
            venue: v3_venue(),
            nft_key: concentrated_nft_key(),
            recipient: user(),
            live_inputs: make_live_inputs(vec![
                (usdc_ref(), U256::from(5u64)),
                (weth_ref(), U256::from(7u64)),
            ]),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 2);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, usdc_ref().key);
                assert_eq!(*d, SignedI256::try_from(5i64).unwrap());
            }
            other => panic!("expected USDC credit, got {other:?}"),
        }
        match &delta.token_changes[1] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, weth_ref().key);
                assert_eq!(*d, SignedI256::try_from(7i64).unwrap());
            }
            other => panic!("expected WETH credit, got {other:?}"),
        }
    }

    /// Empty `fees_owed` produces an empty delta — no-op collect is valid
    /// (e.g. polling for fees on a position with no accrual).
    #[test]
    fn collect_fees_empty_fees_owed_emits_empty_delta() {
        let state = state_with_nft_and_pair();
        let action = CollectFeesAction {
            venue: v3_venue(),
            nft_key: concentrated_nft_key(),
            recipient: user(),
            live_inputs: make_live_inputs(vec![]),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert!(delta.token_changes.is_empty());
    }

    /// Zero-amount entries within `fees_owed` are skipped (avoid no-op
    /// `BalanceDelta` noise).
    #[test]
    fn collect_fees_skips_zero_amount_entries() {
        let state = state_with_nft_and_pair();
        let action = CollectFeesAction {
            venue: v3_venue(),
            nft_key: concentrated_nft_key(),
            recipient: user(),
            live_inputs: make_live_inputs(vec![
                (usdc_ref(), U256::ZERO),
                (weth_ref(), U256::from(7u64)),
            ]),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        // Only the non-zero WETH entry produces a credit.
        assert_eq!(delta.token_changes.len(), 1);
    }

    /// `CollectFees` against a missing NFT returns `TokenNotFound` before
    /// any credit is emitted.
    #[test]
    fn collect_fees_missing_nft_returns_token_not_found() {
        let mut state = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        state
            .tokens
            .insert(usdc_ref().key, fungible_holding(&usdc_ref(), U256::ZERO));
        let action = CollectFeesAction {
            venue: v3_venue(),
            nft_key: concentrated_nft_key(),
            recipient: user(),
            live_inputs: make_live_inputs(vec![(usdc_ref(), U256::from(5u64))]),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::TokenNotFound(_)));
    }

    /// `CollectFees` against a non-concentrated venue (`UniswapV2`) returns
    /// `UnsupportedProtocol` — V2 / Curve / Balancer auto-compound fees and
    /// have no `collect` equivalent.
    #[test]
    fn collect_fees_against_v2_venue_returns_unsupported_protocol() {
        let state = state_with_nft_and_pair();
        let action = CollectFeesAction {
            venue: v2_venue(),
            nft_key: concentrated_nft_key(),
            recipient: user(),
            live_inputs: make_live_inputs(vec![(usdc_ref(), U256::from(5u64))]),
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        match err {
            ReducerError::UnsupportedProtocol { action, protocol } => {
                assert_eq!(action, "collect_fees");
                assert_eq!(protocol, "uniswap_v2");
            }
            other => panic!("expected UnsupportedProtocol, got {other:?}"),
        }
    }
}
