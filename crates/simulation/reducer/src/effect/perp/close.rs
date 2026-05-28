//! `ClosePerpAction` reducer — fully (or partially via `size = Some(..)`)
//! close an existing perpetual position.
//!
//! ## Settlement order
//!
//! 1. Look up the position by `position_id` (effective state — committed
//!    `state.positions` overlaid with any in-flight `PositionChange::Open`
//!    on `delta`).
//! 2. Compute final cash delta = `unrealized_pnl_now − fee_bp × notional /
//!    10_000 + funding_accrued` (all from `ClosePerpLiveInputs`).
//! 3. Apply the cash delta to the position's first collateral token:
//!    positive → credit, negative → debit.
//! 4. Credit the released collateral back to the wallet (the venue returns
//!    the locked margin).
//! 5. Emit `PositionChange::Close { id }`.
//!
//! ## Partial close (size = `Some(..)`)
//!
//! Partial close is **deferred** for Phase 2 — the spec carries the
//! `size: Option<SizeSpec>` field but ergonomic partial-close requires
//! either:
//!   - a `DecreasePerpAction` (already exists for the partial path), or
//!   - position field-walker (`PositionPatch.fields` `after` snapshot
//!     replacement is the wrong shape for "shrink `size_base`").
//!
//! For now `ClosePerpAction { size: Some(_), … }` returns `Invariant` with
//! a guiding message. Phase 3 can lift this by routing the
//! partial path through `DecreasePerpAction` synthesis.

use simulation_state::delta::PositionChange;
use simulation_state::position::PositionKind;
use simulation_state::primitives::SignedI256;
use simulation_state::{EvalContext, StateDelta, WalletState, U256};

use crate::action::perp::ClosePerpAction;
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

use super::math;

impl Reducer for ClosePerpAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let _ = ctx;

        if self.size.is_some() {
            return Err(ReducerError::Invariant(
                "close_perp: partial close (size = Some) deferred — use DecreasePerpAction".into(),
            ));
        }

        let mut delta = StateDelta::new();

        // Look up the position in `state.positions` (only — close already
        // queued on `delta` would be a self-conflict the caller should
        // prevent at the action layer).
        let position = state
            .positions
            .iter()
            .find(|p| p.id == self.position_id)
            .ok_or_else(|| ReducerError::PositionNotFound(self.position_id.clone()))?;

        let PositionKind::PerpPosition(perp) = &position.kind else {
            return Err(ReducerError::Invariant(format!(
                "close_perp: position {} is not a PerpPosition",
                self.position_id
            )));
        };

        // Compute cash delta: PnL − fee + funding.
        let pnl = self.live_inputs.unrealized_pnl_now.value;
        let funding = self.live_inputs.funding_accrued.value;
        let notional = math::notional_usd(perp.size_base, &self.live_inputs.mark_price.value)?;
        let fee = notional * rust_decimal::Decimal::from(self.live_inputs.fee_bp.value)
            / rust_decimal::Decimal::from(10_000_u32);
        // Convert fee to SignedI256 negative (it always reduces cash).
        let fee_str = fee.trunc().to_string();
        let fee_signed: SignedI256 = SignedI256::from_dec_str(&fee_str).map_err(|e| {
            ReducerError::Invariant(format!("close_perp: fee {fee_str} overflow: {e}"))
        })?;
        let cash_delta = pnl
            .checked_sub(fee_signed)
            .ok_or_else(|| {
                ReducerError::Invariant("close_perp: cash_delta subtraction overflow".into())
            })?
            .checked_add(funding)
            .ok_or_else(|| {
                ReducerError::Invariant("close_perp: cash_delta addition overflow".into())
            })?;

        // Primary collateral token for cash settlement.
        let (collateral_token, locked_amount) =
            perp.collateral.first().cloned().ok_or_else(|| {
                ReducerError::Invariant(format!(
                    "close_perp: position {} has no collateral",
                    self.position_id
                ))
            })?;

        // Credit the locked margin back to the wallet (the venue returns it).
        helpers::balance::credit(state, &mut delta, &collateral_token.key, locked_amount)?;

        // Apply cash delta to the wallet's collateral balance.
        if cash_delta.is_positive() {
            let abs = cash_delta.unsigned_abs();
            let abs_u256 = U256::from_str_radix(&abs.to_string(), 10).map_err(|e| {
                ReducerError::Invariant(format!("close_perp: cash_delta abs U256 parse: {e}"))
            })?;
            helpers::balance::credit(state, &mut delta, &collateral_token.key, abs_u256)?;
        } else if cash_delta.is_negative() {
            let abs = cash_delta.unsigned_abs();
            let abs_u256 = U256::from_str_radix(&abs.to_string(), 10).map_err(|e| {
                ReducerError::Invariant(format!("close_perp: cash_delta abs U256 parse: {e}"))
            })?;
            helpers::balance::debit(state, &mut delta, &collateral_token.key, abs_u256)?;
        }
        // Zero cash_delta → no balance change.

        // Close the position.
        delta.position_changes.push(PositionChange::Close {
            id: self.position_id.clone(),
        });

        Ok(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use simulation_state::delta::TokenChange;
    use simulation_state::live_field::{DataSource, LiveField, OracleProvider};
    use simulation_state::position::{MarginMode, PerpPosition, PerpSide, Position, PositionKind};
    use simulation_state::primitives::{
        Address, ChainId, Decimal, MarketRef, ProtocolRef, Time, VenueRef,
    };
    use simulation_state::token::{
        Balance, BaseCategory, FiatCurrency, PegTarget, TokenHolding, TokenKey, TokenKind, TokenRef,
    };
    use simulation_state::wallet::WalletId;
    use std::str::FromStr;

    use crate::action::perp::{ClosePerpLiveInputs, PerpVenue};

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    fn ctx() -> EvalContext {
        use simulation_state::eval_context::RequestKind;
        EvalContext::new(ChainId::ethereum_mainnet(), now(), RequestKind::Transaction)
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
            last_synced_at: now(),
            primitives_source: DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract: usdc_ref().key.contract().copied().unwrap(),
                function: "balanceOf(address)".into(),
                decoder_id: "erc20_balance".into(),
            },
        }
    }

    fn mock_perp_position(id: &str, size_base: u64, entry: &str, locked: u64) -> Position {
        Position {
            id: id.to_string(),
            protocol: ProtocolRef::new("hyperliquid"),
            chain: None,
            kind: PositionKind::PerpPosition(PerpPosition {
                venue: VenueRef::new("hyperliquid"),
                market: MarketRef {
                    symbol: "ETH-PERP".into(),
                    venue: VenueRef::new("hyperliquid"),
                },
                side: PerpSide::Long,
                size_base: U256::from(size_base),
                notional_usd: U256::from(0u64),
                collateral: vec![(usdc_ref(), U256::from(locked))],
                entry_price: Decimal::new(entry),
                margin_mode: MarginMode::Isolated,
                mark_price: live(Decimal::new("0")),
                liq_price: live(None),
                unrealized_pnl: live(SignedI256::ZERO),
                funding_owed: live(SignedI256::ZERO),
                leverage: live(Decimal::new("5")),
            }),
            primitives_synced_at: now(),
            primitives_source: DataSource::UserSupplied,
        }
    }

    fn close_action(
        id: &str,
        pnl: i64,
        funding: i64,
        fee_bp: u32,
        mark_str: &str,
    ) -> ClosePerpAction {
        ClosePerpAction {
            venue: PerpVenue::Hyperliquid {
                chain: ChainId::ethereum_mainnet(),
            },
            position_id: id.to_string(),
            size: None,
            slippage_bp: 50,
            live_inputs: ClosePerpLiveInputs {
                mark_price: live(Decimal::new(mark_str)),
                unrealized_pnl_now: live(SignedI256::try_from(pnl).unwrap()),
                funding_accrued: live(SignedI256::try_from(funding).unwrap()),
                fee_bp: live(fee_bp),
            },
        }
    }

    fn state_with_position(pos: Position, balance: u128) -> WalletState {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(balance));
        s.positions.push(pos);
        s
    }

    /// Profitable close: PnL=+200, funding=+10, fee=0. Locked=1000 → wallet
    /// gets 1000 (release) + 210 (cash) = 1210 credit; position closed.
    #[test]
    fn close_profitable_credits_collateral_and_pnl() {
        let pos = mock_perp_position("perp1", 1, "3000", 1_000);
        let state = state_with_position(pos, 10_000);
        let action = close_action("perp1", 200, 10, 0, "3100");
        let delta = action.apply(&state, &ctx()).unwrap();

        // 2 BalanceDelta (collateral release + cash credit) + 1 Close.
        assert_eq!(delta.token_changes.len(), 2);
        assert_eq!(delta.position_changes.len(), 1);
        match &delta.position_changes[0] {
            PositionChange::Close { id } => assert_eq!(id, "perp1"),
            other => panic!("expected Close, got {other:?}"),
        }

        // Both balance changes are USDC credits.
        let mut total_credit = 0_i64;
        for tc in &delta.token_changes {
            match tc {
                TokenChange::BalanceDelta { key, delta: d } => {
                    assert_eq!(key, &usdc_ref().key);
                    assert!(d.is_positive());
                    total_credit += d.unsigned_abs().to_string().parse::<i64>().unwrap();
                }
                _ => panic!("expected BalanceDelta"),
            }
        }
        assert_eq!(total_credit, 1_210);
    }

    /// Losing close: PnL=−500, funding=0, fee=0. Locked=1000 → wallet gets
    /// 1000 (release) and 500 (cash debit).
    #[test]
    fn close_loss_debits_cash_after_release() {
        let pos = mock_perp_position("perp2", 1, "3000", 1_000);
        let state = state_with_position(pos, 10_000);
        let action = close_action("perp2", -500, 0, 0, "2500");
        let delta = action.apply(&state, &ctx()).unwrap();

        assert_eq!(delta.token_changes.len(), 2);
        assert_eq!(delta.position_changes.len(), 1);
        let mut net: i64 = 0;
        for tc in &delta.token_changes {
            if let TokenChange::BalanceDelta { delta: d, .. } = tc {
                let s: i64 = d.to_string().parse().unwrap();
                net += s;
            }
        }
        // Release +1000, debit -500 → net +500.
        assert_eq!(net, 500);
    }

    /// Partial close (size = Some) is deferred.
    #[test]
    fn close_partial_returns_invariant() {
        let pos = mock_perp_position("perp3", 1, "3000", 1_000);
        let state = state_with_position(pos, 10_000);
        let mut action = close_action("perp3", 0, 0, 0, "3000");
        action.size = Some(crate::action::perp::SizeSpec::BaseAmount {
            amount: U256::from(1_u64),
        });
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("partial close")));
    }

    /// Missing position → `PositionNotFound`.
    #[test]
    fn close_missing_position_returns_position_not_found() {
        let state = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        let action = close_action("ghost", 0, 0, 0, "3000");
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::PositionNotFound(_)));
    }
}
