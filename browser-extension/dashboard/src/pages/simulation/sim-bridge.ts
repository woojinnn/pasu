/**
 * Simulation page → SW bridge.
 *
 * Mirrors `cedar/index.ts`: posts a `sim-step` message to the SW (via the
 * `dashboard-bridge` content script), which forwards to the wasm
 * `simulate_step_json` entry. One call = one simulation step over a single
 * already-decoded `Action`.
 *
 * The host (this module's caller) owns the per-tx loop and threads
 * `next_state` back as the `state` of the next call. The SW / wasm keeps no
 * state across calls; the triple `(state, action, ctx)` fully determines the
 * output, so a buggy step is reproduced by re-submitting the same input.
 *
 * Types are kept opaque (the dashboard never inspects them — it just
 * forwards state from the policy-server through wasm and back). When the
 * panels need to render delta diffs, they should walk a typed view rather
 * than reach into these objects.
 */

import {
  ExtensionBridgeTimeout,
  sendToExtension,
} from "../../server-api/extension-bridge";

/** Tight timeout — wasm step calls return in well under a second locally.
 *  Long timeouts hide "extension not installed" issues. */
const BRIDGE_TIMEOUT_MS = 2_000;

/** Opaque pass-through types. The dashboard treats wallet state, actions,
 *  the eval context, and state deltas as black boxes — they originate at the
 *  policy-server / decoder, flow through wasm, and come back. Rendering code
 *  should derive its own view types from these rather than indexing them
 *  directly. */
export type OpaqueWalletState = Record<string, unknown>;
export type OpaqueAction = Record<string, unknown>;
export type OpaqueEvalContext = Record<string, unknown>;
export type OpaqueStateDelta = Record<string, unknown>;

export interface SimulateStepInput {
  state: OpaqueWalletState;
  action: OpaqueAction;
  ctx: OpaqueEvalContext;
}

export interface SimulateStepOutput {
  delta: OpaqueStateDelta;
  next_state: OpaqueWalletState;
}

/** `true` when the failure is "extension/bridge not reachable". The caller
 *  decides whether to soft-fail (mock mode) or surface a hard error. */
export function isMissingBridge(err: unknown): boolean {
  return err instanceof ExtensionBridgeTimeout;
}

/**
 * One simulation step. Returns `(delta, next_state)`; throws on bridge
 * timeout or any wasm-side failure. The SW already unwraps `{ ok, data }`
 * envelopes, so `sendToExtension` either resolves with `data` or rejects
 * with `ExtensionBridgeError` / `ExtensionBridgeTimeout`.
 */
export async function simulateStepLocal(
  input: SimulateStepInput,
): Promise<SimulateStepOutput> {
  return await sendToExtension<SimulateStepOutput>(
    { type: "sim-step", input },
    BRIDGE_TIMEOUT_MS,
  );
}
