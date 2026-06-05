//! `SignIntentOrderAction` / `CancelIntentOrderAction` reducers —
//! EIP-712 intent flows (`UniswapX` / `CowSwap` / `1inch Fusion` / `Bebop`).
//! ## Off-chain sig handling
//! Intent orders are signed off-chain (EIP-712 typed-data signatures). They do
//! not move funds at the moment of signing — only when a filler/solver presents
//! them at the venue's settlement contract. We model the signing event as a
//! `PendingTx` carrying `AssetCommitment::PermitCap { token: sell, spender:
//! <reactor or settlement>, max_out: sell_amount }` so the wallet's `committed`
//! accounting (PDF §6.1) reflects the spend cap that the signature has
//! authorised, even though no on-chain allowance has been set yet.
//! This mirrors the `Erc20Permit` / `Permit2Sign` pattern in `effect/token.rs`
//! and lets the policy layer reason about intent-order spend caps with the
//! same `cap_for` aggregation that's already wired for permits.
//! ## Venue address binding
//! `IntentVenue` carries the reactor / settlement contract on each variant
//! (`UniswapX` reactor, `CowSwap` `GPv2` settlement, `1inch Fusion` + `Bebop`
//! are chain-bound but the actual filler is dynamic). For `OneInchFusion` and
//! `Bebop` we use `Address::ZERO` as the spender placeholder — the actual
//! filler is determined at settlement time, and the spend cap is policy-
//! relevant against any spender. Policy can match on the `VenueRef.name`
//! field of the `PendingTx.kind.OffchainLimitOrder.venue` slot to whitelist
//! fillers by venue family.
//! * `UniswapX` — <https://github.com/Uniswap/UniswapX>
//!   * `src/base/ReactorStructs.sol::OrderInfo`
//!   * `src/reactors/V2DutchOrderReactor.sol::execute`
//! * `CowSwap` — <https://github.com/cowprotocol/contracts>
//!   * `src/contracts/GPv2Settlement.sol::settle`
//!   * `src/contracts/libraries/GPv2Order.sol` — EIP-712 order struct
//! * 1inch Fusion — <https://docs.1inch.io/docs/fusion-swap/introduction>
//! * Bebop — <https://docs.bebop.xyz/>

use policy_state::pending::{
    AssetCommitment, OrderKind, PendingKind, PendingLifecycle, PendingStatus, PendingTx,
};
use policy_state::primitives::{Address, VenueRef};
use policy_state::{DataSource, EvalContext, PendingChange, StateDelta, WalletState};

use crate::action::amm::{
    CancelIntentOrderAction, IntentOrderKind, IntentVenue, PreSignIntentOrderAction,
    SettleIntentOrderAction, SignIntentOrderAction,
};
use crate::apply::Reducer;
use crate::error::ReducerResult;
use policy_state::delta::PendingRemoveReason;

/// Map the action-side `IntentOrderKind` to the state-side `OrderKind`. Both
/// enums carry the same `Dutch` / `Limit` / `Rfq` variants but live in
/// different crates, so the explicit projection keeps the layering clean.
const fn map_order_kind(kind: &IntentOrderKind) -> OrderKind {
    match kind {
        IntentOrderKind::Dutch => OrderKind::Dutch,
        IntentOrderKind::Limit => OrderKind::Limit,
        IntentOrderKind::Rfq => OrderKind::Rfq,
    }
}

/// Map the action-side `IntentVenue` to a `VenueRef` (state-side venue
/// identifier) and the spender address used for the `PermitCap` commitment.
/// For `UniswapX` / `CowSwap` the spender is the reactor / settlement
/// contract that pulls `sell` when filling. For `OneInchFusion` / `Bebop` the
/// resolver is chosen at settlement time, so we use `Address::ZERO` as a
/// "any spender" placeholder and rely on the venue name for policy matching.
fn project_venue(venue: &IntentVenue) -> (VenueRef, Address) {
    match venue {
        IntentVenue::UniswapX { chain, reactor } => (
            VenueRef {
                name: "uniswap_x".into(),
                chain: Some(chain.clone()),
            },
            *reactor,
        ),
        IntentVenue::CowSwap { chain, settlement } => (
            VenueRef {
                name: "cow_swap".into(),
                chain: Some(chain.clone()),
            },
            *settlement,
        ),
        IntentVenue::OneInchFusion { chain } => (
            VenueRef {
                name: "one_inch_fusion".into(),
                chain: Some(chain.clone()),
            },
            Address::ZERO,
        ),
        IntentVenue::Bebop { chain } => (
            VenueRef {
                name: "bebop".into(),
                chain: Some(chain.clone()),
            },
            Address::ZERO,
        ),
        // 1inch LOP v4: the embedding AggregationRouterV6 (verifying contract)
        // pulls `sell` from the maker on fill, so it is the spender.
        IntentVenue::OneInchLimitOrder {
            chain,
            verifying_contract,
        } => (
            VenueRef {
                name: "one_inch_limit_order".into(),
                chain: Some(chain.clone()),
            },
            *verifying_contract,
        ),
    }
}

/// Synthesize a stable `PendingTx.id` for a `SignIntentOrder`. Derived from
/// the deterministic parts of the action so re-evaluating the same `Action`
/// produces the same id (matches `effect/token.rs::pending_id_for_*`
/// convention).
fn pending_id_for_intent(
    venue_name: &str,
    sell_hex: &str,
    buy_hex: &str,
    sell_amount: &str,
    valid_until_unix: u64,
) -> String {
    format!("intent:{venue_name}:{sell_hex}:{buy_hex}:{sell_amount}:{valid_until_unix}")
}

impl Reducer for SignIntentOrderAction {
    fn apply(&self, _state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();
        let (venue_ref, spender) = project_venue(&self.venue);

        // Stable id components.
        let sell_hex = self
            .sell
            .key
            .contract()
            .map_or_else(|| "native".to_string(), |addr| format!("{addr:#x}"));
        let buy_hex = self
            .buy
            .key
            .contract()
            .map_or_else(|| "native".to_string(), |addr| format!("{addr:#x}"));
        let id = pending_id_for_intent(
            &venue_ref.name,
            &sell_hex,
            &buy_hex,
            &format!("{}", self.sell_amount),
            self.valid_until.as_unix(),
        );

        let pending = PendingTx {
            id,
            kind: PendingKind::OffchainLimitOrder {
                venue: venue_ref,
                sell: self.sell.clone(),
                buy: self.buy.clone(),
                sell_max: self.sell_amount,
                buy_min: self.buy_min,
                order_kind: map_order_kind(&self.order_kind),
            },
            commitment: AssetCommitment::PermitCap {
                token: self.sell.clone(),
                spender,
                max_out: self.sell_amount,
            },
            fill_effect: Box::new(StateDelta::new()),
            lifecycle: PendingLifecycle {
                status: PendingStatus::Active,
                valid_until: Some(self.valid_until),
                // Intent orders do not consume an EIP-712 permit nonce —
                // venue-side nonces (UniswapX reactor's per-order nonce,
                // CowSwap's order hash uniqueness) are tracked at the venue,
                // not the wallet's Permit2 / EIP-2612 namespaces.
                nonce: None,
                on_chain_tx: None,
                raw_status: None,
            },
            sync: DataSource::UserSupplied,
            signed_at: ctx.now,
            signature_payload: Vec::new(),
        };

        delta.pending_changes.push(PendingChange::Add {
            pending: Box::new(pending),
        });
        Ok(delta)
    }
}

impl Reducer for SettleIntentOrderAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        // Settlement can be submitted by a third-party filler, while the order's
        // swapper may be a different wallet. The semantic action is policy
        // visible, but wallet-state balance deltas require execution traces or
        // venue callback simulation. Do not invent a submitter debit here.
        Ok(StateDelta::new())
    }
}

impl Reducer for CancelIntentOrderAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();
        // Signature (if provided) is only an audit / authorisation artefact at
        // the reducer layer — actual sig verification is the
        // adapter/orchestrator's responsibility. Recording the cancellation
        // intent is sufficient for the wallet's `committed` accounting to
        // release the spend cap held by the prior `SignIntentOrder`.
        delta.pending_changes.push(PendingChange::Remove {
            id: self.order_hash.clone(),
            reason: PendingRemoveReason::Cancelled,
        });
        Ok(delta)
    }
}

impl Reducer for PreSignIntentOrderAction {
    fn apply(&self, _state: &WalletState, _ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let mut delta = StateDelta::new();
        // `setPreSignature(orderUid, signed)`:
        //   * signed=false → revoke a prior pre-signature; release any spend cap
        //     held under this order id (mirrors `CancelIntentOrder`).
        //   * signed=true  → mark the order tradable. The economic terms
        //     (sell/buy/amounts) are NOT in calldata — they live in the
        //     off-chain order keyed by the digest — so no `PermitCap` can be
        //     modelled here. Recording the intent is policy-visible (the lowered
        //     Cedar context carries `signed`); wallet-state balance deltas need
        //     the enriched order, so emit no state change rather than invent one.
        if !self.signed {
            delta.pending_changes.push(PendingChange::Remove {
                id: self.order_hash.clone(),
                reason: PendingRemoveReason::Cancelled,
            });
        }
        Ok(delta)
    }
}

// ===========================================================================
// Inline tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::amm::{IntentOrderKind, IntentVenue, SignIntentOrderLiveInputs};
    use policy_state::eval_context::RequestKind;
    use policy_state::live_field::{DataSource, LiveField};
    use policy_state::primitives::{Address, ChainId, Price, Time, U256};
    use policy_state::token::{TokenKey, TokenRef};
    use policy_state::wallet::WalletId;
    use std::str::FromStr;

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn later() -> Time {
        Time::from_unix(1_738_086_400)
    }

    fn ctx() -> EvalContext {
        EvalContext::new(ChainId::ethereum_mainnet(), now(), RequestKind::Transaction)
    }

    fn empty_state() -> WalletState {
        WalletState::new(WalletId::new(
            Address::from([0u8; 20]),
            [ChainId::ethereum_mainnet()],
        ))
    }

    fn usdc_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        }
    }

    fn weth_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap(),
            },
        }
    }

    fn live_inputs() -> SignIntentOrderLiveInputs {
        SignIntentOrderLiveInputs {
            expected_fill_price: LiveField::new(Price::zero(), DataSource::UserSupplied, now()),
            competing_orders: LiveField::new(0u32, DataSource::UserSupplied, now()),
        }
    }

    fn uniswap_x_reactor() -> Address {
        Address::from_str("0x00000011f84b9aa48e5f8aa8b9897600006289be").unwrap()
    }

    fn cow_settlement() -> Address {
        Address::from_str("0x9008d19f58aabd9ed0d60971565aa8510560ab41").unwrap()
    }

    fn recipient() -> Address {
        Address::from_str("0x000000000000000000000000000000000000beef").unwrap()
    }

    /// `UniswapX` Dutch order: `PendingTx` must carry `OffchainLimitOrder`
    /// kind + `PermitCap` commitment against the reactor address + `Dutch`
    /// `order_kind`.
    #[test]
    fn sign_uniswap_x_dutch_emits_pending_with_permit_cap_against_reactor() {
        let state = empty_state();
        let action = SignIntentOrderAction {
            venue: IntentVenue::UniswapX {
                chain: ChainId::ethereum_mainnet(),
                reactor: uniswap_x_reactor(),
            },
            sell: usdc_ref(),
            buy: weth_ref(),
            sell_amount: U256::from(1_000_000_000u64),
            buy_min: U256::from(300_000_000_000_000_000u64),
            order_kind: IntentOrderKind::Dutch,
            recipient: recipient(),
            valid_until: later(),
            live_inputs: live_inputs(),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert!(delta.token_changes.is_empty());
        assert_eq!(delta.pending_changes.len(), 1);

        let PendingChange::Add { pending } = &delta.pending_changes[0] else {
            panic!("expected PendingChange::Add");
        };
        // Commitment.
        match &pending.commitment {
            AssetCommitment::PermitCap {
                token,
                spender,
                max_out,
            } => {
                assert_eq!(token, &usdc_ref());
                assert_eq!(*spender, uniswap_x_reactor());
                assert_eq!(*max_out, U256::from(1_000_000_000u64));
            }
            other => panic!("expected PermitCap, got {other:?}"),
        }
        // Kind.
        match &pending.kind {
            PendingKind::OffchainLimitOrder {
                venue,
                sell,
                buy,
                sell_max,
                buy_min,
                order_kind,
            } => {
                assert_eq!(venue.name, "uniswap_x");
                assert_eq!(venue.chain, Some(ChainId::ethereum_mainnet()));
                assert_eq!(sell, &usdc_ref());
                assert_eq!(buy, &weth_ref());
                assert_eq!(*sell_max, U256::from(1_000_000_000u64));
                assert_eq!(*buy_min, U256::from(300_000_000_000_000_000u64));
                assert_eq!(*order_kind, OrderKind::Dutch);
            }
            other => panic!("expected OffchainLimitOrder, got {other:?}"),
        }
        // Lifecycle.
        assert_eq!(pending.lifecycle.status, PendingStatus::Active);
        assert_eq!(pending.lifecycle.valid_until, Some(later()));
        assert!(pending.lifecycle.nonce.is_none());
        assert_eq!(pending.signed_at, now());
    }

    /// `CowSwap` limit order: venue spender is the settlement contract.
    #[test]
    fn sign_cow_swap_limit_uses_settlement_contract_as_spender() {
        let state = empty_state();
        let action = SignIntentOrderAction {
            venue: IntentVenue::CowSwap {
                chain: ChainId::ethereum_mainnet(),
                settlement: cow_settlement(),
            },
            sell: usdc_ref(),
            buy: weth_ref(),
            sell_amount: U256::from(500_000_000u64),
            buy_min: U256::from(100_000_000_000_000_000u64),
            order_kind: IntentOrderKind::Limit,
            recipient: recipient(),
            valid_until: later(),
            live_inputs: live_inputs(),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        let PendingChange::Add { pending } = &delta.pending_changes[0] else {
            panic!("expected PendingChange::Add");
        };
        match &pending.commitment {
            AssetCommitment::PermitCap { spender, .. } => {
                assert_eq!(*spender, cow_settlement());
            }
            other => panic!("expected PermitCap, got {other:?}"),
        }
        match &pending.kind {
            PendingKind::OffchainLimitOrder {
                venue, order_kind, ..
            } => {
                assert_eq!(venue.name, "cow_swap");
                assert_eq!(*order_kind, OrderKind::Limit);
            }
            other => panic!("expected OffchainLimitOrder, got {other:?}"),
        }
    }

    /// 1inch Fusion: resolver chosen at settlement time → spender placeholder
    /// is the zero address; venue name still routes policy.
    #[test]
    fn sign_fusion_rfq_uses_zero_spender_placeholder() {
        let state = empty_state();
        let action = SignIntentOrderAction {
            venue: IntentVenue::OneInchFusion {
                chain: ChainId::ethereum_mainnet(),
            },
            sell: usdc_ref(),
            buy: weth_ref(),
            sell_amount: U256::from(1u64),
            buy_min: U256::from(1u64),
            order_kind: IntentOrderKind::Rfq,
            recipient: recipient(),
            valid_until: later(),
            live_inputs: live_inputs(),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        let PendingChange::Add { pending } = &delta.pending_changes[0] else {
            panic!("expected PendingChange::Add");
        };
        match &pending.commitment {
            AssetCommitment::PermitCap { spender, .. } => {
                assert_eq!(*spender, Address::ZERO);
            }
            other => panic!("expected PermitCap, got {other:?}"),
        }
        match &pending.kind {
            PendingKind::OffchainLimitOrder {
                venue, order_kind, ..
            } => {
                assert_eq!(venue.name, "one_inch_fusion");
                assert_eq!(*order_kind, OrderKind::Rfq);
            }
            other => panic!("expected OffchainLimitOrder, got {other:?}"),
        }
    }

    /// Bebop RFQ: same zero-spender pattern as Fusion.
    #[test]
    fn sign_bebop_rfq_uses_zero_spender_placeholder() {
        let state = empty_state();
        let action = SignIntentOrderAction {
            venue: IntentVenue::Bebop {
                chain: ChainId::ethereum_mainnet(),
            },
            sell: usdc_ref(),
            buy: weth_ref(),
            sell_amount: U256::from(1u64),
            buy_min: U256::from(1u64),
            order_kind: IntentOrderKind::Rfq,
            recipient: recipient(),
            valid_until: later(),
            live_inputs: live_inputs(),
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        let PendingChange::Add { pending } = &delta.pending_changes[0] else {
            panic!("expected PendingChange::Add");
        };
        match &pending.commitment {
            AssetCommitment::PermitCap { spender, .. } => {
                assert_eq!(*spender, Address::ZERO);
            }
            other => panic!("expected PermitCap, got {other:?}"),
        }
        match &pending.kind {
            PendingKind::OffchainLimitOrder { venue, .. } => {
                assert_eq!(venue.name, "bebop");
            }
            other => panic!("expected OffchainLimitOrder, got {other:?}"),
        }
    }

    /// Cancel emits `PendingChange::Remove` with the `order_hash` +
    /// `Cancelled` reason, no token changes.
    #[test]
    fn cancel_emits_remove_with_cancelled_reason() {
        let state = empty_state();
        let action = CancelIntentOrderAction {
            venue: IntentVenue::UniswapX {
                chain: ChainId::ethereum_mainnet(),
                reactor: uniswap_x_reactor(),
            },
            order_hash: format!("0x{}", "ab".repeat(32)),
            signature: None,
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert!(delta.token_changes.is_empty());
        assert_eq!(delta.pending_changes.len(), 1);
        match &delta.pending_changes[0] {
            PendingChange::Remove { id, reason } => {
                assert_eq!(*id, format!("0x{}", "ab".repeat(32)));
                assert_eq!(*reason, PendingRemoveReason::Cancelled);
            }
            other => panic!("expected Remove, got {other:?}"),
        }
    }

    /// `setPreSignature(signed=false)` revokes → `PendingChange::Remove` with
    /// the `order_hash` + `Cancelled` reason (mirrors `CancelIntentOrder`).
    #[test]
    fn presign_signed_false_emits_remove() {
        let state = empty_state();
        let action = PreSignIntentOrderAction {
            venue: IntentVenue::CowSwap {
                chain: ChainId::ethereum_mainnet(),
                settlement: cow_settlement(),
            },
            order_hash: format!("0x{}", "cd".repeat(28)),
            signed: false,
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert!(delta.token_changes.is_empty());
        assert_eq!(delta.pending_changes.len(), 1);
        match &delta.pending_changes[0] {
            PendingChange::Remove { id, reason } => {
                assert_eq!(*id, format!("0x{}", "cd".repeat(28)));
                assert_eq!(*reason, PendingRemoveReason::Cancelled);
            }
            other => panic!("expected Remove, got {other:?}"),
        }
    }

    /// `setPreSignature(signed=true)` commits to an order whose terms are NOT
    /// in calldata → no state delta (we do not fabricate a spend cap).
    #[test]
    fn presign_signed_true_emits_no_state_change() {
        let state = empty_state();
        let action = PreSignIntentOrderAction {
            venue: IntentVenue::CowSwap {
                chain: ChainId::ethereum_mainnet(),
                settlement: cow_settlement(),
            },
            order_hash: format!("0x{}", "ef".repeat(28)),
            signed: true,
        };
        let delta = action.apply(&state, &ctx()).unwrap();
        assert!(delta.token_changes.is_empty());
        assert!(delta.pending_changes.is_empty());
    }
}
