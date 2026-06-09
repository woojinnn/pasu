//! `ChangeLeverageAction` reducer — update the leverage multiplier on a
//! position or account.
//! ## Effect
//! 1. Validate `new_leverage <= max_leverage` (`LiveField`).
//! 2. For each `affected_positions[i]`, upsert the position with:
//!    - `leverage = new_leverage`
//!    - `liq_price = new_liq_prices[i]` (sourced from the `LiveField` vector)
//!
//! No on-chain balance change — leverage adjustment is purely a policy /
//! risk-recompute knob.

use policy_state::live_field::{DataSource, LiveField};
use policy_state::position::PositionKind;
use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::perp::ChangeLeverageAction;
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

use super::math;

impl Reducer for ChangeLeverageAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();

        // On-chain reduction requires live inputs (the Hyperliquid pre-sign
        // path is evaluated through lowering/policy, not this reducer).
        let li = self
            .live_inputs
            .as_ref()
            .ok_or(ReducerError::MissingField("change_leverage.live_inputs"))?;

        // Leverage cap check.
        let new_lev = math::parse_decimal(&self.new_leverage)?;
        let max_lev = math::parse_decimal(&li.max_leverage.value)?;
        if !max_lev.is_zero() && new_lev > max_lev {
            return Err(ReducerError::Invariant(format!(
                "change_leverage: requested {new_lev} > max {max_lev}"
            )));
        }
        if new_lev.is_zero() {
            return Err(ReducerError::Invariant(
                "change_leverage: leverage must be > 0".into(),
            ));
        }

        let liq_prices = &li.new_liq_prices.value;

        for position_id in &li.affected_positions.value {
            // Verify position kind before mutation.
            let position = state
                .positions
                .iter()
                .find(|p| &p.id == position_id)
                .ok_or_else(|| ReducerError::PositionNotFound(position_id.clone()))?;
            if !matches!(position.kind, PositionKind::PerpPosition(_)) {
                return Err(ReducerError::Invariant(format!(
                    "change_leverage: position {position_id} is not a PerpPosition"
                )));
            }

            let new_liq = liq_prices
                .iter()
                .find(|(id, _)| id == position_id)
                .map(|(_, p)| p.clone());

            let new_lev_str = self.new_leverage.clone();
            helpers::position::upsert_perp_position(state, &mut delta, position_id, |p| {
                if let PositionKind::PerpPosition(pp) = &mut p.kind {
                    pp.leverage =
                        LiveField::new(new_lev_str.clone(), DataSource::UserSupplied, ctx.now);
                    if let Some(liq) = new_liq.clone() {
                        pp.liq_price = LiveField::new(liq, DataSource::UserSupplied, ctx.now);
                    }
                }
            })?;
        }

        Ok(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::delta::PositionChange;
    use policy_state::live_field::{DataSource, LiveField, OracleProvider};
    use policy_state::position::{MarginMode, PerpPosition, PerpSide, Position, PositionKind};
    use policy_state::primitives::{
        Address, ChainId, Decimal, MarketRef, ProtocolRef, SignedI256, Time, VenueRef, U256,
    };
    use policy_state::token::{TokenKey, TokenRef};
    use policy_state::wallet::WalletId;
    use std::str::FromStr;

    use crate::action::perp::{ChangeLeverageLiveInputs, PerpVenue};

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

    fn perp_position(id: &str) -> Position {
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
                collateral: vec![(usdc_ref(), U256::from(1_000_u64))],
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

    fn change_lev_action(
        new_lev: &str,
        max_lev: &str,
        affected: Vec<String>,
        liq: Vec<(String, Option<Decimal>)>,
    ) -> ChangeLeverageAction {
        ChangeLeverageAction {
            venue: PerpVenue::GmxV2 {
                chain: ChainId::ethereum_mainnet(),
            },
            market: MarketRef {
                symbol: "ETH-PERP".into(),
                venue: VenueRef::new("gmx_v2"),
            },
            new_leverage: Decimal::new(new_lev),
            live_inputs: Some(ChangeLeverageLiveInputs {
                max_leverage: live(Decimal::new(max_lev)),
                affected_positions: live(affected),
                new_liq_prices: live(liq),
            }),
        }
    }

    /// Happy path: update leverage 5 → 10 on one position with new liq.
    #[test]
    fn change_leverage_updates_position_leverage_and_liq_price() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.positions.push(perp_position("p1"));
        let action = change_lev_action(
            "10",
            "50",
            vec!["p1".to_string()],
            vec![("p1".to_string(), Some(Decimal::new("2800")))],
        );
        let delta = action.apply(&s, &ctx()).unwrap();

        assert_eq!(delta.position_changes.len(), 1);
        match &delta.position_changes[0] {
            PositionChange::Update { id, patch } => {
                assert_eq!(id, "p1");
                let after: Position =
                    serde_json::from_value(patch.fields.get("after").unwrap().clone()).unwrap();
                if let PositionKind::PerpPosition(p) = &after.kind {
                    assert_eq!(p.leverage.value.as_str(), "10");
                    assert_eq!(p.liq_price.value, Some(Decimal::new("2800")));
                }
            }
            _ => panic!("expected Update"),
        }
    }

    /// Leverage > max → Invariant.
    #[test]
    fn change_leverage_above_max_rejected() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.positions.push(perp_position("p1"));
        let action = change_lev_action("100", "50", vec!["p1".to_string()], vec![]);
        let err = action.apply(&s, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("> max")));
    }

    /// Zero leverage → Invariant.
    #[test]
    fn change_leverage_zero_rejected() {
        let s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        let action = change_lev_action("0", "50", vec![], vec![]);
        let err = action.apply(&s, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("must be > 0")));
    }

    /// Affected position missing → `PositionNotFound`.
    #[test]
    fn change_leverage_missing_position_returns_position_not_found() {
        let s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        let action = change_lev_action("10", "50", vec!["ghost".to_string()], vec![]);
        let err = action.apply(&s, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::PositionNotFound(_)));
    }
}
