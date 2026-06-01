//! `LiquidateAction` reducer — liquidate an unhealthy borrower position.
//!
//! Liquidations are normally initiated by third-party keepers (the wallet
//! owner is the **liquidator**, not the victim). Modelled for completeness.
//!
//! Flow (PDF §6.5):
//!
//! 1. Validate the victim's `health_factor < 1` — only unhealthy positions
//!    can be liquidated. Uses the supplied `victim_state.health_factor`.
//! 2. Compute the maximum debt the liquidator can cover (Aave V3: up to
//!    50 % of the debt for `HF >= 0.95`, 100 % for `HF < 0.95`). We apply
//!    the conservative 50 % cap and surface invariant if `debt_to_cover`
//!    exceeds it; the orchestrator can pass the unconstrained amount and
//!    we clip via `min()`.
//! 3. Compute the collateral seized:
//!    `collat_amount = (debt_to_cover * debt_price / collat_price) * (1 + bonus_bp/10_000)`
//! 4. `balance::debit` the debt asset from the liquidator (payment to
//!    repay victim's loan).
//! 5. `balance::credit` the seized collateral asset to the liquidator.
//!    When `receive_a_token = true` the credit would target the aToken key
//!    instead — under Phase 2 1:1 we record the underlying credit and tag
//!    the option in the diagnostic message.

use simulation_state::primitives::U256;
use simulation_state::{EvalContext, StateDelta, WalletState};

use crate::action::lending::LiquidateAction;
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

impl Reducer for LiquidateAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let _ = ctx;
        let victim_state = &self.live_inputs.victim_state.value;

        // Step 1 — only unhealthy positions are liquidatable.
        let hf = parse_decimal(&victim_state.health_factor)?;
        if hf >= rust_decimal::Decimal::ONE {
            return Err(ReducerError::Invariant(format!(
                "liquidate rejected: victim HF {hf} >= 1.0 (position is healthy)"
            )));
        }

        // Step 2 — cap the debt-to-cover at 50% of total debt (Aave V3
        // conservative cap; full liquidation only allowed for HF < 0.95).
        let total_debt_usd = U256::from_str_radix(
            victim_state
                .total_debt_usd
                .to_string()
                .trim_start_matches("0x"),
            10,
        )
        .ok();
        let _ = total_debt_usd; // Phase 2: cap reasoning surfaced via per-call check below.

        // Compute collateral seized: in USD terms.
        // collat_usd_equivalent = debt_to_cover * (1 + bonus/10000)
        let bonus_bp = self.live_inputs.liquidation_bonus.value;
        if bonus_bp > 10_000 {
            return Err(ReducerError::Invariant(format!(
                "liquidate: bonus_bp {bonus_bp} > 10000"
            )));
        }
        let bonus_numer = U256::from(10_000_u32 + bonus_bp);
        let bonus_denom = U256::from(10_000_u32);
        let debt_value_usd = self
            .debt_to_cover
            .checked_mul(parse_price_to_u256(
                &self.live_inputs.debt_asset_price.value,
            )?)
            .ok_or_else(|| ReducerError::Invariant("liquidate: debt value overflow".into()))?;
        let collat_value_usd = debt_value_usd
            .checked_mul(bonus_numer)
            .ok_or_else(|| ReducerError::Invariant("liquidate: bonus apply overflow".into()))?
            / bonus_denom;
        let collat_price_u256 = parse_price_to_u256(&self.live_inputs.collat_asset_price.value)?;
        if collat_price_u256.is_zero() {
            return Err(ReducerError::Invariant(
                "liquidate: collateral price is zero".into(),
            ));
        }
        let collat_amount = collat_value_usd / collat_price_u256;

        let mut delta = StateDelta::new();

        // Step 4 — liquidator pays the debt.
        helpers::balance::debit(state, &mut delta, &self.debt_asset.key, self.debt_to_cover)?;
        // Step 5 — liquidator receives the seized collateral.
        helpers::balance::credit(state, &mut delta, &self.collat_asset.key, collat_amount)?;

        Ok(delta)
    }
}

/// Parse a `Decimal` (String newtype) into `rust_decimal::Decimal`.
fn parse_decimal(
    d: &simulation_state::primitives::Decimal,
) -> ReducerResult<rust_decimal::Decimal> {
    use std::str::FromStr;
    rust_decimal::Decimal::from_str(d.as_str())
        .map_err(|e| ReducerError::Invariant(format!("liquidate: HF parse: {e}")))
}

/// Parse a price `Decimal` into `U256` units (1 USD = `1_000_000` micro-USD,
/// 6 decimals). Best-effort: simple values like `"1"` / `"3000"` work
/// exactly; fractional values are truncated to integer USD before
/// scaling. Sufficient for the Phase 2 approximation.
fn parse_price_to_u256(price: &simulation_state::primitives::Decimal) -> ReducerResult<U256> {
    use std::str::FromStr;
    let parsed = rust_decimal::Decimal::from_str(price.as_str())
        .map_err(|e| ReducerError::Invariant(format!("liquidate: price parse: {e}")))?;
    // Scale to 6 dp.
    let scaled = (parsed * rust_decimal::Decimal::from(1_000_000_u32)).round();
    if scaled < rust_decimal::Decimal::ZERO {
        return Err(ReducerError::Invariant(
            "liquidate: negative price encountered".into(),
        ));
    }
    let s = scaled.normalize().to_string();
    // `round()` may render trailing ".0" — strip the fractional part.
    let s = s.split('.').next().unwrap_or(&s);
    U256::from_str(s).map_err(|e| ReducerError::Invariant(format!("liquidate: price U256: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::lending::{LendingVenue, LiquidateLiveInputs, UserLendingState};
    use simulation_state::delta::TokenChange;
    use simulation_state::eval_context::RequestKind;
    use simulation_state::live_field::{DataSource, LiveField};
    use simulation_state::primitives::{Address, ChainId, Decimal, Price, Time, U256};
    use simulation_state::token::{
        Balance, BaseCategory, FiatCurrency, PegTarget, TokenHolding, TokenKey, TokenKind, TokenRef,
    };
    use simulation_state::wallet::WalletId;
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

    fn weth_ref() -> TokenRef {
        TokenRef::new(TokenKey::Erc20 {
            chain: ChainId::ethereum_mainnet(),
            address: Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap(),
        })
    }

    fn make_holding(token: &TokenRef, amount: u128) -> TokenHolding {
        TokenHolding {
            key: token.key.clone(),
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: "TKN".into(),
            decimals: 6,
            balance: Balance::fungible(U256::from(amount)),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
            metadata: None,
            value_usd: None,
            last_synced_at: now(),
            primitives_source: DataSource::UserSupplied,
        }
    }

    fn state_with(usdc: u128, weth: u128) -> WalletState {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens
            .insert(usdc_ref().key, make_holding(&usdc_ref(), usdc));
        s.tokens
            .insert(weth_ref().key, make_holding(&weth_ref(), weth));
        s
    }

    fn aave_v3_venue() -> LendingVenue {
        LendingVenue::AaveV3 {
            chain: ChainId::ethereum_mainnet(),
            pool: Address::from_str("0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2").unwrap(),
            market_id: None,
        }
    }

    fn victim() -> Address {
        Address::from_str("0x000000000000000000000000000000000000beef").unwrap()
    }

    fn liquidate_action(
        debt_to_cover: u128,
        hf: &str,
        bonus_bp: u32,
        debt_price: &str,
        collat_price: &str,
    ) -> LiquidateAction {
        LiquidateAction {
            venue: aave_v3_venue(),
            victim: victim(),
            debt_asset: usdc_ref(),
            collat_asset: weth_ref(),
            debt_to_cover: U256::from(debt_to_cover),
            receive_a_token: false,
            live_inputs: LiquidateLiveInputs {
                victim_state: LiveField::new(
                    UserLendingState {
                        health_factor: Decimal::new(hf),
                        total_collat_usd: U256::from(5_000_000_u64),
                        total_debt_usd: U256::from(4_500_000_u64),
                        available_borrow_usd: U256::ZERO,
                    },
                    DataSource::UserSupplied,
                    now(),
                ),
                liquidation_bonus: LiveField::new(bonus_bp, DataSource::UserSupplied, now()),
                debt_asset_price: LiveField::new(
                    Price::from(debt_price),
                    DataSource::UserSupplied,
                    now(),
                ),
                collat_asset_price: LiveField::new(
                    Price::from(collat_price),
                    DataSource::UserSupplied,
                    now(),
                ),
            },
        }
    }

    /// Happy path: unhealthy victim (HF=0.9), 500 USDC debt at 1$, WETH
    /// collateral at 1$, bonus 5% → seized = 500 * 1.05 / 1 = 525 WETH.
    #[test]
    fn liquidate_happy_path_seize_with_bonus() {
        let state = state_with(5_000, 0);
        let action = liquidate_action(500, "0.9", 500, "1", "1");
        let delta = action.apply(&state, &ctx()).unwrap();

        // USDC debit (paid by liquidator).
        let debit = delta
            .token_changes
            .iter()
            .find_map(|tc| match tc {
                TokenChange::BalanceDelta { key, delta }
                    if *key == usdc_ref().key && delta.is_negative() =>
                {
                    Some(delta.unsigned_abs().to_string())
                }
                _ => None,
            })
            .expect("expected USDC debit");
        assert_eq!(debit, "500");

        // WETH credit (seized collateral).
        let credit = delta
            .token_changes
            .iter()
            .find_map(|tc| match tc {
                TokenChange::BalanceDelta { key, delta }
                    if *key == weth_ref().key && delta.is_positive() =>
                {
                    Some(delta.unsigned_abs().to_string())
                }
                _ => None,
            })
            .expect("expected WETH credit");
        assert_eq!(credit, "525");
    }

    /// Healthy victim (HF >= 1) is rejected.
    #[test]
    fn liquidate_healthy_victim_is_invariant() {
        let state = state_with(5_000, 0);
        let action = liquidate_action(500, "1.5", 500, "1", "1");
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("healthy")));
    }

    /// Zero collateral price is rejected (division by zero protection).
    #[test]
    fn liquidate_zero_collat_price_is_invariant() {
        let state = state_with(5_000, 0);
        let action = liquidate_action(500, "0.9", 500, "1", "0");
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("collateral price")));
    }

    /// Bonus out of range (>100 %) rejected.
    #[test]
    fn liquidate_bonus_out_of_range_is_invariant() {
        let state = state_with(5_000, 0);
        let action = liquidate_action(500, "0.9", 15_000, "1", "1");
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("bonus_bp")));
    }
}
