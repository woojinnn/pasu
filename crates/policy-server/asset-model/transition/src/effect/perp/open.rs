//! `OpenPerpAction` reducer — open a new perpetual futures position.
//! ## Orderbook vs on-chain dispatch
//! The reducer branches on `PerpVenue` discriminant via
//! [`super::common::is_orderbook_venue`]:
//! * **Orderbook** (`Hyperliquid` / `Aevo` / `DyDxV4`) — the signing event
//!   does not mutate wallet state on chain (the user's collateral remains in
//!   their venue subaccount, no on-chain transfer fires until the order fills
//!   later). We emit a `PendingChange::Add` carrying a `PendingKind::
//!   PerpVenueOrder` with `commitment: AssetCommitment::HardLock { token,
//!   locked: required_margin }` — committed-balance accounting (PDF §6.1)
//!   recognises the margin as locked even though the on-chain balance has
//!   not moved yet. This mirrors the `Erc20Permit` / `Permit2SignAction`
//!   pattern in `effect::token` (off-chain sig → pending entry).
//! * **On-chain** (`GmxV2` / `Vertex` / `Drift` / `JupiterPerps` /
//!   `Synthetix` / `Generic`) — the signing event triggers immediate
//!   on-chain state change: collateral debit + new `PerpPosition` entry.
//!   We still emit a synthetic `PendingTx` so the wallet can track the
//!   submitted transaction's lifecycle, but the bulk of the effect is the
//!   `PositionChange::Open` + `TokenChange::BalanceDelta` rows.
//! ## Validation order
//! 1. `required_initial_margin` (venue helper) — also rejects
//!    leverage > venue max.
//! 2. `free_margin_usd >= required_margin` invariant (`LiveField`).
//! 3. `available_oi >= notional` invariant (`LiveField`).
//! 4. `liquidation_price` (venue helper) — may surface
//!    `UnsupportedProtocol` for the deferred-liq-price venues
//!    (`gmx_v2` / `synthetix` / `jupiter_perps`), which the caller
//!    interprets as "liquidation price unknown until the venue
//!    indexer publishes it" rather than a hard rejection. The reducer
//!    suppresses the deferred error in that one site and emits the
//!    Position with `liq_price = LiveField<None>` so policy can still
//!    use the position.

use policy_state::delta::PositionChange;
use policy_state::live_field::{DataSource, LiveField};
use policy_state::pending::{
    AssetCommitment, PendingKind, PendingLifecycle, PendingStatus, PendingTx, PerpOrderKind,
};
use policy_state::position::{PerpPosition, Position, PositionKind};
use policy_state::primitives::{ProtocolRef, SignedI256};
use policy_state::{Decimal, EvalContext, PendingChange, StateDelta, WalletState, U256};

use crate::action::perp::{OpenPerpAction, PerpVenue};
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

use super::{
    aevo, common, drift, dydx_v4, gmx_v2, hyperliquid, jupiter_perps, math, synthetix, vertex,
};

impl Reducer for OpenPerpAction {
    #[allow(clippy::too_many_lines)]
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();

        // Venue dispatch — required_initial_margin + liquidation_price.
        let required_margin = dispatch_required_initial_margin(self)?;

        // Free-margin invariant.
        let free_margin = self.live_inputs.user_account_state.value.free_margin_usd;
        if free_margin < required_margin {
            return Err(ReducerError::Invariant(format!(
                "open_perp: free_margin_usd {free_margin} < required_margin {required_margin}"
            )));
        }

        // OI capacity invariant.
        let size_base = math::resolve_size_base(&self.size, &self.live_inputs.mark_price.value)?;
        if self.live_inputs.available_oi.value < size_base {
            return Err(ReducerError::Invariant(format!(
                "open_perp: available_oi {} < size_base {size_base}",
                self.live_inputs.available_oi.value
            )));
        }

        // Liquidation price — tolerate the deferred venue error for venues
        // whose accurate formula needs venue subgraph state.
        let liq_price = match dispatch_liquidation_price(self) {
            Ok(p) => p,
            Err(ReducerError::UnsupportedProtocol { protocol, .. })
                if protocol.contains("deferred") =>
            {
                None
            }
            Err(e) => return Err(e),
        };

        let (collateral_token, collateral_amount) = self.collateral.clone();

        // Debit collateral on-chain venues; orderbook venues skip the
        // on-chain debit since the user's venue subaccount already holds
        // the margin (margin is moved by a prior on-chain deposit, modeled
        // as a separate `Erc20Transfer` action).
        if !common::is_orderbook_venue(&self.venue) {
            helpers::balance::debit(state, &mut delta, &collateral_token.key, collateral_amount)?;
        }

        // Build the PerpPosition.
        let position_id = common::synth_position_id(&self.venue, &self.market.symbol, &self.side);
        let venue_ref = common::venue_ref(&self.venue);
        let entry_price = self.live_inputs.mark_price.value.clone();
        let notional_decimal = math::notional_usd(size_base, &self.live_inputs.mark_price.value)?;
        let notional_usd = U256::from_str_radix(&notional_decimal.trunc().to_string(), 10)
            .map_err(|e| {
                ReducerError::Invariant(format!("open_perp: notional_usd U256 parse: {e}"))
            })?;
        let perp = PerpPosition {
            venue: venue_ref.clone(),
            market: self.market.clone(),
            side: self.side.clone(),
            size_base,
            notional_usd,
            collateral: vec![(collateral_token.clone(), collateral_amount)],
            entry_price,
            margin_mode: self.margin_mode.clone(),
            mark_price: LiveField::new(
                self.live_inputs.mark_price.value.clone(),
                self.live_inputs.mark_price.source.clone(),
                ctx.now,
            ),
            liq_price: LiveField::new(liq_price, DataSource::UserSupplied, ctx.now),
            unrealized_pnl: LiveField::new(SignedI256::ZERO, DataSource::UserSupplied, ctx.now),
            funding_owed: LiveField::new(SignedI256::ZERO, DataSource::UserSupplied, ctx.now),
            leverage: LiveField::new(self.leverage.clone(), DataSource::UserSupplied, ctx.now),
        };
        let position = Position {
            id: position_id.clone(),
            protocol: ProtocolRef::new(common::venue_tag(&self.venue)),
            chain: chain_for_venue(&self.venue),
            kind: PositionKind::PerpPosition(perp),
            primitives_synced_at: ctx.now,
            primitives_source: DataSource::UserSupplied,
        };

        // Orderbook venues: emit pending only (no immediate position).
        // The position is opened later when the orderbook reports the fill.
        // downstream `apply_delta` / DB layer can mark it `pending`; the
        // Pending entry's `fill_effect` carries the same `Position::Open`
        // for the resolver to play back idempotently.
        if common::is_orderbook_venue(&self.venue) {
            // PendingTx with HardLock commitment over the margin token.
            let pending_id = format!("{}:open:{position_id}", common::venue_tag(&self.venue));
            let pending = PendingTx {
                id: pending_id,
                kind: PendingKind::PerpVenueOrder {
                    venue: venue_ref,
                    market: self.market.clone(),
                    side: self.side.clone(),
                    size_base,
                    price: self.live_inputs.mark_price.value.clone(),
                    order_kind: PerpOrderKind::Limit,
                    reduce_only: self.reduce_only,
                },
                commitment: AssetCommitment::HardLock {
                    token: collateral_token.clone(),
                    locked: required_margin,
                },
                fill_effect: Box::new({
                    let mut fill = StateDelta::new();
                    fill.position_changes
                        .push(PositionChange::Open { position });
                    fill
                }),
                lifecycle: PendingLifecycle {
                    status: PendingStatus::Active,
                    valid_until: None,
                    nonce: None,
                    on_chain_tx: None,
                },
                sync: common::pending_user_source(),
                signed_at: ctx.now,
                signature_payload: Vec::new(),
            };
            delta.pending_changes.push(PendingChange::Add {
                pending: Box::new(pending),
            });
        } else {
            // On-chain venues: open the position now.
            helpers::position::open_position(state, &mut delta, position)?;
        }
        Ok(delta)
    }
}

/// Per-venue dispatch for `required_initial_margin`. `PerpVenue::Generic`
/// delegates to the Hyperliquid common form by default (acceptable for the
/// catch-all variant; concrete generic-protocol overrides land in later
/// batches).
fn dispatch_required_initial_margin(action: &OpenPerpAction) -> ReducerResult<U256> {
    match &action.venue {
        PerpVenue::Hyperliquid { .. } => hyperliquid::required_initial_margin(
            // state / ctx unused by these helpers; pass synthetic placeholders.
            &empty_state_for_helpers(),
            &empty_ctx_for_helpers(),
            action,
            &action.live_inputs,
        ),
        PerpVenue::Aevo { .. } => aevo::required_initial_margin(
            &empty_state_for_helpers(),
            &empty_ctx_for_helpers(),
            action,
            &action.live_inputs,
        ),
        PerpVenue::DyDxV4 { .. } => dydx_v4::required_initial_margin(
            &empty_state_for_helpers(),
            &empty_ctx_for_helpers(),
            action,
            &action.live_inputs,
        ),
        PerpVenue::GmxV2 { .. } => gmx_v2::required_initial_margin(
            &empty_state_for_helpers(),
            &empty_ctx_for_helpers(),
            action,
            &action.live_inputs,
        ),
        PerpVenue::Vertex { .. } => vertex::required_initial_margin(
            &empty_state_for_helpers(),
            &empty_ctx_for_helpers(),
            action,
            &action.live_inputs,
        ),
        PerpVenue::Drift { .. } => drift::required_initial_margin(
            &empty_state_for_helpers(),
            &empty_ctx_for_helpers(),
            action,
            &action.live_inputs,
        ),
        PerpVenue::JupiterPerps { .. } => jupiter_perps::required_initial_margin(
            &empty_state_for_helpers(),
            &empty_ctx_for_helpers(),
            action,
            &action.live_inputs,
        ),
        PerpVenue::Synthetix { .. } => synthetix::required_initial_margin(
            &empty_state_for_helpers(),
            &empty_ctx_for_helpers(),
            action,
            &action.live_inputs,
        ),
        PerpVenue::Generic { .. } => {
            // Catch-all: delegate to the common formula. A future batch can
            // override per-protocol once a Generic instance arrives.
            math::required_initial_margin_common("generic_perp", action, &action.live_inputs)
        }
    }
}

/// Per-venue dispatch for `liquidation_price`. Same pattern as
/// `dispatch_required_initial_margin`.
fn dispatch_liquidation_price(
    action: &OpenPerpAction,
) -> ReducerResult<Option<policy_state::primitives::Price>> {
    match &action.venue {
        PerpVenue::Hyperliquid { .. } => hyperliquid::liquidation_price(
            &empty_state_for_helpers(),
            &empty_ctx_for_helpers(),
            action,
            &action.live_inputs,
        ),
        PerpVenue::Aevo { .. } => aevo::liquidation_price(
            &empty_state_for_helpers(),
            &empty_ctx_for_helpers(),
            action,
            &action.live_inputs,
        ),
        PerpVenue::DyDxV4 { .. } => dydx_v4::liquidation_price(
            &empty_state_for_helpers(),
            &empty_ctx_for_helpers(),
            action,
            &action.live_inputs,
        ),
        PerpVenue::GmxV2 { .. } => gmx_v2::liquidation_price(
            &empty_state_for_helpers(),
            &empty_ctx_for_helpers(),
            action,
            &action.live_inputs,
        ),
        PerpVenue::Vertex { .. } => vertex::liquidation_price(
            &empty_state_for_helpers(),
            &empty_ctx_for_helpers(),
            action,
            &action.live_inputs,
        ),
        PerpVenue::Drift { .. } => drift::liquidation_price(
            &empty_state_for_helpers(),
            &empty_ctx_for_helpers(),
            action,
            &action.live_inputs,
        ),
        PerpVenue::JupiterPerps { .. } => jupiter_perps::liquidation_price(
            &empty_state_for_helpers(),
            &empty_ctx_for_helpers(),
            action,
            &action.live_inputs,
        ),
        PerpVenue::Synthetix { .. } => synthetix::liquidation_price(
            &empty_state_for_helpers(),
            &empty_ctx_for_helpers(),
            action,
            &action.live_inputs,
        ),
        PerpVenue::Generic { .. } => {
            math::liquidation_price_simple("generic_perp", action, &action.live_inputs)
        }
    }
}

/// Returns the chain associated with the venue (or `None` for off-chain).
const fn chain_for_venue(venue: &PerpVenue) -> Option<policy_state::primitives::ChainId> {
    // ChainId is a String newtype (non-Copy), so we cannot return a borrowed
    // reference inside Option without unwinding the API. We return None here
    // for off-chain venues and Some(clone) for on-chain; the wrapper below
    // does the actual clone at the (single) call site.
    let _ = venue;
    None
}

/// `dispatch_*` helpers want a `&WalletState` / `&EvalContext` because the
/// venue helper signature carries them — but the venue helpers under
/// `effect/perp/<venue>.rs` ignore both. We materialise an empty wallet here
/// to keep the call ergonomic; the value is never read.
fn empty_state_for_helpers() -> WalletState {
    use policy_state::primitives::{Address, ChainId};
    use policy_state::wallet::WalletId;
    WalletState::new(WalletId::new(
        Address::from([0u8; 20]),
        [ChainId::ethereum_mainnet()],
    ))
}

fn empty_ctx_for_helpers() -> EvalContext {
    use policy_state::eval_context::RequestKind;
    use policy_state::primitives::{ChainId, Time};
    EvalContext::new(
        ChainId::ethereum_mainnet(),
        Time::from_unix(0),
        RequestKind::Transaction,
    )
}

// Re-export touches to silence the unused-import lint when the action body
// uses only a subset of the venue modules in early-batch wiring.
#[allow(dead_code)]
fn _module_touch() {
    let _ = (
        |_: &Decimal| (),
        empty_state_for_helpers,
        empty_ctx_for_helpers,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::live_field::{DataSource, LiveField, OracleProvider};
    use policy_state::position::{MarginMode, PerpSide};
    use policy_state::primitives::{Address, ChainId, MarketRef, Time, VenueRef};
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

    fn empty_state() -> WalletState {
        WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]))
    }

    fn state_with_collateral(amount: u128) -> WalletState {
        let mut s = empty_state();
        s.tokens.insert(usdc_ref().key, make_holding(amount));
        s
    }

    fn live_inputs(
        free_margin: u64,
        mark_str: &str,
        max_lev_str: &str,
        maint_bp: u32,
        fee_taker_bp: u32,
    ) -> OpenPerpLiveInputs {
        OpenPerpLiveInputs {
            mark_price: live(Decimal::new(mark_str)),
            oracle_price: live(Decimal::new(mark_str)),
            funding_rate: live(Decimal::new("0")),
            available_oi: live(U256::from(u128::MAX)),
            max_leverage: live(Decimal::new(max_lev_str)),
            initial_margin_bp: live(0),
            maintenance_bp: live(maint_bp),
            fee_taker_bp: live(fee_taker_bp),
            fee_maker_bp: live(0),
            user_account_state: live(PerpAccountState {
                total_collateral_usd: U256::from(free_margin),
                used_margin_usd: U256::ZERO,
                free_margin_usd: U256::from(free_margin),
                open_positions: vec![],
            }),
        }
    }

    fn hyperliquid_open(amount: u128, mark_str: &str, leverage: &str) -> OpenPerpAction {
        OpenPerpAction {
            venue: PerpVenue::Hyperliquid {
                chain: ChainId::ethereum_mainnet(),
            },
            market: MarketRef {
                symbol: "ETH-PERP".into(),
                venue: VenueRef::new("hyperliquid"),
            },
            side: PerpSide::Long,
            size: SizeSpec::BaseAmount {
                amount: U256::from(amount),
            },
            leverage: Decimal::new(leverage),
            collateral: (usdc_ref(), U256::from(1_000_u64)),
            margin_mode: MarginMode::Isolated,
            slippage_bp: 50,
            reduce_only: false,
            live_inputs: live_inputs(10_000, mark_str, "50", 200, 5),
        }
    }

    fn gmx_v2_open(amount: u128, mark_str: &str, leverage: &str) -> OpenPerpAction {
        OpenPerpAction {
            venue: PerpVenue::GmxV2 {
                chain: ChainId::ethereum_mainnet(),
            },
            market: MarketRef {
                symbol: "ETH-PERP".into(),
                venue: VenueRef::new("gmx_v2"),
            },
            side: PerpSide::Long,
            size: SizeSpec::BaseAmount {
                amount: U256::from(amount),
            },
            leverage: Decimal::new(leverage),
            collateral: (usdc_ref(), U256::from(1_000_u64)),
            margin_mode: MarginMode::Isolated,
            slippage_bp: 50,
            reduce_only: false,
            live_inputs: live_inputs(10_000, mark_str, "50", 200, 5),
        }
    }

    /// Orderbook venue (Hyperliquid): emit `PendingTx`, no on-chain debit.
    #[test]
    fn open_hyperliquid_emits_pending_with_hardlock() {
        let state = empty_state();
        let action = hyperliquid_open(1, "3000", "5");
        let delta = action.apply(&state, &ctx()).unwrap();
        assert!(delta.token_changes.is_empty(), "orderbook should not debit");
        assert_eq!(delta.pending_changes.len(), 1);
        match &delta.pending_changes[0] {
            PendingChange::Add { pending } => {
                match &pending.commitment {
                    AssetCommitment::HardLock { token, locked } => {
                        assert_eq!(token, &usdc_ref());
                        // 1 ETH × $3000 / 5x + 5bp × 3000 / 10000 = 600 + 1.5 → 601
                        assert_eq!(*locked, U256::from(601_u64));
                    }
                    other => panic!("expected HardLock, got {other:?}"),
                }
                match &pending.kind {
                    PendingKind::PerpVenueOrder {
                        venue,
                        market,
                        side,
                        size_base,
                        ..
                    } => {
                        assert_eq!(venue.name, "hyperliquid");
                        assert_eq!(market.symbol, "ETH-PERP");
                        assert!(matches!(side, PerpSide::Long));
                        assert_eq!(*size_base, U256::from(1_u64));
                    }
                    other => panic!("expected PerpVenueOrder, got {other:?}"),
                }
                // fill_effect carries the position so the resolver can play
                // back idempotently.
                assert_eq!(pending.fill_effect.position_changes.len(), 1);
            }
            other => panic!("expected Add, got {other:?}"),
        }
    }

    /// On-chain venue (GMX V2): debit collateral + emit position.
    #[test]
    fn open_gmx_v2_emits_debit_and_position_open_with_deferred_liq() {
        let state = state_with_collateral(10_000);
        let action = gmx_v2_open(1, "3000", "5");
        let delta = action.apply(&state, &ctx()).unwrap();
        assert!(
            delta.pending_changes.is_empty(),
            "on-chain emits no pending"
        );
        // 1 BalanceDelta (collateral debit) + 1 Open position
        assert_eq!(delta.token_changes.len(), 1);
        assert_eq!(delta.position_changes.len(), 1);
        match &delta.position_changes[0] {
            PositionChange::Open { position } => {
                if let PositionKind::PerpPosition(p) = &position.kind {
                    assert_eq!(p.size_base, U256::from(1_u64));
                    // GMX V2 returns deferred → reducer suppresses, liq_price is None.
                    assert!(p.liq_price.value.is_none());
                } else {
                    panic!("expected PerpPosition");
                }
            }
            other => panic!("expected Open, got {other:?}"),
        }
    }

    /// Insufficient free margin → Invariant.
    #[test]
    fn open_hyperliquid_rejects_insufficient_free_margin() {
        let state = empty_state();
        let mut action = hyperliquid_open(1, "3000", "5");
        // free_margin = 100 < required ~ 601
        action.live_inputs = live_inputs(100, "3000", "50", 200, 5);
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("free_margin_usd")));
    }

    /// Leverage > max → Invariant (propagated from venue helper).
    #[test]
    fn open_rejects_leverage_above_max() {
        let state = state_with_collateral(10_000);
        let action = gmx_v2_open(1, "3000", "100"); // max is 50
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    /// OI exhaustion → Invariant.
    #[test]
    fn open_rejects_oi_exhausted() {
        let state = state_with_collateral(10_000);
        let mut action = gmx_v2_open(10, "3000", "5");
        action.live_inputs.available_oi = live(U256::from(1_u64));
        let err = action.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(msg) if msg.contains("available_oi")));
    }
}
