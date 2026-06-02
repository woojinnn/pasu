//! `LaunchpadAction` reducers — `Commit` / `ClaimAllocation` / `ClaimVested` /
//! `Refund` / `WithdrawCommit`.
//! ## Lifecycle (spec §8)
//! A launchpad sale (`CoinList` / `Buidlpad` / `Echo` / `Fjord` / `DAO Maker` / etc.)
//! flows through:
//! 1. **`Commit`** — user deposits a `pay_token` (USDC / native) into the
//!    sale contract. We `debit` the wallet and open a
//!    [`Position::LaunchpadAllocation`](policy_state::position::LaunchpadAllocation)
//!    with `paid = [(pay_token, amount)]`, `allocated = (placeholder_token, 0)`
//!    (the allocation is announced post-sale), and the optional
//!    `vest_schedule` taken from the live sale state.
//! 2. **`ClaimAllocation`** — sale is over; the platform tells us the final
//!    allocated `(TokenRef, amount)`. We `credit` the allocation token and
//!    update the position's `allocated` + `claimed += allocated`. Any
//!    `refund_due` is also credited (oversubscription).
//! 3. **`ClaimVested`** — for sales with a `VestSchedule`, the user
//!    periodically claims the `claimable_now` slice. We `credit` and
//!    bump `position.claimed`.
//! 4. **`Refund`** — refund flow (failed sale / oversubscription cancel).
//!    `credit` the refund amount on the refund token; the position is
//!    `Close`-d so downstream views can treat the allocation as fully
//!    settled.
//! 5. **`WithdrawCommit`** — pre-sale (sale still active) withdrawal
//!    cancelling part or all of a prior `Commit`. `credit` the
//!    `pay_token`. If the full commit is withdrawn the position is
//!    `Close`-d, otherwise it is `Update`-d with reduced `paid`.
//! ## State invariants enforced
//! * `Commit` rejects an inactive sale.
//! * `Commit` rejects when `user_committed + amount > user_cap`.
//! * `Commit` rejects when `total_committed + amount > hard_cap` if a hard
//!   cap is present.
//! * `Commit` rejects when `ctx.now` is outside `sale_window`.
//! * `ClaimAllocation` rejects when `is_claimable.value` is false.
//! * `ClaimVested` requires the referenced [`Position`] to be either a
//!   [`PositionKind::LaunchpadAllocation`] or
//!   [`PositionKind::VestingSchedule`].
//! * `WithdrawCommit` rejects when the sale is no longer `is_active`
//!   (post-sale withdrawal is the refund path, not this one).
//! * `WithdrawCommit` rejects when `live_inputs.withdrawable` < requested
//!   amount.
//! ## Position id convention
//! All launchpad-flow actions (`Commit` / `ClaimAllocation` / `Refund` /
//! `WithdrawCommit`) derive a deterministic id of shape
//! `launchpad:{platform.name}:{sale_id}:{recipient_hex}`. `ClaimVested`
//! uses the caller-supplied `position_id` directly (the action body
//! covers both `LaunchpadAllocation` and `VestingSchedule` positions
//! so it cannot synthesise the id).

use policy_state::delta::PositionPatch;
use policy_state::position::{
    LaunchpadAllocation, Position, PositionId, PositionKind, VestCurve, VestSchedule,
    VestingSchedule,
};
use policy_state::primitives::{Address, Time, U256};
use policy_state::{DataSource, EvalContext, PositionChange, StateDelta, WalletState};
use serde_json::json;

use crate::action::launchpad::{
    ClaimAllocationAction, ClaimVestedAction, CommitAction, LaunchpadAction, RefundAction,
    SaleState, WithdrawCommitAction,
};
use crate::apply::Reducer;
use crate::error::{ReducerError, ReducerResult};
use crate::helpers;

impl Reducer for LaunchpadAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        match self {
            Self::Commit(a) => a.apply(state, ctx),
            Self::ClaimAllocation(a) => a.apply(state, ctx),
            Self::ClaimVested(a) => a.apply(state, ctx),
            Self::Refund(a) => a.apply(state, ctx),
            Self::WithdrawCommit(a) => a.apply(state, ctx),
        }
    }
}

// ---------------------------------------------------------------------------
// Shared helpers (private to this file — `helpers/*` is read-only).
// ---------------------------------------------------------------------------

/// Deterministic position id for a launchpad allocation. Combines
/// `(platform.name, sale_id, recipient_hex)` so the same caller invoking
/// `Commit` / `ClaimAllocation` / `Refund` against the same sale always
/// converges to a single position.
fn launchpad_position_id(platform_name: &str, sale_id: &str, recipient: Address) -> String {
    format!("launchpad:{platform_name}:{sale_id}:{recipient:#x}")
}

/// Effective position lookup that also considers `PositionChange::Open`
/// entries already queued on `delta` (mirrors `helpers::position` private
/// `effective_position` but specialised to launchpad/vesting kinds and
/// inlined here so we don't have to widen the helpers crate's API).
fn effective_launchpad_position<'a>(
    state: &'a WalletState,
    delta: &'a StateDelta,
    id: &PositionId,
) -> Option<&'a Position> {
    for change in delta.position_changes.iter().rev() {
        match change {
            PositionChange::Open { position } if &position.id == id => return Some(position),
            PositionChange::Close { id: closed_id } if closed_id == id => return None,
            _ => {}
        }
    }
    state.positions.iter().find(|p| &p.id == id)
}

/// Push a `PositionChange::Update { id, patch = { "after": <new_position> } }`
/// following the same snapshot-style convention used by `helpers::position`.
fn push_position_update(delta: &mut StateDelta, new_position: &Position) {
    let after = serde_json::to_value(new_position).unwrap_or(serde_json::Value::Null);
    delta.position_changes.push(PositionChange::Update {
        id: new_position.id.clone(),
        patch: PositionPatch {
            fields: json!({ "after": after }),
        },
    });
}

/// Saturating `U256` subtraction helper. We do not propagate `Option<U256>`
/// because all call sites have already checked for underflow.
const fn saturating_sub(a: U256, b: U256) -> U256 {
    a.saturating_sub(b)
}

// ---------------------------------------------------------------------------
// Commit
// ---------------------------------------------------------------------------

impl Reducer for CommitAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let sale = &self.live_inputs.sale_state.value;

        // ---- 1. Sale activity + window checks --------------------------
        if !sale.is_active {
            return Err(ReducerError::Invariant(format!(
                "launchpad commit rejected: sale {} is not active",
                self.sale_id
            )));
        }
        let (start, end) = sale.sale_window;
        if ctx.now < start || ctx.now > end {
            return Err(ReducerError::Invariant(format!(
                "launchpad commit on sale {} outside sale_window ({}..={})",
                self.sale_id,
                start.as_unix(),
                end.as_unix()
            )));
        }

        // ---- 2. Per-user cap check -------------------------------------
        let user_cap = self.live_inputs.user_cap.value;
        let user_committed = self.live_inputs.user_committed.value;
        let new_user_total = user_committed.checked_add(self.amount).ok_or_else(|| {
            ReducerError::Invariant(format!(
                "launchpad commit overflow on sale {}",
                self.sale_id
            ))
        })?;
        if new_user_total > user_cap {
            return Err(ReducerError::Invariant(format!(
                "launchpad commit on sale {} exceeds user cap ({} > {})",
                self.sale_id, new_user_total, user_cap
            )));
        }

        // ---- 3. Hard cap check (if present) ----------------------------
        if let Some(hard) = sale.hard_cap {
            let new_total = sale
                .total_committed
                .checked_add(self.amount)
                .ok_or_else(|| {
                    ReducerError::Invariant(format!(
                        "launchpad commit overflow on sale {} total_committed",
                        self.sale_id
                    ))
                })?;
            if new_total > hard {
                return Err(ReducerError::Invariant(format!(
                    "launchpad commit on sale {} exceeds hard cap ({} > {})",
                    self.sale_id, new_total, hard
                )));
            }
        }

        // ---- 4. Debit pay_token ---------------------------------------
        let mut delta = StateDelta::new();
        helpers::balance::debit(state, &mut delta, &self.pay_token.key, self.amount)?;

        // ---- 5. Open or augment LaunchpadAllocation position -----------
        let id = launchpad_position_id(&self.platform.name, &self.sale_id, self.recipient);
        let chain = Some(self.pay_token.key.chain().clone());
        let new_paid_entry = (self.pay_token.clone(), self.amount);

        if let Some(existing) = effective_launchpad_position(state, &delta, &id) {
            let mut new_position = existing.clone();
            let PositionKind::LaunchpadAllocation(ref mut alloc) = new_position.kind else {
                return Err(ReducerError::Invariant(format!(
                    "position {id} exists but is not a LaunchpadAllocation"
                )));
            };
            // Coalesce paid amounts on the same pay_token; otherwise push.
            if let Some(entry) = alloc.paid.iter_mut().find(|(t, _)| t == &self.pay_token) {
                entry.1 = entry
                    .1
                    .checked_add(self.amount)
                    .ok_or_else(|| ReducerError::Invariant("paid sum overflow".into()))?;
            } else {
                alloc.paid.push(new_paid_entry);
            }
            new_position.primitives_synced_at = ctx.now;
            push_position_update(&mut delta, &new_position);
        } else {
            // Brand-new position. `allocated` is announced post-sale, so we
            // seed `(pay_token, 0)` as a placeholder; `ClaimAllocation` will
            // overwrite both legs with the live allocation tuple.
            // Sales without a vest schedule are represented by a degenerate
            // Linear curve with `start == end == now` so the schedule
            // deserialises without an optional dance downstream.
            let default_vest = VestSchedule {
                start: ctx.now,
                cliff: None,
                end: Some(ctx.now),
                curve: VestCurve::Linear,
                total: U256::ZERO,
            };
            let allocation = LaunchpadAllocation {
                platform: self.platform.clone(),
                sale_id: self.sale_id.clone(),
                paid: vec![new_paid_entry],
                allocated: (self.pay_token.clone(), U256::ZERO),
                vest: sale.vest_schedule.clone().unwrap_or(default_vest),
                claimed: U256::ZERO,
                claimable_now: U256::ZERO,
            };
            let position = Position {
                id,
                protocol: self.platform.clone(),
                chain,
                kind: PositionKind::LaunchpadAllocation(allocation),
                primitives_synced_at: ctx.now,
                primitives_source: DataSource::UserSupplied,
            };
            delta
                .position_changes
                .push(PositionChange::Open { position });
        }

        Ok(delta)
    }
}

// ---------------------------------------------------------------------------
// ClaimAllocation
// ---------------------------------------------------------------------------

impl Reducer for ClaimAllocationAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        if !self.live_inputs.is_claimable.value {
            return Err(ReducerError::Invariant(format!(
                "launchpad allocation on sale {} is not yet claimable",
                self.sale_id
            )));
        }

        let (alloc_token, alloc_amount) = self.live_inputs.allocated.value.clone();
        let refund_due = self.live_inputs.refund_due.value;

        let mut delta = StateDelta::new();
        helpers::balance::credit(state, &mut delta, &alloc_token.key, alloc_amount)?;

        // Optional refund (oversubscription). The refund token is the
        // pay_token recorded on the position; we look that up below.
        let id = launchpad_position_id(&self.platform.name, &self.sale_id, self.recipient);
        let existing = effective_launchpad_position(state, &delta, &id)
            .ok_or_else(|| ReducerError::PositionNotFound(id.clone()))?;
        let mut new_position = existing.clone();

        let PositionKind::LaunchpadAllocation(ref mut alloc) = new_position.kind else {
            return Err(ReducerError::Invariant(format!(
                "position {id} exists but is not a LaunchpadAllocation"
            )));
        };

        // Credit the refund on the first paid pay_token (the canonical
        // refund channel — sales with multiple pay_tokens are not yet
        // supported by `RefundLiveInputs`).
        if !refund_due.is_zero() {
            let pay_token = alloc.paid.first().map(|(t, _)| t.clone()).ok_or_else(|| {
                ReducerError::Invariant(format!(
                    "ClaimAllocation refund on {id} but no paid entries"
                ))
            })?;
            helpers::balance::credit(state, &mut delta, &pay_token.key, refund_due)?;
        }

        alloc.allocated = (alloc_token, alloc_amount);
        // `claimed` advances only by the portion that is unconditionally
        // released at allocation time. If a vest curve is in force the
        // `claimable_now` field tracks future unlocks; we conservatively
        // set `claimed` to the full allocated amount only when no vest
        // window is in place (start == end), otherwise the per-unlock
        // ClaimVested calls own the bookkeeping.
        let has_vest_window = alloc.vest.start < alloc.vest.end.unwrap_or(alloc.vest.start);
        if has_vest_window {
            // Vesting: ClaimAllocation only records the allocation total.
            // Downstream `ClaimVested` claims the per-cliff/per-step slice.
            alloc.vest.total = alloc_amount;
        } else {
            alloc.claimed = alloc_amount;
            alloc.claimable_now = U256::ZERO;
        }
        new_position.primitives_synced_at = ctx.now;
        push_position_update(&mut delta, &new_position);

        Ok(delta)
    }
}

// ---------------------------------------------------------------------------
// ClaimVested
// ---------------------------------------------------------------------------

impl Reducer for ClaimVestedAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let id = &self.position_id;
        let empty = StateDelta::new();
        let existing = effective_launchpad_position(state, &empty, id)
            .ok_or_else(|| ReducerError::PositionNotFound(id.clone()))?;

        let claimable_now = self.live_inputs.claimable_now.value;
        let requested = self.amount.unwrap_or(claimable_now);
        if requested > claimable_now {
            return Err(ReducerError::Invariant(format!(
                "ClaimVested on {id} requests {requested} > claimable_now {claimable_now}"
            )));
        }
        if requested.is_zero() {
            // Nothing to claim — still a valid no-op (e.g. dust trigger).
            return Ok(StateDelta::new());
        }

        let mut delta = StateDelta::new();
        let mut new_position = existing.clone();
        match &mut new_position.kind {
            PositionKind::LaunchpadAllocation(alloc) => {
                let (token, _) = alloc.allocated.clone();
                helpers::balance::credit(state, &mut delta, &token.key, requested)?;
                alloc.claimed = alloc
                    .claimed
                    .checked_add(requested)
                    .ok_or_else(|| ReducerError::Invariant("claimed overflow".into()))?;
                alloc.claimable_now = saturating_sub(claimable_now, requested);
            }
            PositionKind::VestingSchedule(schedule) => {
                let VestingSchedule {
                    token,
                    claimed,
                    claimable_now: snap_claimable,
                    ..
                } = schedule;
                helpers::balance::credit(state, &mut delta, &token.key, requested)?;
                *claimed = claimed
                    .checked_add(requested)
                    .ok_or_else(|| ReducerError::Invariant("claimed overflow".into()))?;
                *snap_claimable = saturating_sub(*snap_claimable, requested);
            }
            other => {
                return Err(ReducerError::Invariant(format!(
                    "ClaimVested target {id} is not a LaunchpadAllocation or VestingSchedule \
                     (got {other:?})"
                )));
            }
        }
        new_position.primitives_synced_at = ctx.now;
        push_position_update(&mut delta, &new_position);

        Ok(delta)
    }
}

// ---------------------------------------------------------------------------
// Refund
// ---------------------------------------------------------------------------

impl Reducer for RefundAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        let refund_amount = self.live_inputs.refund_amount.value;
        let refund_token = self.live_inputs.refund_token.value.clone();
        if refund_amount.is_zero() {
            return Err(ReducerError::Invariant(format!(
                "RefundAction on sale {} has refund_amount = 0",
                self.sale_id
            )));
        }

        let mut delta = StateDelta::new();
        helpers::balance::credit(state, &mut delta, &refund_token.key, refund_amount)?;

        // The position is closed: the user has exited this sale via refund.
        // (If a sale supported partial refunds with continued participation
        // the action surface would need a separate `PartialRefundAction` —
        // current spec §8 routes oversubscription refunds through
        // `ClaimAllocationAction.live_inputs.refund_due` instead.)
        let id = launchpad_position_id(&self.platform.name, &self.sale_id, self.recipient);
        if effective_launchpad_position(state, &delta, &id).is_some() {
            delta.position_changes.push(PositionChange::Close { id });
        }
        let _ = ctx;
        Ok(delta)
    }
}

// ---------------------------------------------------------------------------
// WithdrawCommit
// ---------------------------------------------------------------------------

impl Reducer for WithdrawCommitAction {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        // Sale must still be active — post-sale withdrawal flows through
        // `RefundAction` instead.
        let sale: &SaleState = &self.live_inputs.sale_state.value;
        if !sale.is_active {
            return Err(ReducerError::Invariant(format!(
                "WithdrawCommit on sale {} rejected: sale is no longer active",
                self.sale_id
            )));
        }

        let withdrawable = self.live_inputs.withdrawable.value;
        let requested = self.amount.unwrap_or(withdrawable);
        if requested.is_zero() {
            return Err(ReducerError::Invariant(format!(
                "WithdrawCommit on sale {} requested zero",
                self.sale_id
            )));
        }
        if requested > withdrawable {
            return Err(ReducerError::Invariant(format!(
                "WithdrawCommit on sale {sale_id}: requested {requested} > withdrawable {withdrawable}",
                sale_id = self.sale_id,
            )));
        }

        // The action does not carry a recipient (the platform always
        // refunds to the original committer). We derive the position id
        // from `state.wallet_id.address` — the wallet that originally
        // submitted the `Commit`.
        let id = launchpad_position_id(&self.platform.name, &self.sale_id, state.wallet_id.address);
        let empty = StateDelta::new();
        let existing = effective_launchpad_position(state, &empty, &id)
            .ok_or_else(|| ReducerError::PositionNotFound(id.clone()))?;
        let mut new_position = existing.clone();
        let PositionKind::LaunchpadAllocation(ref mut alloc) = new_position.kind else {
            return Err(ReducerError::Invariant(format!(
                "position {id} exists but is not a LaunchpadAllocation"
            )));
        };

        // Identify the pay_token to credit back. Real-world platforms only
        // expose a single pay_token per sale; we mirror that by using the
        // first paid entry (which is also the canonical refund channel
        // recorded on `LaunchpadAllocation.paid`).
        let pay_token = alloc.paid.first().map(|(t, _)| t.clone()).ok_or_else(|| {
            ReducerError::Invariant(format!(
                "WithdrawCommit on {id} but position has no paid entries"
            ))
        })?;

        let mut delta = StateDelta::new();
        helpers::balance::credit(state, &mut delta, &pay_token.key, requested)?;

        // Update or close depending on the remaining balance.
        let total_paid: U256 = alloc
            .paid
            .iter()
            .map(|(_, a)| *a)
            .fold(U256::ZERO, |acc, x| {
                acc.checked_add(x).unwrap_or(U256::ZERO)
            });
        if requested >= total_paid {
            delta.position_changes.push(PositionChange::Close { id });
        } else {
            // Reduce paid amounts proportionally — for the single pay_token
            // case (the only supported one today) this is just a
            // subtraction from the first paid entry. Mirror the
            // single-pay-token assumption from `pay_token` lookup above.
            if let Some(entry) = alloc.paid.first_mut() {
                entry.1 = saturating_sub(entry.1, requested);
            }
            new_position.primitives_synced_at = ctx.now;
            push_position_update(&mut delta, &new_position);
        }

        Ok(delta)
    }
}

// Touch the helper to avoid an `unused` warning if a future refactor drops
// the `RefundAction` ctx parameter — keeps the linter happy without forcing
// us to rename the `ctx` binding.
#[allow(dead_code)]
const fn _ensure_time_zero() -> Time {
    Time::from_unix(0)
}

// ===========================================================================
// Inline tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::launchpad::{
        ClaimAllocationLiveInputs, ClaimVestedLiveInputs, CommitLiveInputs, RefundLiveInputs,
        SaleState, WithdrawCommitLiveInputs,
    };
    use policy_state::delta::TokenChange;
    use policy_state::eval_context::RequestKind;
    use policy_state::live_field::DataSource;
    use policy_state::position::VestCurve;
    use policy_state::primitives::{Address, ChainId, Duration, Price, ProtocolRef, Time, U256};
    use policy_state::token::{
        Balance, BaseCategory, FiatCurrency, PegTarget, TokenHolding, TokenKey, TokenKind, TokenRef,
    };
    use policy_state::wallet::{WalletId, WalletState};
    use policy_state::LiveField;
    use std::str::FromStr;

    fn now() -> Time {
        Time::from_unix(1_738_000_000)
    }

    fn user() -> Address {
        Address::from_str("0x000000000000000000000000000000000000a01c").unwrap()
    }

    fn usdc_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap(),
            },
        }
    }

    fn project_token_ref() -> TokenRef {
        TokenRef {
            key: TokenKey::Erc20 {
                chain: ChainId::ethereum_mainnet(),
                address: Address::from_str("0x000000000000000000000000000000000000face").unwrap(),
            },
        }
    }

    fn make_holding(token: &TokenRef, amount: u128) -> TokenHolding {
        let key = token.key.clone();
        let contract = key
            .contract()
            .copied()
            .unwrap_or_else(|| Address::from([0u8; 20]));
        TokenHolding {
            key,
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
            last_synced_at: Time::from_unix(1_000_000),
            primitives_source: DataSource::OnchainView {
                chain: ChainId::ethereum_mainnet(),
                contract,
                function: "balanceOf(address)".into(),
                decoder_id: "erc20_balance".into(),
            },
        }
    }

    fn state_with_holdings() -> WalletState {
        let mut s = WalletState::new(WalletId::new(user(), [ChainId::ethereum_mainnet()]));
        let usdc = make_holding(&usdc_ref(), 5_000_000_000);
        let project = make_holding(&project_token_ref(), 0);
        s.tokens.insert(usdc.key.clone(), usdc);
        s.tokens.insert(project.key.clone(), project);
        s
    }

    fn ctx() -> EvalContext {
        EvalContext::new(ChainId::ethereum_mainnet(), now(), RequestKind::Transaction)
    }

    fn live_u256(v: u128) -> LiveField<U256> {
        LiveField::new(U256::from(v), DataSource::UserSupplied, now())
            .with_ttl(Duration::from_secs(60))
    }

    fn live_bool(b: bool) -> LiveField<bool> {
        LiveField::new(b, DataSource::UserSupplied, now()).with_ttl(Duration::from_secs(60))
    }

    fn live_token(t: TokenRef) -> LiveField<TokenRef> {
        LiveField::new(t, DataSource::UserSupplied, now()).with_ttl(Duration::from_secs(60))
    }

    fn live_allocation(t: TokenRef, amt: u128) -> LiveField<(TokenRef, U256)> {
        LiveField::new((t, U256::from(amt)), DataSource::UserSupplied, now())
            .with_ttl(Duration::from_secs(60))
    }

    fn open_window() -> (Time, Time) {
        (
            Time::from_unix(now().as_unix() - 3600),
            Time::from_unix(now().as_unix() + 3600),
        )
    }

    fn sale_state_active(total: u128, hard_cap: Option<u128>) -> SaleState {
        SaleState {
            is_active: true,
            total_committed: U256::from(total),
            hard_cap: hard_cap.map(U256::from),
            soft_cap: None,
            sale_window: open_window(),
            vest_schedule: None,
        }
    }

    fn sale_state_inactive() -> SaleState {
        let mut s = sale_state_active(0, None);
        s.is_active = false;
        s
    }

    fn live_sale_state(s: SaleState) -> LiveField<SaleState> {
        LiveField::new(s, DataSource::UserSupplied, now()).with_ttl(Duration::from_secs(60))
    }

    fn live_opt_price() -> LiveField<Option<Price>> {
        LiveField::new(None, DataSource::UserSupplied, now())
    }

    fn commit_action(amount: u128, cap: u128) -> CommitAction {
        CommitAction {
            platform: ProtocolRef::new("coinlist"),
            sale_id: "sale-1".into(),
            pay_token: usdc_ref(),
            amount: U256::from(amount),
            recipient: user(),
            live_inputs: CommitLiveInputs {
                sale_state: live_sale_state(sale_state_active(0, None)),
                user_cap: live_u256(cap),
                user_committed: live_u256(0),
                expected_token_price: live_opt_price(),
            },
        }
    }

    // ---------- CommitAction ----------

    #[test]
    fn commit_happy_path_debits_and_opens_position() {
        let state = state_with_holdings();
        let a = commit_action(1_000_000_000, 5_000_000_000);
        let delta = a.apply(&state, &ctx()).unwrap();

        assert_eq!(delta.token_changes.len(), 1);
        match &delta.token_changes[0] {
            TokenChange::BalanceDelta { delta: d, .. } => assert!(d.is_negative()),
            other => panic!("expected BalanceDelta, got {other:?}"),
        }

        assert_eq!(delta.position_changes.len(), 1);
        match &delta.position_changes[0] {
            PositionChange::Open { position } => {
                assert!(matches!(
                    position.kind,
                    PositionKind::LaunchpadAllocation(_)
                ));
            }
            other => panic!("expected Position::Open, got {other:?}"),
        }
    }

    #[test]
    fn commit_inactive_sale_is_invariant() {
        let state = state_with_holdings();
        let mut a = commit_action(1_000_000_000, 5_000_000_000);
        a.live_inputs.sale_state = live_sale_state(sale_state_inactive());
        let err = a.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    #[test]
    fn commit_exceeds_user_cap_is_invariant() {
        let state = state_with_holdings();
        let a = commit_action(2_000_000_000, 1_000_000_000);
        let err = a.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    #[test]
    fn commit_exceeds_hard_cap_is_invariant() {
        let state = state_with_holdings();
        let mut a = commit_action(2_000_000_000, 5_000_000_000);
        a.live_inputs.sale_state =
            live_sale_state(sale_state_active(900_000_000, Some(1_000_000_000)));
        let err = a.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    #[test]
    fn commit_outside_window_is_invariant() {
        let state = state_with_holdings();
        let mut a = commit_action(1_000_000_000, 5_000_000_000);
        let mut s = sale_state_active(0, None);
        s.sale_window = (
            Time::from_unix(now().as_unix() + 1000),
            Time::from_unix(now().as_unix() + 2000),
        );
        a.live_inputs.sale_state = live_sale_state(s);
        let err = a.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    #[test]
    fn commit_twice_coalesces_paid_entry() {
        let mut state = state_with_holdings();
        let a1 = commit_action(500_000_000, 5_000_000_000);
        let d1 = a1.apply(&state, &ctx()).unwrap();
        state = crate::helpers::delta::apply_delta(&state, &d1).unwrap();

        let mut a2 = a1;
        a2.live_inputs.user_committed = live_u256(500_000_000);
        let d2 = a2.apply(&state, &ctx()).unwrap();
        // The second commit emits an Update (existing position is augmented).
        let position_id = launchpad_position_id("coinlist", "sale-1", user());
        let found_update = d2.position_changes.iter().any(|c| {
            matches!(c,
                PositionChange::Update { id, .. } if id == &position_id
            )
        });
        assert!(found_update, "expected an Update on second commit");
    }

    // ---------- ClaimAllocationAction ----------

    fn commit_then_state(amount: u128, cap: u128) -> WalletState {
        let state = state_with_holdings();
        let a = commit_action(amount, cap);
        let d = a.apply(&state, &ctx()).unwrap();
        crate::helpers::delta::apply_delta(&state, &d).unwrap()
    }

    fn claim_allocation_action(allocated: u128, refund_due: u128) -> ClaimAllocationAction {
        ClaimAllocationAction {
            platform: ProtocolRef::new("coinlist"),
            sale_id: "sale-1".into(),
            recipient: user(),
            live_inputs: ClaimAllocationLiveInputs {
                allocated: live_allocation(project_token_ref(), allocated),
                refund_due: live_u256(refund_due),
                is_claimable: live_bool(true),
            },
        }
    }

    #[test]
    fn claim_allocation_credits_token_and_updates_position() {
        let state = commit_then_state(1_000_000_000, 5_000_000_000);
        let a = claim_allocation_action(1_000_000_000_000_000_000u128, 0);
        let d = a.apply(&state, &ctx()).unwrap();
        // 1 token credit + 1 position update.
        assert_eq!(d.token_changes.len(), 1);
        assert_eq!(d.position_changes.len(), 1);
        assert!(matches!(
            d.position_changes[0],
            PositionChange::Update { .. }
        ));
    }

    #[test]
    fn claim_allocation_credits_refund_due() {
        let state = commit_then_state(1_000_000_000, 5_000_000_000);
        let a = claim_allocation_action(500_000_000_000_000_000u128, 100);
        let d = a.apply(&state, &ctx()).unwrap();
        // 2 credits (allocation + refund) + 1 update.
        assert_eq!(d.token_changes.len(), 2);
        assert_eq!(d.position_changes.len(), 1);
    }

    #[test]
    fn claim_allocation_not_claimable_is_invariant() {
        let state = commit_then_state(1_000_000_000, 5_000_000_000);
        let mut a = claim_allocation_action(1, 0);
        a.live_inputs.is_claimable = live_bool(false);
        let err = a.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    #[test]
    fn claim_allocation_without_commit_is_position_not_found() {
        let state = state_with_holdings();
        let a = claim_allocation_action(1, 0);
        let err = a.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::PositionNotFound(_)));
    }

    // ---------- ClaimVestedAction ----------

    fn claim_vested_action(
        position_id: PositionId,
        claimable: u128,
        amount: Option<u128>,
    ) -> ClaimVestedAction {
        ClaimVestedAction {
            position_id,
            amount: amount.map(U256::from),
            live_inputs: ClaimVestedLiveInputs {
                claimable_now: live_u256(claimable),
                next_unlock: LiveField::new(None, DataSource::UserSupplied, now()),
            },
        }
    }

    fn launchpad_position_with_vest(id: &str, total: u128, claimable: u128) -> Position {
        Position {
            id: id.to_string(),
            protocol: ProtocolRef::new("coinlist"),
            chain: Some(ChainId::ethereum_mainnet()),
            kind: PositionKind::LaunchpadAllocation(LaunchpadAllocation {
                platform: ProtocolRef::new("coinlist"),
                sale_id: "sale-1".into(),
                paid: vec![(usdc_ref(), U256::from(1_000u64))],
                allocated: (project_token_ref(), U256::from(total)),
                vest: VestSchedule {
                    start: Time::from_unix(now().as_unix()),
                    cliff: None,
                    end: Some(Time::from_unix(now().as_unix() + 86_400)),
                    curve: VestCurve::Linear,
                    total: U256::from(total),
                },
                claimed: U256::ZERO,
                claimable_now: U256::from(claimable),
            }),
            primitives_synced_at: now(),
            primitives_source: DataSource::UserSupplied,
        }
    }

    fn vesting_schedule_position(id: &str, total: u128, claimable: u128) -> Position {
        Position {
            id: id.to_string(),
            protocol: ProtocolRef::new("optimism_grant"),
            chain: Some(ChainId::ethereum_mainnet()),
            kind: PositionKind::VestingSchedule(VestingSchedule {
                granter: ProtocolRef::new("optimism_grant"),
                token: project_token_ref(),
                schedule: VestSchedule {
                    start: Time::from_unix(now().as_unix()),
                    cliff: None,
                    end: Some(Time::from_unix(now().as_unix() + 86_400)),
                    curve: VestCurve::Linear,
                    total: U256::from(total),
                },
                claimed: U256::ZERO,
                claimable_now: U256::from(claimable),
            }),
            primitives_synced_at: now(),
            primitives_source: DataSource::UserSupplied,
        }
    }

    #[test]
    fn claim_vested_on_launchpad_allocation_credits_and_updates() {
        let mut state = state_with_holdings();
        let pos = launchpad_position_with_vest("alloc-1", 1_000, 500);
        state.positions.push(pos);
        let a = claim_vested_action("alloc-1".into(), 500, None);
        let d = a.apply(&state, &ctx()).unwrap();
        assert_eq!(d.token_changes.len(), 1);
        assert!(matches!(
            d.position_changes[0],
            PositionChange::Update { .. }
        ));
    }

    #[test]
    fn claim_vested_on_vesting_schedule_credits_and_updates() {
        let mut state = state_with_holdings();
        let pos = vesting_schedule_position("vest-1", 1_000, 250);
        state.positions.push(pos);
        let a = claim_vested_action("vest-1".into(), 250, Some(200));
        let d = a.apply(&state, &ctx()).unwrap();
        assert_eq!(d.token_changes.len(), 1);
        assert!(matches!(
            d.position_changes[0],
            PositionChange::Update { .. }
        ));
    }

    #[test]
    fn claim_vested_zero_returns_empty_delta() {
        let mut state = state_with_holdings();
        let pos = launchpad_position_with_vest("alloc-1", 1_000, 0);
        state.positions.push(pos);
        let a = claim_vested_action("alloc-1".into(), 0, None);
        let d = a.apply(&state, &ctx()).unwrap();
        assert!(d.is_empty());
    }

    #[test]
    fn claim_vested_request_exceeds_claimable_is_invariant() {
        let mut state = state_with_holdings();
        let pos = launchpad_position_with_vest("alloc-1", 1_000, 250);
        state.positions.push(pos);
        let a = claim_vested_action("alloc-1".into(), 250, Some(300));
        let err = a.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    #[test]
    fn claim_vested_missing_position_is_position_not_found() {
        let state = state_with_holdings();
        let a = claim_vested_action("ghost".into(), 1, None);
        let err = a.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::PositionNotFound(_)));
    }

    // ---------- RefundAction ----------

    fn refund_action(amount: u128) -> RefundAction {
        RefundAction {
            platform: ProtocolRef::new("coinlist"),
            sale_id: "sale-1".into(),
            recipient: user(),
            live_inputs: RefundLiveInputs {
                refund_amount: live_u256(amount),
                refund_token: live_token(usdc_ref()),
            },
        }
    }

    #[test]
    fn refund_credits_and_closes_position() {
        let state = commit_then_state(1_000_000_000, 5_000_000_000);
        let a = refund_action(1_000_000_000);
        let d = a.apply(&state, &ctx()).unwrap();
        assert_eq!(d.token_changes.len(), 1);
        let close_present = d
            .position_changes
            .iter()
            .any(|c| matches!(c, PositionChange::Close { .. }));
        assert!(close_present, "expected PositionChange::Close");
    }

    #[test]
    fn refund_without_position_still_credits() {
        // Refund is allowed even without a tracked position (off-platform
        // commits / cross-wallet refunds). Only the credit lands.
        let state = state_with_holdings();
        let a = refund_action(100);
        let d = a.apply(&state, &ctx()).unwrap();
        assert_eq!(d.token_changes.len(), 1);
        assert!(d.position_changes.is_empty());
    }

    #[test]
    fn refund_zero_is_invariant() {
        let state = state_with_holdings();
        let a = refund_action(0);
        let err = a.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    // ---------- WithdrawCommitAction ----------

    fn withdraw_action(amount: Option<u128>, withdrawable: u128) -> WithdrawCommitAction {
        WithdrawCommitAction {
            platform: ProtocolRef::new("coinlist"),
            sale_id: "sale-1".into(),
            amount: amount.map(U256::from),
            live_inputs: WithdrawCommitLiveInputs {
                withdrawable: live_u256(withdrawable),
                sale_state: live_sale_state(sale_state_active(0, None)),
            },
        }
    }

    #[test]
    fn withdraw_full_commit_closes_position() {
        let state = commit_then_state(1_000_000_000, 5_000_000_000);
        let a = withdraw_action(None, 1_000_000_000);
        let d = a.apply(&state, &ctx()).unwrap();
        assert_eq!(d.token_changes.len(), 1);
        let close_present = d
            .position_changes
            .iter()
            .any(|c| matches!(c, PositionChange::Close { .. }));
        assert!(close_present);
    }

    #[test]
    fn withdraw_partial_updates_position() {
        let state = commit_then_state(1_000_000_000, 5_000_000_000);
        let a = withdraw_action(Some(400_000_000), 1_000_000_000);
        let d = a.apply(&state, &ctx()).unwrap();
        assert_eq!(d.token_changes.len(), 1);
        let update_present = d
            .position_changes
            .iter()
            .any(|c| matches!(c, PositionChange::Update { .. }));
        assert!(update_present);
    }

    #[test]
    fn withdraw_on_inactive_sale_is_invariant() {
        let state = commit_then_state(1_000_000_000, 5_000_000_000);
        let mut a = withdraw_action(Some(100), 1_000_000_000);
        a.live_inputs.sale_state = live_sale_state(sale_state_inactive());
        let err = a.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    #[test]
    fn withdraw_exceeds_withdrawable_is_invariant() {
        let state = commit_then_state(1_000_000_000, 5_000_000_000);
        let a = withdraw_action(Some(2_000_000_000), 1_000_000_000);
        let err = a.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::Invariant(_)));
    }

    #[test]
    fn withdraw_without_position_is_position_not_found() {
        let state = state_with_holdings();
        let a = withdraw_action(None, 100);
        let err = a.apply(&state, &ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::PositionNotFound(_)));
    }

    // ---------- Dispatcher ----------

    #[test]
    fn launchpad_action_dispatcher_routes_to_commit() {
        let state = state_with_holdings();
        let outer = LaunchpadAction::Commit(commit_action(1_000_000_000, 5_000_000_000));
        let d = outer.apply(&state, &ctx()).unwrap();
        assert_eq!(d.token_changes.len(), 1);
    }
}
