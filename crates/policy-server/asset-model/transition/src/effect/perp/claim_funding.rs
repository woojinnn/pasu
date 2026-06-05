//! `ClaimFundingAction` reducer — settle accrued funding payments to the wallet.
//! ## Effect
//! For each `(token, amount)` in `live_inputs.claimable`:
//!   - `amount > 0` → credit the wallet (received funding).
//!   - `amount == 0` → no-op (skipped).
//!   - amounts here are always `U256` (the `LiveField` models positive
//!     claimable balances only — funding *paid* is debited at the
//!     time-of-close step and tracked via `ClosePerpLiveInputs::funding_accrued`,
//!     not here).
//! ## Multi-market
//! `self.market` is optional. When `None` the claim covers all markets on
//! the venue (the orchestrator pre-aggregates the per-token totals into
//! `live_inputs.claimable`). Reducer-side this distinction is invisible —
//! we iterate the slice either way.

use policy_state::{EvalContext, StateDelta, WalletState, U256};

use crate::action::perp::ClaimFundingAction;
use crate::apply::Reducer;
use crate::error::ReducerResult;
use crate::helpers;

impl Reducer for ClaimFundingAction {
    fn apply(&self, state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();
        for (token, amount) in &self.live_inputs.claimable.value {
            if *amount == U256::ZERO {
                continue;
            }
            helpers::balance::credit(state, &mut delta, &token.key, *amount)?;
        }
        Ok(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::delta::TokenChange;
    use policy_state::live_field::{DataSource, LiveField, OracleProvider};
    use policy_state::primitives::{Address, ChainId, Time};
    use policy_state::token::{
        Balance, BaseCategory, FiatCurrency, PegTarget, TokenHolding, TokenKey, TokenKind, TokenRef,
    };
    use policy_state::wallet::WalletId;
    use std::str::FromStr;

    use crate::action::perp::{ClaimFundingLiveInputs, PerpVenue};
    use crate::error::ReducerError;

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn ctx() -> EvalContext {
        use policy_state::eval_context::RequestKind;
        EvalContext::new(ChainId::ethereum_mainnet(), now(), RequestKind::Transaction)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    fn usdc_ref() -> TokenRef {
        TokenRef::new(TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
        })
    }

    fn weth_ref() -> TokenRef {
        TokenRef::new(TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap(),
        })
    }

    fn live<T>(value: T) -> LiveField<T> {
        LiveField::new(
            value,
            DataSource::OracleFeed {
                provider: OracleProvider::Chainlink,
                feed_id: "TEST".into(),
            },
            now(),
        )
    }

    fn make_holding(key: TokenKey, amount: u128, symbol: &str) -> TokenHolding {
        let contract = key
            .contract()
            .copied()
            .unwrap_or_else(|| Address::from([0u8; 20]));
        TokenHolding {
            key,
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: symbol.into(),
            decimals: 18,
            balance: Balance::fungible(U256::from(amount)),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
            metadata: None,
            value_usd: None,
            last_synced_at: now(),
            primitives_source: DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract,
                function: "balanceOf(address)".into(),
                decoder_id: "erc20_balance".into(),
            },
        }
    }

    fn state_with_two_tokens() -> WalletState {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens
            .insert(usdc_ref().key, make_holding(usdc_ref().key, 0, "USDC"));
        s.tokens
            .insert(weth_ref().key, make_holding(weth_ref().key, 0, "WETH"));
        s
    }

    /// Single-token claim credits one balance.
    #[test]
    fn claim_funding_single_token_credit() {
        let state = state_with_two_tokens();
        let action = ClaimFundingAction {
            venue: PerpVenue::Hyperliquid {
                chain: ChainId::ethereum_mainnet(),
            },
            market: None,
            live_inputs: ClaimFundingLiveInputs {
                claimable: live(vec![(usdc_ref(), U256::from(123_u64))]),
            },
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 1);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, usdc_ref().key);
                assert!(d.is_positive());
                assert_eq!(d.unsigned_abs().to_string(), "123");
            }
            _ => panic!("expected BalanceDelta"),
        }
    }

    /// Multi-token claim credits each token.
    #[test]
    fn claim_funding_multi_token_credit() {
        let state = state_with_two_tokens();
        let action = ClaimFundingAction {
            venue: PerpVenue::Hyperliquid {
                chain: ChainId::ethereum_mainnet(),
            },
            market: None,
            live_inputs: ClaimFundingLiveInputs {
                claimable: live(vec![
                    (usdc_ref(), U256::from(100_u64)),
                    (weth_ref(), U256::from(5_u64)),
                ]),
            },
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 2);
    }

    /// Zero-amount entries are skipped (no `BalanceDelta` emitted).
    #[test]
    fn claim_funding_zero_amount_skipped() {
        let state = state_with_two_tokens();
        let action = ClaimFundingAction {
            venue: PerpVenue::Hyperliquid {
                chain: ChainId::ethereum_mainnet(),
            },
            market: None,
            live_inputs: ClaimFundingLiveInputs {
                claimable: live(vec![
                    (usdc_ref(), U256::ZERO),
                    (weth_ref(), U256::from(5_u64)),
                ]),
            },
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        // Only WETH BalanceDelta — USDC zero entry skipped.
        assert_eq!(delta.token_changes.len(), 1);
    }

    /// Missing holding → `TokenNotFound` propagated from `helpers::balance::credit`.
    #[test]
    fn claim_funding_missing_holding_returns_token_not_found() {
        let state = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        let action = ClaimFundingAction {
            venue: PerpVenue::Hyperliquid {
                chain: ChainId::ethereum_mainnet(),
            },
            market: None,
            live_inputs: ClaimFundingLiveInputs {
                claimable: live(vec![(usdc_ref(), U256::from(1_u64))]),
            },
        };
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::TokenNotFound(_)));
    }
}
