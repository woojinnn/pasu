/**
 * Phase 4D — Multicall handler scaffold (TS side).
 *
 * Wires the SW orchestrator into the v3 `declarative_route_request_v3_json`
 * WASM entry for Universal-Router (UR) `execute(bytes commands, bytes[] inputs, uint256 deadline)`
 * calls. The handler's surface area is intentionally narrow at Phase 4D:
 *
 *   1. SW receives an `eth_sendTransaction` with calldata starting at
 *      UR's `execute` selector (`0x3593564c`).
 *   2. Orchestrator calls `tryDeclarativeRouteV3` (already wired in Phase 4B).
 *   3. WASM emits a v3 `Action[]` — Phase 4B returns a single
 *      `ActionBody::Unknown` stub. Phase 4D-full will replace the stub
 *      with a real children list (one per UR opcode) wrapped in
 *      `ActionBody::Multicall { actions: [...] }`.
 *   4. This handler validates and surfaces the children for the Cedar
 *      pipeline (or for the upcoming v3 verdict path).
 *
 * Scope split (auto-mode decision):
 *
 * | Layer | Responsibility | Phase |
 * |---|---|---|
 * | TS multicall-handler  | Validate `actions` shape, flatten for audit,
 *                          surface child counts                          | 4D (this file)  |
 * | WASM declarative_route_request_v3_json | Manifest lookup + emit.body
 *                          decode + per_opcode_body recurse              | 4D.5 / Phase 5  |
 * | adapters-v3 (Rust)    | declarative parser: `$args.*` substitution,
 *                          `recurse.strategy: "commands_and_inputs_paired"`,
 *                          per_opcode dispatch                           | 4D.5 / Phase 5  |
 *
 * Phase 4D (this file) stays a thin TS pass-through: it accepts the
 * WASM v3 result, normalises the `actions` array shape, and exposes
 * helpers to inspect a `Multicall` body. The TS handler does NOT
 * decode UR calldata locally — that would duplicate the Rust
 * declarative parser. The WASM remains the single source of truth.
 */

import type { DeclarativeRouteV3Outcome } from "./adapter-loader/declarative-route";

// ───────────────────────────────────────────────────────────────────────────
// v3 ActionBody shapes — minimal subset consumed by the handler.
// Full schema lives in `policy_transition::action`. We don't redeclare
// every variant here; the handler only inspects `domain` discriminator
// and `actions` recursion for Multicall.
// ───────────────────────────────────────────────────────────────────────────

/**
 * Minimal `ActionBody::Multicall` shape. The Rust enum carries
 * `actions: Vec<ActionBody>` flattened under `#[serde(tag = "domain")]`,
 * so on the wire the JSON looks like
 *
 *   { "domain": "multicall", "actions": [ <body>, <body>, ... ] }
 *
 * The handler uses this as a structural probe — anything matching
 * `{domain: "multicall", actions: any[]}` is treated as the parent of
 * a children list.
 */
export interface MulticallActionBody {
  domain: "multicall";
  actions: ActionBodyLike[];
}

/**
 * Body discriminator the handler needs to distinguish. We don't strongly
 * type every variant (token / amm / lending / ...) because Phase 4D
 * only cares about the multicall recursion shape; the rest pass
 * through opaquely.
 */
export type ActionBodyLike =
  | MulticallActionBody
  | { domain: "unknown"; target: string; chain: string; calldata: string; value: string }
  | { domain: string; [k: string]: unknown };

/** v3 wire shape — `meta` + `body`. */
export interface V3ActionLike {
  meta: Record<string, unknown>;
  body: ActionBodyLike;
}

// ───────────────────────────────────────────────────────────────────────────
// Handler API
// ───────────────────────────────────────────────────────────────────────────

export interface MulticallHandlerResult {
  /**
   * v3 actions surfaced from the WASM result. Always a `V3ActionLike[]`
   * — single-emit bundles return one element; multicall_recurse
   * bundles return a `[{ body: { domain: "multicall", actions: [...] }}]`
   * action node. Phase 4D keeps both shapes side-by-side so the audit
   * surface doesn't have to choose.
   */
  actions: V3ActionLike[];
  /**
   * Flattened children — leaves of any nested `Multicall` actions,
   * in DFS order. Empty when the root is a single-emit (non-multicall)
   * action. Used by the upcoming Cedar v3 pipeline to evaluate each
   * child action independently.
   */
  flattened: ActionBodyLike[];
  /** Outer decoder id (UR/SR02/NFPM bundle), forwarded verbatim. */
  decoderId: string;
  /**
   * Nested-multicall depth observed in the actions list. Always 0 for
   * the Phase 4B stub (it emits `Unknown` only) and ≤ 1 for the
   * Phase 4D-full UR `execute` case. The reducer's `apply_multicall`
   * already supports unbounded depth (`Multicall { actions: Vec<Self> }`),
   * but Phase 4D caps observed depth at `recurse.max_depth = 3`
   * matching the registry manifest.
   */
  maxDepth: number;
}

/**
 * Phase 4D — process a `DeclarativeRouteV3Outcome` from
 * `tryDeclarativeRouteV3` into a normalised handler result.
 *
 * - `kind === "hit"` → walks the actions list, identifies any
 *   `Multicall` actions, and produces a flat children list for
 *   downstream consumers.
 * - `kind === "miss"` / `"fault"` → returns `null`; orchestrator falls
 *   through to the static path (Phase 5 makes this the verdict
 *   driver).
 *
 * The handler is a pure-data pass-through: no WASM call, no manifest
 * lookup. The heavy lifting (manifest decode, emit.body substitution,
 * per_opcode recurse) lives behind the v3 WASM entry — Phase 4D-full
 * replaces the stub there, which automatically flows actions through
 * this handler unchanged.
 */
export function handleV3Outcome(
  outcome: DeclarativeRouteV3Outcome,
): MulticallHandlerResult | null {
  if (outcome.kind !== "hit") return null;
  const actionsRaw = outcome.value.actions as unknown as V3ActionLike[];
  if (!Array.isArray(actionsRaw)) {
    return {
      actions: [],
      flattened: [],
      decoderId: outcome.value.decoderId,
      maxDepth: 0,
    };
  }
  const flattened: ActionBodyLike[] = [];
  let maxDepth = 0;
  for (const action of actionsRaw) {
    const depth = walkBody(action.body, flattened);
    if (depth > maxDepth) maxDepth = depth;
  }
  return {
    actions: actionsRaw,
    flattened,
    decoderId: outcome.value.decoderId,
    maxDepth,
  };
}

/**
 * DFS over an `ActionBody`-like tree. Leaves (non-multicall bodies)
 * are appended to `out`; `Multicall` actions are descended into.
 * Returns the maximum nesting depth observed (0 for a leaf).
 *
 * The walker is structural — it relies only on the `domain ===
 * "multicall"` discriminator and the `actions` field. Any future
 * `ActionBody` variant that introduces nested actions would need an
 * explicit case here.
 */
function walkBody(body: ActionBodyLike, out: ActionBodyLike[]): number {
  if (isMulticallBody(body)) {
    let maxChildDepth = 0;
    for (const child of body.actions) {
      const childDepth = walkBody(child, out);
      if (childDepth > maxChildDepth) maxChildDepth = childDepth;
    }
    return maxChildDepth + 1;
  }
  out.push(body);
  return 0;
}

function isMulticallBody(body: ActionBodyLike): body is MulticallActionBody {
  return (
    typeof body === "object" &&
    body !== null &&
    (body as { domain?: unknown }).domain === "multicall" &&
    Array.isArray((body as { actions?: unknown }).actions)
  );
}

// ───────────────────────────────────────────────────────────────────────────
// Pending follow-ups (Phase 4D.5 / Phase 5)
// ───────────────────────────────────────────────────────────────────────────
//
// 1. WASM `declarative_route_request_v3_json` (in
//    `crates/policy-engine-wasm/src/declarative_exports.rs`) currently
//    emits a single `ActionBody::Unknown` stub. Replace with:
//      a. callkey lookup against the registry-v2 bridge (`DECLARATIVE_STATE`).
//      b. manifest.emit.body `$args.*` / `$chain` / `$resolved.*`
//         substitution into a typed v3 ActionBody.
//      c. `recurse.strategy === "commands_and_inputs_paired"`:
//         decode `commands` (bytes) + `inputs` (bytes[]); for each
//         opcode_byte, look up `per_opcode_body[opcode]`; recurse with
//         the inner `inputs[i]` as the substitution scope; wrap children
//         in `ActionBody::Multicall { actions: Vec<ActionBody> }`.
//
// 2. Sync orchestrator wire-up: gas_price `LiveField` currently uses a
//    stub Pyth source; Phase 5 replaces with the real Sync layer feed.
//
// 3. Permit2 `nonce_key` follow-through: the typed-sig path emits
//    `nonce_key: undefined` today. Once Phase 4D wires
//    `Permit2.nonceBitmap(owner, word)` reads through Sync, the typed-sig
//    router can attach the LiveField pair.
//
// 4. EIP-2612 / UniswapX typed-data manifests: registry-v2 needs the
//    typed_data index keyed by `(verifyingContract, primaryType)` (not
//    `domain.name`, which collides across EIP-2612 tokens). Today only
//    Permit2 / PermitSingle is keyed.
