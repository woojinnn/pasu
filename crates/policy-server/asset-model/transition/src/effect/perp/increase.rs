//! `IncreasePerpAction` reducer — add size to an existing perpetual position.
//! ## Effect
//! 1. Resolve the additional `size_base` from the action's `SizeSpec` against
//!    the `LiveField` mark price.
//! 2. (Optional) Debit any extra collateral the user posts via
//!    `add_collateral`.
//! 3. Upsert the position via `helpers::position::upsert_perp_position`:
//!    - `new_size_base = old_size + Δsize`
//!    - `new_entry = (old_size × old_entry + Δsize × mark) / new_size`
//!      (weighted average entry price)
//!    - `collateral[0].1 += Δcollateral` (if posted)
//!    - `notional_usd = new_size × mark`
//! ## Orderbook vs on-chain
//! (Hyperliquid / Aevo / `DyDx` V4) the increase is a separate orderbook
//! signing event that should be modeled by a dedicated
//! `PlaceOrderAction` (with `reduce_only = false`) and is therefore
//! out of scope for this reducer. The reducer rejects orderbook venues
//! with `Invariant`.

use policy_state::position::PositionKind;
use policy_state::{EvalContext, StateDelta, WalletState, U256};

use crate::action::perp::IncreasePerpAction;
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

use super::{common, math};

impl Reducer for IncreasePerpAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let _ = ctx;

        if common::is_orderbook_venue(&self.venue) {
            return Err(ReducerError::Invariant(format!(
                "increase_perp: orderbook venue {} requires PlaceOrderAction \
                 (with reduce_only=false), not IncreasePerpAction",
                common::venue_tag(&self.venue),
            )));
        }

        let mut delta = StateDelta::new();

        // Look up the existing position.
        let position = state
            .positions
            .iter()
            .find(|p| p.id == self.position_id)
            .ok_or_else(|| ReducerError::PositionNotFound(self.position_id.clone()))?;
        let PositionKind::PerpPosition(old_perp) = &position.kind else {
            return Err(ReducerError::Invariant(format!(
                "increase_perp: position {} is not a PerpPosition",
                self.position_id
            )));
        };

        let delta_size = math::resolve_size_base(&self.size, &self.live_inputs.mark_price.value)?;
        if delta_size == U256::ZERO {
            return Err(ReducerError::Invariant(
                "increase_perp: resolved Δsize is zero".into(),
            ));
        }

        // Weighted-average entry price.
        let old_size = math::u256_to_decimal(old_perp.size_base)?;
        let new_size_u256 = old_perp.size_base.checked_add(delta_size).ok_or_else(|| {
            ReducerError::Invariant("increase_perp: size addition overflow".into())
        })?;
        let new_size = math::u256_to_decimal(new_size_u256)?;
        let old_entry = math::parse_decimal(&old_perp.entry_price)?;
        let mark = math::parse_decimal(&self.live_inputs.mark_price.value)?;
        let new_entry_d =
            (old_size * old_entry + math::u256_to_decimal(delta_size)? * mark) / new_size;

        // Optional extra collateral debit.
        if let Some((collateral_token, amount)) = &self.add_collateral {
            helpers::balance::debit(state, &mut delta, &collateral_token.key, *amount)?;
        }

        let new_notional_d = new_size * mark;
        let new_notional_u256 = U256::from_str_radix(&new_notional_d.trunc().to_string(), 10)
            .map_err(|e| {
                ReducerError::Invariant(format!("increase_perp: notional U256 parse: {e}"))
            })?;
        let extra_collat = self.add_collateral.clone();
        let new_entry_state = math::decimal_to_state(new_entry_d);

        helpers::position::upsert_perp_position(state, &mut delta, &self.position_id, |p| {
            if let PositionKind::PerpPosition(pp) = &mut p.kind {
                pp.size_base = new_size_u256;
                pp.notional_usd = new_notional_u256;
                pp.entry_price = new_entry_state;
                if let Some((tok, amt)) = extra_collat {
                    let mut found = false;
                    for (t, a) in &mut pp.collateral {
                        if t == &tok {
                            *a = a.saturating_add(amt);
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        pp.collateral.push((tok, amt));
                    }
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
    use policy_state::position::{MarginMode, PerpPosition, PerpSide, Position};
    use policy_state::primitives::{
        Address, ChainId, Decimal, MarketRef, ProtocolRef, SignedI256, Time, VenueRef,
    };
    use policy_state::token::{
        Balance, BaseCategory, FiatCurrency, PegTarget, TokenHolding, TokenKey, TokenKind, TokenRef,
    };
    use policy_state::wallet::WalletId;
    use std::str::FromStr;

    use crate::action::perp::{OpenPerpLiveInputs, PerpAccountState, PerpVenue, SizeSpec};

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    fn ctx() -> EvalContext {
        use policy_state::eval_context::RequestKind;
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

    fn perp_position(id: &str, size: u64, entry: &str) -> Position {
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
                collateral: vec![(usdc_ref(), U256::from(1_000_u64))],
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

    fn live_inputs(mark_str: &str) -> OpenPerpLiveInputs {
        OpenPerpLiveInputs {
            mark_price: live(Decimal::new(mark_str)),
            oracle_price: live(Decimal::new(mark_str)),
            funding_rate: live(Decimal::new("0")),
            available_oi: live(U256::from(u128::MAX)),
            max_leverage: live(Decimal::new("50")),
            initial_margin_bp: live(0),
            maintenance_bp: live(200),
            fee_taker_bp: live(5),
            fee_maker_bp: live(0),
            user_account_state: live(PerpAccountState {
                total_collateral_usd: U256::from(10_000_u64),
                used_margin_usd: U256::ZERO,
                free_margin_usd: U256::from(10_000_u64),
                open_positions: vec![],
            }),
        }
    }

    fn increase_action(
        id: &str,
        delta_amount: u64,
        mark_str: &str,
        add_coll: Option<u64>,
    ) -> IncreasePerpAction {
        IncreasePerpAction {
            venue: PerpVenue::GmxV2 {
                chain: ChainId::ethereum_mainnet(),
            },
            position_id: id.to_string(),
            size: SizeSpec::BaseAmount {
                amount: U256::from(delta_amount),
            },
            add_collateral: add_coll.map(|c| (usdc_ref(), U256::from(c))),
            slippage_bp: 50,
            live_inputs: live_inputs(mark_str),
        }
    }

    /// Increase 1 ETH @ mark 3000 onto an existing 1 ETH @ entry 3000 →
    /// new size = 2, new entry = (1×3000 + 1×3000) / 2 = 3000.
    #[test]
    fn increase_keeps_entry_when_mark_equals_old_entry() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(10_000));
        s.positions.push(perp_position("p1", 1, "3000"));
        let action = increase_action("p1", 1, "3000", None);
        let delta = action.apply(&s, &ctx()).unwrap();

        assert_eq!(delta.position_changes.len(), 1);
        match &delta.position_changes[0] {
            PositionChange::Update { id, patch } => {
                assert_eq!(id, "p1");
                let after: Position =
                    serde_json::from_value(patch.fields.get("after").unwrap().clone()).unwrap();
                if let PositionKind::PerpPosition(p) = &after.kind {
                    assert_eq!(p.size_base, U256::from(2_u64));
                    assert_eq!(p.entry_price.as_str(), "3000");
                } else {
                    panic!("expected PerpPosition");
                }
            }
            other => panic!("expected Update, got {other:?}"),
        }
        assert!(
            delta.token_changes.is_empty(),
            "no add_collateral → no debit"
        );
    }

    /// Weighted entry: 1 ETH @ 3000 + 1 ETH @ 3100 → entry = 3050.
    #[test]
    fn increase_weighted_entry_when_mark_higher() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(10_000));
        s.positions.push(perp_position("p1", 1, "3000"));
        let action = increase_action("p1", 1, "3100", None);
        let delta = action.apply(&s, &ctx()).unwrap();

        match &delta.position_changes[0] {
            PositionChange::Update { patch, .. } => {
                let after: Position =
                    serde_json::from_value(patch.fields.get("after").unwrap().clone()).unwrap();
                if let PositionKind::PerpPosition(p) = &after.kind {
                    assert_eq!(p.entry_price.as_str(), "3050");
                }
            }
            _ => panic!("expected Update"),
        }
    }

    /// `add_collateral` fires a debit.
    #[test]
    fn increase_with_extra_collateral_debits() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.tokens.insert(usdc_ref().key, make_holding(10_000));
        s.positions.push(perp_position("p1", 1, "3000"));
        let action = increase_action("p1", 1, "3000", Some(500));
        let delta = action.apply(&s, &ctx()).unwrap();
        assert_eq!(delta.token_changes.len(), 1);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { key, delta: d } => {
                assert_eq!(*key, usdc_ref().key);
                assert!(d.is_negative());
                assert_eq!(d.unsigned_abs().to_string(), "500");
            }
            _ => panic!("expected BalanceDelta"),
        }
    }

    /// Orderbook venue rejection.
    #[test]
    fn increase_orderbook_venue_rejected() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.positions.push(perp_position("p1", 1, "3000"));
        let mut action = increase_action("p1", 1, "3000", None);
        action.venue = PerpVenue::Hyperliquid {
            chain: ChainId::ethereum_mainnet(),
        };
        let err = action.apply(&s, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("orderbook venue")));
    }

    /// Zero delta size → Invariant.
    #[test]
    fn increase_zero_delta_rejected() {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        s.positions.push(perp_position("p1", 1, "3000"));
        let action = increase_action("p1", 0, "3000", None);
        let err = action.apply(&s, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("Δsize is zero")));
    }
}
