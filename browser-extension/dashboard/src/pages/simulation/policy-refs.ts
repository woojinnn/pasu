/**
 * Heuristic policy ↔ state-key linking.
 *
 * Cedar policies don't (yet) declare which state fields they read, so we
 * infer that from the policy name. When the policy-state reducer exposes
 * a provenance map (`policy_id → [state_key]`), drop this and consume
 * the real refs.
 *
 * The keys returned line up with the `key` field on rows in `state-mock.ts`
 * (or a coarse prefix like `tokens` / `positions`). The UI uses those to
 * highlight the rows that triggered a violation.
 */
import type { SimState } from "./state-mock";

export interface PolicyRefs {
  /** Concrete row keys that the policy is implicated in. */
  rowKeys: string[];
  /** Coarse buckets when no specific row matches — e.g. all positions. */
  buckets: Array<"tokens" | "positions" | "nfts" | "wallet">;
}

export function inferPolicyRefs(
  policyName: string,
  state: SimState,
): PolicyRefs {
  const n = policyName.toLowerCase();
  const rowKeys: string[] = [];
  const buckets: PolicyRefs["buckets"] = [];

  if (n.includes("slippage") || n.includes("swap")) {
    buckets.push("tokens");
  }
  if (n.includes("health") || n.includes("hf") || n.includes("ltv") || n.includes("lend") || n.includes("borrow")) {
    buckets.push("positions");
    for (const p of state.positions) rowKeys.push(p.key);
  }
  if (n.includes("allowlist") || n.includes("whitelist") || n.includes("recipient") || n.includes("transfer")) {
    buckets.push("tokens");
  }
  if (n.includes("sanction") || n.includes("blocked") || n.includes("compliance")) {
    buckets.push("wallet");
  }
  if (n.includes("unknown") || n.includes("unverified")) {
    for (const t of state.tokens) if (t.unknown) rowKeys.push(t.key);
    buckets.push("tokens");
  }
  if (n.includes("stale") || n.includes("price")) {
    for (const t of state.tokens) if (t.stale) rowKeys.push(t.key);
    buckets.push("tokens");
  }
  return { rowKeys, buckets };
}
