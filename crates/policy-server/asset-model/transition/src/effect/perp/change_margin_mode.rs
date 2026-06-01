//! `ChangeMarginModeAction` reducer — toggle between cross and isolated margin.
//! ## Effect
//! For each affected position:
//!   - Update `margin_mode` to `self.new_mode`.
//!   - Adjust `collateral[0].1` to the matching entry in
//!     `margin_reallocation` (`Cross → Isolated` reduces the isolated slot
//!     to the position's allocated chunk; `Isolated → Cross` returns
//!     unused collateral to the shared cross pool).
//!
//! No wallet-side balance change — margin reallocation happens within the
//! venue subaccount; the user's on-chain collateral total is unchanged.

use policy_state::position::PositionKind;
use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::perp::ChangeMarginModeAction;
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

impl Reducer for ChangeMarginModeAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let _ = ctx;
        let mut delta = StateDelta::new();

        let reallocs = &self.live_inputs.margin_reallocation.value;

        for position_id in &self.live_inputs.affected_positions.value {
            // Verify position exists and is a PerpPosition.
            let position = state
                .positions
                .iter()
                .find(|p| &p.id == position_id)
                .ok_or_else(|| ReducerError::PositionNotFound(position_id.clone()))?;
            if !matches!(position.kind, PositionKind::PerpPosition(_)) {
                return Err(ReducerError::Invariant(format!(
                    "change_margin_mode: position {position_id} is not a PerpPosition"
                )));
            }

            let new_collat_amount = reallocs
                .iter()
                .find(|(id, _)| id == position_id)
                .map(|(_, amt)| *amt);

            let new_mode = self.new_mode.clone();
            helpers::position::upsert_perp_position(state, &mut delta, position_id, |p| {
                if let PositionKind::PerpPosition(pp) = &mut p.kind {
                    pp.margin_mode = new_mode.clone();
                    if let (Some(first), Some(amt)) = (pp.collateral.first_mut(), new_collat_amount)
                    {
                        first.1 = amt;
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

    use crate::action::perp::{ChangeMarginModeLiveInputs, PerpVenue};

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

    fn perp_position(id: &str, mode: MarginMode, locked: u64) -> Position {
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
                margin_mode: mode,
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

    fn change_mode_action(
        new_mode: MarginMode,
        affected: Vec<String>,
        realloc: Vec<(String, U256)>,
    ) -> ChangeMarginModeAction {
        ChangeMarginModeAction {
            venue: PerpVenue::GmxV2 {
                chain: ChainId::ethereum_mainnet(),
            },
            market: MarketRef {
                symbol: "ETH-PERP".into(),
                venue: VenueRef::new("gmx_v2"),
            },
            new_mode,
            live_inputs: ChangeMarginModeLiveInputs {
                affected_positions: live(affected),
                margin_reallocation: live(realloc),
            },
        }
    }

    /// Cross → Isolated: reduces collateral slot to allocated chunk.
    #[test]
    fn change_margin_mode_cross_to_isolated_reduces_collateral_slot() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.positions
            .push(perp_position("p1", MarginMode::Cross, 5_000));
        let action = change_mode_action(
            MarginMode::Isolated,
            vec!["p1".to_string()],
            vec![("p1".to_string(), U256::from(1_500_u64))],
        );
        let delta = action.apply(&s, &ctx()).unwrap();

        match &delta.position_changes[0] {
            PositionChange::Update { patch, .. } => {
                let after: Position =
                    serde_json::from_value(patch.fields.get("after").unwrap().clone()).unwrap();
                if let PositionKind::PerpPosition(p) = &after.kind {
                    assert!(matches!(p.margin_mode, MarginMode::Isolated));
                    assert_eq!(p.collateral[0].1, U256::from(1_500_u64));
                }
            }
            _ => panic!("expected Update"),
        }
    }

    /// Affected position missing → `PositionNotFound`.
    #[test]
    fn change_margin_mode_missing_position_returns_position_not_found() {
        let s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        let action = change_mode_action(MarginMode::Cross, vec!["ghost".to_string()], vec![]);
        let err = action.apply(&s, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::PositionNotFound(_)));
    }
}
