//! `simulate_step_json`: one (state, action) -> (delta, next_state) step.
//!
//! Thin wrapper that ties together the two pure-Rust pieces already shipped:
//!   1. `policy_transition::apply` — pure reducer producing a `StateDelta` from
//!      `(WalletState, Action, EvalContext)`.
//!   2. `policy_transition::helpers::delta::apply_delta` — applies the delta
//!      back to a fresh `WalletState` so the host gets the post-step state in
//!      one round trip.
//!
//! Why a single export instead of letting the host chain the two:
//!   * keeps step atomicity at the WASM boundary — a partial `apply` /
//!     `apply_delta` mismatch cannot escape into host state.
//!   * one JSON parse/serialize per simulation step instead of two.
//!   * matches the simulation-page panel contract: every step is
//!     `{ pre, delta, post, verdict? }`. Verdict aggregation is deliberately
//!     not folded in here — the simulator UI may want to drive policies
//!     independently of the reducer, and `evaluate_action_v2_json` already
//!     owns that surface.
//!
//! Wire shape:
//! ```text
//! input:  { state, action, ctx }
//! output: { ok: true,  data: { delta, next_state } }
//!     or: { ok: false, error: { kind, message } }
//! ```
//! `kind` discriminates failure surfaces: `"invalid_input"` (JSON parse),
//! `"apply_failed"` (reducer rejected the action), `"apply_delta_failed"`
//! (delta could not be composed onto the state — invariant violation).

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::wasm_bindgen;

use policy_state::{EvalContext, StateDelta, WalletState};
use policy_transition::action::Action;
use policy_transition::apply::apply;
use policy_transition::helpers::delta::apply_delta;

use crate::dto::{EngineErrorDto, Envelope};
use crate::exports::check_input_size;

#[derive(Debug, Deserialize)]
struct SimStepInput {
    state: WalletState,
    action: Action,
    ctx: EvalContext,
}

#[derive(Debug, Serialize)]
struct SimStepOutput {
    delta: StateDelta,
    next_state: WalletState,
}

/// One simulation step over a single already-decoded `Action`.
///
/// The host owns the loop: read base state once from the policy server, feed
/// `(state, action_i, ctx)` for each `i`, and overwrite the local `state`
/// variable with `next_state` before the next call. The WASM keeps no state
/// across calls — `(state, action, ctx)` fully determines the output.
///
/// For batched / multi-call actions the host should pass each inner `Action`
/// from the decoder's `actions: Vec<Action>` output in order; this entry does
/// not split a multicall on the caller's behalf.
#[wasm_bindgen]
#[must_use]
pub fn simulate_step_json(input_json: String) -> String {
    let result = (|| -> Result<SimStepOutput, EngineErrorDto> {
        check_input_size(&input_json, "simulate_step_json")?;

        let input: SimStepInput = serde_json::from_str(&input_json)
            .map_err(|e| EngineErrorDto::new("invalid_input", e.to_string()))?;

        let delta = apply(&input.state, &input.action, &input.ctx)
            .map_err(|e| EngineErrorDto::new("apply_failed", e.to_string()))?;

        let next_state = apply_delta(&input.state, &delta)
            .map_err(|e| EngineErrorDto::new("apply_delta_failed", e.to_string()))?;

        Ok(SimStepOutput { delta, next_state })
    })();

    match result {
        Ok(out) => Envelope::ok(out).to_json(),
        Err(e) => Envelope::<()>::err(e.kind, e.message).to_json(),
    }
}
