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

// ── calldata decode ───────────────────────────────────────────────────────

/** Wire shape for `sim-decode`. Mirrors the SW's
 *  `DeclarativeRouteRequestV3Input` — every numeric (`uint256`) field is a
 *  base-10 decimal string so the wasm boundary parses safely without losing
 *  precision through JS numbers. */
export interface DecodeCalldataInput {
  chain_id: number;
  /** "0x" + 40 hex. Case-insensitive. */
  to: string;
  /** "0x" + 8 hex. Case-insensitive. */
  selector: string;
  /** Raw "0x"-prefixed calldata. */
  calldata: string;
  /** `msg.value` as a base-10 decimal string. */
  value?: string;
  /** Declared gas limit as a base-10 decimal string. */
  gas_limit?: string;
  /** Current gas price as a base-10 decimal string. */
  gas_price?: string;
  /** `tx.from` — "0x" + 40 hex. */
  submitter: string;
  /** Unix epoch seconds at which the Action was submitted. */
  submitted_at: number;
  /** Sequential transaction nonce of `submitter`. */
  nonce?: number;
  /** Optional `block.timestamp` distinct from `submitted_at`. */
  block_timestamp?: number;
}

export interface DecodeCalldataOutput {
  /** Typed `policy_transition::action::Action[]` produced by the v3 route
   *  engine. A simple ERC20 transfer / approve decodes to one Action; batched
   *  calls (Universal Router, Multicall) decode to many. */
  actions: OpaqueAction[];
  /** Decoder id (`<registry-path>@<version>`) the bundle matched on. Empty
   *  string when the input didn't match any installed manifest — the
   *  `actions` array is then a single `Unknown`-bodied stub. */
  decoder_id: string;
}

/**
 * Decode a raw EVM tx into `Action[]`. One call = one calldata; multicall
 * batches surface as multiple entries in `actions`. The host then feeds each
 * `Action` through `simulateStepLocal` in order.
 */
export async function decodeCalldataLocal(
  input: DecodeCalldataInput,
): Promise<DecodeCalldataOutput> {
  return await sendToExtension<DecodeCalldataOutput>(
    { type: "sim-decode", input },
    BRIDGE_TIMEOUT_MS,
  );
}

// ── v3 bundle install status ──────────────────────────────────────────────

/** How many v3 decoder bundles the SW installed at boot. `bootCompleted` is
 *  `false` while the install pass is still in-flight (the probe distinguishes
 *  "warming up" from "no bundles"). */
export interface V3BundleStatus {
  count: number;
  bootCompleted: boolean;
}

/**
 * Read the SW's per-lifetime v3-bundle-install counter. The simulation
 * probe surfaces a warning when the decoder has nothing to look up —
 * without installed bundles every route call falls back to
 * `ActionBody::Unknown` and the simulator's `Action` shape is opaque
 * calldata.
 *
 * Returns `{ count: 0, bootCompleted: true }` on bridge timeout so the
 * dashboard renders the same "no bundles" message it would for a SW that
 * finished boot with 0 successful installs.
 */
export async function getV3BundleStatus(): Promise<V3BundleStatus> {
  try {
    return await sendToExtension<V3BundleStatus>(
      { type: "sim-v3-bundle-count" },
      BRIDGE_TIMEOUT_MS,
    );
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) {
      return { count: 0, bootCompleted: true };
    }
    throw err;
  }
}

// ── action evaluation ─────────────────────────────────────────────────────

/** Tx-level routing fields the v2 evaluator needs alongside the action.
 *  Mirrors `ActionTxInputDto` in `wasm-bridge.types.ts` — chain_id here is
 *  CAIP-2 STRING (e.g. `"eip155:1"`), NOT a decimal number. */
export interface EvaluateActionTx {
  chain_id: string;
  from: string;
  to: string;
}

/** Wire shape for `sim-evaluate`. Mirrors `EvaluateActionV2InputDto`. The
 *  decoded `Action` JSON splits into `action: <Action>.body` and
 *  `meta: <Action>.meta` — both are pass-through here since the dashboard
 *  doesn't model the variant schema. `results` replays prior `policy_rpc`
 *  fetches into `context.custom.*`; pass `{}` when no enrichment is needed
 *  and the engine surfaces a `SystemFail` for any policy that required it. */
export interface EvaluateActionInput {
  action: OpaqueAction;
  meta: Record<string, unknown>;
  tx: EvaluateActionTx;
  bundles: ReadonlyArray<{ policy: string; manifest: unknown }>;
  results: Record<string, unknown>;
}

export interface MatchedPolicy {
  policy_id: string;
  reason: string | null;
  severity: "warn" | "deny";
  origin: "policy" | "system";
}

/** WASM v2 verdict shape — `pass | warn | fail` discriminated on `kind`.
 *  Mirrors `VerdictDto` in `wasm-bridge.types.ts`. Kept structural so the
 *  dashboard doesn't have to import the SW-side types directly. */
export type EvaluateActionVerdict =
  | { kind: "pass" }
  | { kind: "warn"; matched: ReadonlyArray<MatchedPolicy> }
  | { kind: "fail"; matched: ReadonlyArray<MatchedPolicy> };

/**
 * Evaluate one (action, meta, tx, bundles, results) → verdict. Symmetric
 * with {@link simulateStepLocal} so the host can pair a state-step call
 * with a verdict-step call at each iteration of its tx loop.
 *
 * Throws on bridge timeout or wasm-side failure (the SW unwraps the
 * `{ ok, data }` envelope).
 */
export async function evaluateActionLocal(
  input: EvaluateActionInput,
): Promise<EvaluateActionVerdict> {
  return await sendToExtension<EvaluateActionVerdict>(
    { type: "sim-evaluate", input },
    BRIDGE_TIMEOUT_MS,
  );
}

// ── integrated sequence ───────────────────────────────────────────────────

/** One row of the per-step output. `pre_state` is what the policy verdict
 *  was computed against; `next_state` is what gets threaded into the next
 *  step. Hosts that visualise both verdict and state evolution consume
 *  this directly. */
export interface SimulatedStep {
  /** The action that was simulated, verbatim — keeps the row self-contained
   *  for rendering / debugging without an out-of-band index lookup. */
  action: OpaqueAction;
  /** Wallet state BEFORE the action — the snapshot the verdict was computed
   *  against. The first step's pre_state is the orchestrator's
   *  `initialState`. */
  pre_state: OpaqueWalletState;
  /** Engine verdict against `pre_state`. */
  verdict: EvaluateActionVerdict;
  /** Engine-emitted delta for this step (already applied into next_state). */
  delta: OpaqueStateDelta;
  /** Wallet state AFTER the action — the pre_state of the next step (or
   *  the terminal state when this is the last step). */
  next_state: OpaqueWalletState;
}

export interface SimulateSequenceInput {
  /** Initial wallet state — typically the policy-server's
   *  `GET /wallets/:addr/state` payload. */
  initialState: OpaqueWalletState;
  /** Pre-decoded actions in execution order. Each action is the full
   *  `{ body, meta }` JSON the v3 decoder produced. */
  actions: ReadonlyArray<OpaqueAction>;
  /** Sim context shared across every step (chain, now, request_kind, …).
   *  The orchestrator overrides `action_index` per step. */
  baseCtx: OpaqueEvalContext;
  /** Tx-level routing fields for the v2 evaluator (chain_id, from, to). */
  tx: EvaluateActionTx;
  /** Installed `{policy, manifest}` bundles to evaluate. Empty array →
   *  every verdict is `pass`. */
  bundles: ReadonlyArray<{ policy: string; manifest: unknown }>;
}

export interface SimulateSequenceOutput {
  /** One entry per input action, in order. */
  steps: SimulatedStep[];
  /** Terminal wallet state after the last successful step. Equals
   *  `initialState` when `actions` is empty. */
  finalState: OpaqueWalletState;
}

/**
 * Walk a sequence of decoded actions, threading state forward AND computing
 * the policy verdict at each step. Pairs `evaluate_action_v2_json` with
 * `simulate_step_json` per iteration so the UI can render
 * "verdict + state-diff" rows from one orchestrated call.
 *
 * Semantics:
 *   - Verdict is computed against `pre_state` (the state BEFORE the action),
 *     mirroring how a real wallet enforces — the policy decides whether to
 *     allow the side-effect, then the side-effect happens.
 *   - State threading runs UNCONDITIONALLY: even a `fail` verdict's step
 *     still produces `next_state` so subsequent rows show the
 *     hypothetical "what if this were allowed" trajectory. Callers that
 *     want short-circuit semantics can iterate and break on the first fail.
 *   - `results = {}` (no `policy_rpc` enrichment). Any policy that requires
 *     enrichment will surface a `SystemFail`-shaped `fail` verdict — that's
 *     the engine's contract, not a bug here.
 *
 * Throws on the first WASM error (a deterministic apply / evaluate
 * failure), but lets a verdict's `fail` kind flow through normally.
 */
export async function simulateSequenceWithVerdicts(
  input: SimulateSequenceInput,
): Promise<SimulateSequenceOutput> {
  const steps: SimulatedStep[] = [];
  let pre: OpaqueWalletState = input.initialState;

  for (let i = 0; i < input.actions.length; i++) {
    const fullAction = input.actions[i];
    // Action JSON is `{ body, meta }`; evaluator needs them split. Sim-step
    // takes the full Action — same source object, no copy needed.
    const meta = (fullAction as { meta?: Record<string, unknown> }).meta ?? {};
    const body =
      (fullAction as { body?: Record<string, unknown> }).body ?? fullAction;

    const verdict = await evaluateActionLocal({
      action: body,
      meta,
      tx: input.tx,
      bundles: input.bundles,
      results: {},
    });

    const ctxForStep: OpaqueEvalContext = {
      ...input.baseCtx,
      action_index: i,
    };
    const { delta, next_state } = await simulateStepLocal({
      state: pre,
      action: fullAction,
      ctx: ctxForStep,
    });

    steps.push({
      action: fullAction,
      pre_state: pre,
      verdict,
      delta,
      next_state,
    });
    pre = next_state;
  }

  return { steps, finalState: pre };
}
