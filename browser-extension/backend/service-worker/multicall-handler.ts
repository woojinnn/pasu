/**
 * Multicall handler (TS side).
 *
 * Accepts the WASM v3 route result for Universal-Router calls, normalises the
 * `actions` array shape, and exposes helpers to inspect a `Multicall` body.
 * The WASM owns all calldata decoding — this module is a thin pass-through.
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
 * type every variant (token / amm / lending / ...) because this layer
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
   * action node. Both shapes are kept side-by-side so the audit
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
   * Nested-multicall depth observed in the actions list. The reducer supports
   * unbounded depth; the registry manifest caps it at `recurse.max_depth = 3`.
   */
  maxDepth: number;
}

/**
 * Process a `DeclarativeRouteV3Outcome` into a normalised handler result.
 *
 * - `kind === "hit"` → walks the actions list, identifies any `Multicall`
 *   actions, and produces a flat children list for downstream consumers.
 * - `kind === "miss"` / `"fault"` → returns `null`.
 *
 * Pure data pass-through: no WASM call, no manifest lookup.
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

