//! Top-level entry point and the `Reducer` trait.
//! The simulator's contract is:
//! ```ignore
//! apply(state, action, ctx) -> ReducerResult<StateDelta>
//! ```
//! Pure function: `state` is read-only and the returned `StateDelta` describes
//! what *would* change. Use `helpers::delta::apply_delta` to advance the state.

use policy_state::{EvalContext, StateDelta, WalletState};

use crate::action::{Action, ActionBody};
use crate::error::{ReducerError, ReducerResult};

/// A reducer applies itself (typically an `Action` subtree) to a wallet state
/// and returns the resulting `StateDelta`. Composable: outer `Reducer`s
/// `match` on their variants and delegate to inner ones.
pub trait Reducer {
    /// Compute the `StateDelta` produced when `self` is applied to `state`
    /// under the evaluation context `ctx`.
    ///
    /// The implementation must NOT mutate `state`. All change information
    /// lives in the returned delta.
    ///
    /// # Errors
    ///
    /// Returns [`ReducerError`] when the action is
    /// invalid for the supplied state or a reducer invariant is violated.
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta>;
}

/// Public entry point — call this from the caller's side.
/// Mirrors `Reducer::apply` but takes `state` first so the caller-side
/// reading order is `(state, action, ctx)`.
///
/// # Errors
///
/// Returns [`ReducerError`] when the action cannot be applied to the supplied
/// state.
pub fn apply(state: &WalletState, action: &Action, ctx: &EvalContext) -> ReducerResult<StateDelta> {
    action.body.apply(state, ctx)
}

impl Reducer for ActionBody {
    fn apply(&self, state: &WalletState, ctx: &EvalContext) -> ReducerResult<StateDelta> {
        match self {
            Self::Token(a) => a.apply(state, ctx),
            Self::Amm(a) => a.apply(state, ctx),
            Self::Lending(a) => a.apply(state, ctx),
            Self::Airdrop(a) => a.apply(state, ctx),
            Self::Launchpad(a) => a.apply(state, ctx),
            Self::Perp(a) => a.apply(state, ctx),
            Self::LiquidStaking(a) => a.apply(state, ctx),
            Self::Permission(a) => a.apply(state, ctx),
            Self::Yield(a) => a.apply(state, ctx),
            Self::Restaking(a) => a.apply(state, ctx),
            Self::Staking(a) => a.apply(state, ctx),
            // Hyperliquid CORE actions record their effect against the wallet's
            // off-chain Hl account (see effect::hyperliquid_core). No fetch: the
            // reducer reads only state + ctx; Sync populates the base balance.
            Self::HyperliquidCore(a) => a.apply(state, ctx),
            Self::Multicall { actions } => apply_multicall(state, ctx, actions),
            Self::Unknown { target, .. } => Err(ReducerError::UnknownAction(format!(
                "unidentified call to {target:?}"
            ))),
        }
    }
}

/// Sequentially apply each child action, advancing the state with each
/// child's delta before applying the next, and accumulate into a single
/// `StateDelta`.
/// Implements the PDF spec multicall semantics: `state → state' → state''`
/// where each child sees the state produced by all previous siblings. The
/// returned delta is the merged composition of all child deltas (sequence
/// merge: `merge_delta(a, b)` = "apply `a` then `b`").
fn apply_multicall(
    state: &WalletState,
    ctx: &EvalContext,
    actions: &[ActionBody],
) -> ReducerResult<StateDelta> {
    let mut accumulated = StateDelta::default();
    let mut current = state.clone();
    for (i, body) in actions.iter().enumerate() {
        let sub_ctx = ctx.clone().with_action_index(i);
        let sub_delta = body.apply(&current, &sub_ctx)?;
        current = crate::helpers::delta::apply_delta(&current, &sub_delta)?;
        accumulated = crate::helpers::delta::merge_delta(accumulated, sub_delta)?;
    }
    Ok(accumulated)
}

#[cfg(test)]
mod tests {
    use super::*;

    use policy_state::eval_context::{RequestKind, SimulationMode};
    use policy_state::primitives::{Address, ChainId, Time, U256};
    use policy_state::wallet::{WalletId, WalletState};

    fn eval_ctx() -> EvalContext {
        EvalContext::new(
            ChainId::ethereum_mainnet(),
            Time::from_unix(1_738_000_000),
            RequestKind::Transaction,
        )
        .with_simulation(SimulationMode::Preview)
    }

    fn empty_state() -> WalletState {
        WalletState::new(WalletId::new(
            Address::from([0u8; 20]),
            std::iter::empty::<ChainId>(),
        ))
    }

    fn unknown_body(value: u64) -> ActionBody {
        ActionBody::Unknown {
            target: Address::from([0u8; 20]),
            chain: ChainId::ethereum_mainnet(),
            calldata: format!("0x{value:08x}"),
            value: U256::ZERO,
        }
    }

    #[test]
    fn unknown_body_errs() {
        let state = empty_state();
        let body = unknown_body(0);
        let err = body.apply(&state, &eval_ctx()).unwrap_err();
        assert!(matches!(err, ReducerError::UnknownAction(_)));
    }

    #[test]
    fn empty_multicall_returns_empty_delta() {
        let state = empty_state();
        let multi = ActionBody::Multicall { actions: vec![] };
        let delta = multi.apply(&state, &eval_ctx()).unwrap();
        assert!(delta.is_empty());
    }

    #[test]
    fn multicall_with_unknown_child_propagates_err() {
        let state = empty_state();
        let multi = ActionBody::Multicall {
            actions: vec![unknown_body(1)],
        };
        let res = multi.apply(&state, &eval_ctx());
        assert!(res.is_err());
    }

    #[test]
    fn nested_empty_multicall_returns_empty_delta() {
        // Sequence-merge invariant: two empty children -> empty accumulated delta.
        let state = empty_state();
        let multi = ActionBody::Multicall {
            actions: vec![
                ActionBody::Multicall { actions: vec![] },
                ActionBody::Multicall { actions: vec![] },
            ],
        };
        let delta = multi.apply(&state, &eval_ctx()).unwrap();
        assert!(delta.is_empty());
    }

    #[test]
    fn hyperliquid_core_is_not_a_no_op() {
        use crate::action::hyperliquid_core::{HlWithdrawAction, HyperliquidCoreAction};
        use policy_state::primitives::{Address, Decimal};

        let state = empty_state();
        let body = ActionBody::HyperliquidCore(HyperliquidCoreAction::Withdraw(HlWithdrawAction {
            destination: Address::from([0xde; 20]),
            amount: Decimal::new("1000"),
        }));
        let delta = body.apply(&state, &eval_ctx()).unwrap();
        assert!(
            !delta.is_empty(),
            "HL action must now produce a non-empty delta"
        );
        assert_eq!(delta.position_changes.len(), 1);
    }
}
