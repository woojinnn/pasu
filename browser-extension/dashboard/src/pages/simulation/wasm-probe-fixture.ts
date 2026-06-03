/**
 * Hardcoded sample payload for the WASM step probe.
 *
 * The JSON itself is produced by the `emit_sim_step_sample` example in
 * `crates/policy-engine-wasm/examples/` — a one-shot Rust program that
 * serializes the same typed `WalletState` / `Action` / `EvalContext` triple
 * the integration test uses (`crates/policy-engine-wasm/tests/sim_step.rs`).
 *
 * Re-generate when the upstream type shapes change:
 *   cargo run -p policy-engine-wasm --example emit_sim_step_sample \
 *     > browser-extension/dashboard/src/pages/simulation/sim-step-sample.json
 *
 * This guarantees the dashboard probe's wire format never drifts from the
 * wasm-bindgen `.d.ts` emission — both are derived from the same Rust
 * structs.
 *
 * Domain: ERC20 transfer of 250,000,000 USDC out of a wallet that holds
 * 1,000,000,000. Expected wasm response: `delta.token_changes[0]` is a
 * negative BalanceDelta for USDC; `next_state` shows the holding at
 * 750,000,000.
 */

import type { SimulateStepInput } from "./sim-bridge";
import sample from "./sim-step-sample.json";

export const SAMPLE_ERC20_TRANSFER_PROBE: SimulateStepInput =
  sample as unknown as SimulateStepInput;
