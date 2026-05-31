//! `EvalContext` — metadata describing a single evaluation call.
//!
//! Not state, not an action, not a `LiveField`: it is the contextual information
//! the caller passes to the reducer for this one evaluation.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use crate::primitives::{ChainId, Time};

/// Kind of request being evaluated; mirrors `RootRequest.requestKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub enum RequestKind {
    /// A standard on-chain transaction.
    Transaction,
    /// An off-chain message signature (e.g. EIP-712 / `personal_sign`).
    Signature,
    /// An ERC-4337 user operation.
    UserOperation,
}

/// Evaluation mode controlling whether results are simulated or persisted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(rename_all = "snake_case")]
pub enum SimulationMode {
    /// Dry-run simulation for previewing the outcome to the user; state is not committed.
    Preview,
    /// Real evaluation after signing, applying changes to actual state.
    Commit,
}

/// Contextual metadata for a single evaluation call passed from the caller to the reducer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct EvalContext {
    /// Chain that this evaluation is performed against.
    pub chain: ChainId,
    /// Reference timestamp for the evaluation (block.timestamp or wall-clock).
    pub now: Time,
    /// Index of this action within the request's `actions[]` list.
    pub envelope_index: usize,
    /// Kind of request being evaluated.
    pub request_kind: RequestKind,
    /// Evaluation mode (preview simulation or commit).
    pub simulation: SimulationMode,
}

impl EvalContext {
    /// Creates a new `EvalContext` for the given chain, time, and request kind,
    /// defaulting `envelope_index` to 0 and `simulation` to [`SimulationMode::Preview`].
    #[must_use]
    pub const fn new(chain: ChainId, now: Time, request_kind: RequestKind) -> Self {
        Self {
            chain,
            now,
            envelope_index: 0,
            request_kind,
            simulation: SimulationMode::Preview,
        }
    }

    /// Returns this context with `envelope_index` set to `i`.
    #[must_use]
    pub const fn with_envelope_index(mut self, i: usize) -> Self {
        self.envelope_index = i;
        self
    }

    /// Returns this context with the simulation mode set to `mode`.
    #[must_use]
    pub const fn with_simulation(mut self, mode: SimulationMode) -> Self {
        self.simulation = mode;
        self
    }
}
