//! `DelegateBorrowAction` reducer — `Aave` credit delegation.
//! Authorizes `delegatee` to borrow `amount` of `asset` against the wallet
//! owner's collateral. Models on-chain `approveDelegation(delegatee, asset,
//! amount)` on the debt token.
//! Flow (PDF §6.10):
//! 1. Reject venues other than Aave V2 / V3 (other venues don't have
//!    `DelegationAwareDebtToken`).
//! 2. Emit a `TokenChange::ApprovalSet` with the delegatee as spender and
//!    the amount as the allowance. The `key` is the underlying asset's
//!    will substitute in once the registry surfaces them.
//! 3. No `LendingAccount` mutation required — credit delegation does not
//!    alter the wallet's collateral / debt state; only the *spender*'s
//!    ability to borrow on behalf of the wallet.

use policy_state::approval::AllowanceSpec;
use policy_state::delta::TokenChange;
use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::lending::{DelegateBorrowAction, LendingVenue};
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};

impl Reducer for DelegateBorrowAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let _ = state;
        if !matches!(
            self.venue,
            LendingVenue::AaveV2 { .. } | LendingVenue::AaveV3 { .. }
        ) {
            return Err(ReducerError::UnsupportedProtocol {
                action: "delegate_borrow".into(),
                protocol: super::venue_tag(&self.venue).into(),
            });
        }

        // RateMode hint (Variable / Stable) is recorded in the action; on-
        // chain it picks one of two distinct debt-token contracts. Once
        // those addresses are surfaced through a registry, the `key` here
        // will swap to the debt-token `TokenKey` and the rate_mode is
        // captured implicitly. Today we emit ApprovalSet against the
        // underlying asset.
        let _ = &self.rate_mode;

        let mut delta = StateDelta::new();
        delta.token_changes.push(TokenChange::ApprovalSet {
            key: self.asset.key.clone(),
            spender: self.delegatee,
            allowance: AllowanceSpec::new(self.amount, ctx.now),
        });
        Ok(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::delta::TokenChange;
    use policy_state::eval_context::RequestKind;
    use policy_state::primitives::{Address, ChainId, Time, U256};
    use policy_state::token::{RateMode, TokenKey, TokenRef};
    use policy_state::wallet::WalletId;
    use std::str::FromStr;

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn ctx() -> EvalContext {
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

    fn aave_v3_venue() -> LendingVenue {
        LendingVenue::AaveV3 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2").unwrap(),
            market_id: None,
        }
    }

    fn delegatee() -> Address {
        Address::from_str("0x00000000000000000000000000000000000c0de1").unwrap()
    }

    fn action(amount: u128, venue: LendingVenue) -> DelegateBorrowAction {
        DelegateBorrowAction {
            venue,
            asset: usdc_ref(),
            delegatee: delegatee(),
            amount: U256::from(amount),
            rate_mode: RateMode::Variable,
        }
    }

    /// Happy path: emit an `ApprovalSet` with delegatee as spender.
    #[test]
    fn delegate_borrow_happy_path() {
        let state = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        let delta = action(1_000, aave_v3_venue())
            .apply(&state, &ctx())
            .unwrap();
        assert_eq!(delta.token_changes.len(), 1);
        match &delta.token_changes[0] {
            TokenChange::ApprovalSet {
                key,
                spender,
                allowance,
            } => {
                assert_eq!(*key, usdc_ref().key);
                assert_eq!(*spender, delegatee());
                assert_eq!(allowance.amount, U256::from(1_000u64));
                assert!(!allowance.is_unlimited);
            }
            other => panic!("expected ApprovalSet, got {other:?}"),
        }
    }

    /// Non-Aave venue: `UnsupportedProtocol`.
    #[test]
    fn delegate_non_aave_unsupported() {
        let state = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        let v = LendingVenue::CompoundV2 {
            chain: ChainId::ethereum_mainnet(),
            comptroller: Address::from_str("0x3d9819210a31b4961b30ef54be2aed79b9c9cd3b").unwrap(),
        };
        let err = action(1_000, v).apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::UnsupportedProtocol { .. }));
    }
}
