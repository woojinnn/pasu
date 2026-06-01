//! `DecreasePerpAction` reducer — partially reduce an existing perpetual position.
//! ## Effect
//! 1. Resolve the reduction `Δsize` from the action's `SizeSpec` against the
//!    `LiveField` mark price.
//! 2. Validate `Δsize ≤ old_size_base` (cannot reduce more than open).
//! 3. Compute realized `PnL` pro-rata: `realized = unrealized_pnl_now × Δsize
//!    / old_size_base`. Fee = `fee_bp × Δsize × mark / 10_000`. Funding is
//!    settled in full (matches venue behaviour — funding is per-position not
//!    per-size).
//! 4. Apply cash delta to the primary collateral token.
//! 5. Upsert the position with `new_size_base = old_size − Δsize`. Entry
//!    price is preserved (decrease does not move the average entry).
//! ## Full close
//! If `Δsize == old_size_base` the action redirects to
//! [`super::close::ClosePerpAction`] via an `Invariant` with guidance —
//! Decrease only partially shrinks.

use policy_state::position::PositionKind;
use policy_state::primitives::SignedI256;
use policy_state::{EvalContext, StateDelta, WalletState, U256};

use crate::action::perp::DecreasePerpAction;
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

use super::{common, math};

impl Reducer for DecreasePerpAction {
    #[allow(clippy::too_many_lines)]
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let _ = ctx;

        if common::is_orderbook_venue(&self.venue) {
            return Err(ReducerError::Invariant(format!(
                "decrease_perp: orderbook venue {} requires PlaceLimitOrderAction \
                 (with reduce_only=true), not DecreasePerpAction",
                common::venue_tag(&self.venue),
            )));
        }

        let mut delta = StateDelta::new();

        let position = state
            .positions
            .iter()
            .find(|p| p.id == self.position_id)
            .ok_or_else(|| ReducerError::PositionNotFound(self.position_id.clone()))?;
        let PositionKind::PerpPosition(old_perp) = &position.kind else {
            return Err(ReducerError::Invariant(format!(
                "decrease_perp: position {} is not a PerpPosition",
                self.position_id
            )));
        };

        let delta_size = math::resolve_size_base(&self.size, &self.live_inputs.mark_price.value)?;
        if delta_size == U256::ZERO {
            return Err(ReducerError::Invariant(
                "decrease_perp: resolved Δsize is zero".into(),
            ));
        }
        if delta_size > old_perp.size_base {
            return Err(ReducerError::Invariant(format!(
                "decrease_perp: Δsize {delta_size} > open size_base {}",
                old_perp.size_base,
            )));
        }
        if delta_size == old_perp.size_base {
            return Err(ReducerError::Invariant(
                "decrease_perp: Δsize equals open size — use ClosePerpAction for full close".into(),
            ));
        }

        // Pro-rata realized PnL.
        let old_size_d = math::u256_to_decimal(old_perp.size_base)?;
        let delta_size_d = math::u256_to_decimal(delta_size)?;
        let pnl_d = rust_decimal::Decimal::from_str_radix(
            &self.live_inputs.unrealized_pnl_now.value.to_string(),
            10,
        )
        .map_err(|e| ReducerError::Invariant(format!("decrease_perp: pnl parse: {e}")))?;
        let realized = pnl_d * delta_size_d / old_size_d;

        let mark = math::parse_decimal(&self.live_inputs.mark_price.value)?;
        let notional = delta_size_d * mark;
        let fee = notional * rust_decimal::Decimal::from(self.live_inputs.fee_bp.value)
            / rust_decimal::Decimal::from(10_000_u32);

        let funding_d = rust_decimal::Decimal::from_str_radix(
            &self.live_inputs.funding_accrued.value.to_string(),
            10,
        )
        .map_err(|e| ReducerError::Invariant(format!("decrease_perp: funding parse: {e}")))?;

        let cash_delta_d = realized - fee + funding_d;
        let cash_delta_str = cash_delta_d.trunc().to_string();
        let cash_delta: SignedI256 = SignedI256::from_dec_str(&cash_delta_str).map_err(|e| {
            ReducerError::Invariant(format!(
                "decrease_perp: cash_delta {cash_delta_str} overflow: {e}"
            ))
        })?;

        let (collateral_token, _) = old_perp.collateral.first().cloned().ok_or_else(|| {
            ReducerError::Invariant(format!(
                "decrease_perp: position {} has no collateral",
                self.position_id
            ))
        })?;

        if cash_delta.is_positive() {
            let abs = cash_delta.unsigned_abs();
            let abs_u256 = U256::from_str_radix(&abs.to_string(), 10).map_err(|e| {
                ReducerError::Invariant(format!("decrease_perp: abs U256 parse: {e}"))
            })?;
            helpers::balance::credit(state, &mut delta, &collateral_token.key, abs_u256)?;
        } else if cash_delta.is_negative() {
            let abs = cash_delta.unsigned_abs();
            let abs_u256 = U256::from_str_radix(&abs.to_string(), 10).map_err(|e| {
                ReducerError::Invariant(format!("decrease_perp: abs U256 parse: {e}"))
            })?;
            helpers::balance::debit(state, &mut delta, &collateral_token.key, abs_u256)?;
        }

        // Shrink the position size.
        let new_size = old_perp.size_base.checked_sub(delta_size).ok_or_else(|| {
            ReducerError::Invariant("decrease_perp: size subtraction underflow".into())
        })?;
        let new_size_d = math::u256_to_decimal(new_size)?;
        let new_notional_d = new_size_d * mark;
        let new_notional_u256 = U256::from_str_radix(&new_notional_d.trunc().to_string(), 10)
            .map_err(|e| {
                ReducerError::Invariant(format!("decrease_perp: notional U256 parse: {e}"))
            })?;

        helpers::position::upsert_perp_position(state, &mut delta, &self.position_id, |p| {
            if let PositionKind::PerpPosition(pp) = &mut p.kind {
                pp.size_base = new_size;
                pp.notional_usd = new_notional_u256;
                // entry_price unchanged.
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
    use policy_state::position::{MarginMode, PerpPosition, PerpSide, Position};
    use policy_state::primitives::{
        Address, ChainId, Decimal, MarketRef, ProtocolRef, Time, VenueRef,
    };
    use policy_state::token::{
        Balance, BaseCategory, FiatCurrency, PegTarget, TokenHolding, TokenKey, TokenKind, TokenRef,
    };
    use policy_state::wallet::WalletId;
    use std::str::FromStr;

    use crate::action::perp::{ClosePerpLiveInputs, PerpVenue, SizeSpec};

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

    fn perp_position(id: &str, size: u64, entry: &str, locked: u64) -> Position {
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
                size_base: U256::from(size),
                notional_usd: U256::ZERO,
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

    fn decrease_action(
        id: &str,
        delta_amount: u64,
        mark_str: &str,
        pnl: i64,
        funding: i64,
        fee_bp: u32,
    ) -> DecreasePerpAction {
        DecreasePerpAction {
            venue: PerpVenue::GmxV2 {
                chain: ChainId::ethereum_mainnet(),
            },
            position_id: id.to_string(),
            size: SizeSpec::BaseAmount {
                amount: U256::from(delta_amount),
            },
            slippage_bp: 50,
            live_inputs: ClosePerpLiveInputs {
                mark_price: live(Decimal::new(mark_str)),
                unrealized_pnl_now: live(SignedI256::try_from(pnl).unwrap()),
                funding_accrued: live(SignedI256::try_from(funding).unwrap()),
                fee_bp: live(fee_bp),
            },
        }
    }

    /// Decrease 1 of 2 ETH with `PnL` +200: pro-rata realized = 100. No fee /
    /// funding → +100 credit. Size shrinks to 1.
    #[test]
    fn decrease_realizes_prorata_pnl_and_shrinks_size() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(10_000));
        s.positions.push(perp_position("p1", 2, "3000", 1_000));

        let action = decrease_action("p1", 1, "3000", 200, 0, 0);
        let delta = action.apply(&s, &ctx()).unwrap();

        assert_eq!(delta.token_changes.len(), 1);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { delta: d, .. } => {
                assert!(d.is_positive());
                assert_eq!(d.unsigned_abs().to_string(), "100");
            }
            _ => panic!("expected BalanceDelta"),
        }
        assert_eq!(delta.position_changes.len(), 1);
        match &delta.position_changes[0] {
            PositionChange::Update { id, patch } => {
                assert_eq!(id, "p1");
                let after: Position =
                    serde_json::from_value(patch.fields.get("after").unwrap().clone()).unwrap();
                if let PositionKind::PerpPosition(p) = &after.kind {
                    assert_eq!(p.size_base, U256::from(1_u64));
                }
            }
            _ => panic!("expected Update"),
        }
    }

    /// Full-size decrease redirected to `ClosePerpAction`.
    #[test]
    fn decrease_full_size_rejected() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(10_000));
        s.positions.push(perp_position("p1", 1, "3000", 1_000));
        let action = decrease_action("p1", 1, "3000", 0, 0, 0);
        let err = action.apply(&s, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("full close")));
    }

    /// Over-size decrease rejected.
    #[test]
    fn decrease_over_size_rejected() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(10_000));
        s.positions.push(perp_position("p1", 1, "3000", 1_000));
        let action = decrease_action("p1", 2, "3000", 0, 0, 0);
        let err = action.apply(&s, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("> open size_base")));
    }
}
