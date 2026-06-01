/**
 * Phase A.1 — EIP-712 typed-data signature router (SW side), manifest-driven.
 *
 * Maps an `eth_signTypedData{,_v3,_v4}` request onto the v3 `Action` tree the
 * orchestrator hands downstream, by looking the manifest up in the registry-v2
 * `by-typed-data/` index on the routing triple
 * `(chainId, verifyingContract, primaryType)` and decoding it through the
 * WASM `declarative_route_typed_data_v3_json` pipeline.
 *
 *   match.typed_data → index key (build-index.ts Task 2):
 *     `<chainId>__<verifyingContract>__<primaryType>`
 *   (verifyingContract lowercased; `:` in primaryType escaped to `__` in the
 *   filename only — see `typedDataUrl` in registry/client.ts).
 *
 * `domain.name` is NOT part of the key: EIP-2612 token Permits carry the
 * token name there (e.g. "USD Coin"), so it can't disambiguate. It is passed
 * to the WASM for audit / display only.
 *
 * Flow (replaces the Phase 4C per-protocol hardcode):
 *   1. Extract the triple from `typedData.domain` + `primaryType`.
 *   2. `installDeclarativeBundleV3ByTypedData` — fetch the manifest via the
 *      `by-typed-data/` index + install into the WASM typed_data bridge. A
 *      miss (no publisher / fetch fault) returns `{ ok: false }` → router
 *      returns `null` (orchestrator preserves the observability-only row).
 *   3. `declarativeRouteTypedDataV3` — decode the manifest emit-rules over the
 *      typed-data `message`. The WASM owns all `$args.*` substitution and the
 *      numeric coercion (`amount` / `nonce` / `deadline`) that the deleted
 *      Phase 4C per-protocol hardcode used to do by hand.
 *
 * The orchestrator wiring (replacing the legacy typed-sig path with this
 * async router) is Task 6 — this module only exposes the async entries.
 */

import type { TypedSignaturePayload } from "@lib/types";
import { declarativeRouteTypedDataV3 } from "./wasm-bridge";
import { installDeclarativeBundleV3ByTypedData } from "./adapter-loader/declarative-adapter-loader";

// ───────────────────────────────────────────────────────────────────────────
// EIP-712 typed-data shape
// ───────────────────────────────────────────────────────────────────────────

/**
 * Minimal EIP-712 typed-data shape we consume. We DO NOT validate the full
 * `types` table here — the registry-v2 manifest install does that and the
 * WASM decode reuses it. The router only needs the domain triple
 * (`chainId`, `verifyingContract`) and the `primaryType` discriminator to
 * pick a manifest, plus `message` (handed to the WASM decode verbatim).
 *
 * `message` is `unknown` because each protocol carries a different payload
 * shape; the WASM manifest decode narrows it. We deliberately keep this
 * portable across wallet libraries (viem / ethers / metamask) where field
 * nesting is stable but value types vary (string vs bigint for `uint256`).
 */
export interface EIP712TypedData {
  domain: {
    name?: string;
    version?: string;
    chainId?: number | string;
    verifyingContract?: string;
    salt?: string;
  };
  primaryType: string;
  types: Record<string, Array<{ name: string; type: string }>>;
  message: unknown;
}

/** Permit2 contract address — CREATE2-deterministic; same on every chain. */
export const PERMIT2_ADDRESS = "0x000000000022d473030f116ddee9f6b43ac78ba3";

// ───────────────────────────────────────────────────────────────────────────
// v3 Action skeleton (subset matching `policy_transition::action`)
// ───────────────────────────────────────────────────────────────────────────

/**
 * v3 `ActionMeta.nature.OffchainSig` payload. Mirrors the Rust enum
 * variant tagged with `"kind": "offchain_sig"`. We keep
 * `verifying_contract` as the canonical lowercase form so the audit
 * surface and Cedar policies don't have to case-fold.
 */
export interface OffchainSigNature {
  kind: "offchain_sig";
  domain: {
    name: string;
    version?: string;
    chain_id?: number;
    verifying_contract?: string;
    salt?: string;
  };
  deadline: number; // unix seconds — `sigDeadline` for Permit2
  nonce_key?: {
    kind: "permit2_nonce_bitmap";
    word: string; // base-10 decimal U256
    bit: number;
  };
}

/**
 * Permit2-shaped `TokenAction::Permit2SignAllowance`. Mirrors the Rust
 * variant tagged `"action": "permit2_sign_allowance"`. The body is
 * intentionally flat (no LiveField on `nonce`) for Phase 4C — Phase 4D
 * upgrades `nonce` to a LiveField pair `(word, bit)` once the Sync
 * orchestrator is wired.
 */
export interface Permit2SignAllowanceBody {
  domain: "token";
  token: {
    action: "permit2_sign_allowance";
    permit2_sign_allowance: {
      token: { key: { standard: "erc20"; chain: string; address: string } };
      spender: string;
      amount: string;
      expires_at: number;
      sig_deadline: number;
      nonce: string;
    };
  };
}

export interface SigRouterAction {
  meta: {
    submitted_at: number;
    submitter: string;
    nature: OffchainSigNature;
  };
  body: Permit2SignAllowanceBody;
  // Decoder id — manifest the router matched on. Empty when no
  // manifest matched (caller treats as a miss).
  decoder_id: string;
}

// ───────────────────────────────────────────────────────────────────────────
// Public entry
// ───────────────────────────────────────────────────────────────────────────

/**
 * Router result — the decoded v3 `Action` list plus the matched bundle's
 * declarative decoder id. Mirrors the on-chain `declarativeRouteRequestV3`
 * result (`actions` + `decoder_id`) so the orchestrator's downstream handling
 * is uniform across the call path and the sign path.
 */
export interface TypedDataRouteResult {
  actions: unknown[];
  decoderId: string;
}

/**
 * Phase A.1 — typed-data router entry (async, manifest-driven).
 *
 * Returns the decoded `Action` list when the request matches a manifest in
 * the `by-typed-data/` index AND the WASM decode succeeds, or `null` for a
 * miss (no manifest, or decode failure — orchestrator falls through to the
 * observability-only audit row).
 *
 * Match is strict — the triple must match a published manifest exactly. A
 * dApp-supplied `verifyingContract` / `primaryType` with a subtle typo misses
 * on purpose; we never fuzzy-match a benign signature onto a high-trust body.
 */
export async function routeTypedData(args: {
  typedData: EIP712TypedData;
  submitter: string;
  submittedAt?: number;
}): Promise<TypedDataRouteResult | null> {
  const chainId = parseDomainChainId(args.typedData.domain.chainId);
  const verifyingContract =
    args.typedData.domain.verifyingContract?.toLowerCase();
  const primaryType = args.typedData.primaryType;
  if (chainId === null || !verifyingContract || !primaryType) {
    return null;
  }

  // T1 — derive the optional `witness_type` 4th routing-key component. Permit2
  // `permitWitnessTransferFrom` payloads (UniswapX intent orders etc.) all
  // share `(chainId, Permit2, "PermitWitnessTransferFrom")`; the actual order
  // type is the EIP-712 `witness` field's type inside types[primaryType]. We
  // locate it by the field NAMED "witness" — this is the Permit2 witness
  // convention (the field is always named `witness` in IPermit2's
  // `PermitWitnessTransferFrom`). Absent → `undefined` (the 3-tuple key, every
  // non-witness manifest).
  const witnessType = extractWitnessType(args.typedData, primaryType);

  // Thread witnessType into BOTH the install/fetch key and the WASM route
  // below. The install path builds the `by-typed-data/` index URL from this
  // key (`typedDataUrl`): without witnessType the live SW would fetch the
  // 3-segment file and 404 against build-index's 4-segment witness file, so
  // the manifest would never install and the WASM route would miss. The key
  // is spread conditionally (omitting `witnessType` rather than setting it to
  // `undefined`) for `exactOptionalPropertyTypes` — omission keeps the URL +
  // install cache key byte-identical to the pre-T1 3-tuple for non-witness
  // payloads.
  const installed = await installDeclarativeBundleV3ByTypedData({
    chainId,
    verifyingContract,
    primaryType,
    ...(witnessType !== undefined ? { witnessType } : {}),
  });
  if (!installed.ok) {
    return null;
  }

  const result = await declarativeRouteTypedDataV3({
    chainId,
    verifyingContract,
    primaryType,
    witnessType,
    domainName: args.typedData.domain.name,
    message: args.typedData.message,
    submitter: args.submitter,
    submittedAt: args.submittedAt ?? Math.floor(Date.now() / 1000),
  });
  if (!result.ok || !result.data) {
    return null;
  }

  return { actions: result.data.actions, decoderId: result.data.decoder_id };
}

// ───────────────────────────────────────────────────────────────────────────
// Helpers
// ───────────────────────────────────────────────────────────────────────────

/**
 * T1 — extract the EIP-712 `witness` field's struct type from
 * `types[primaryType]`, used as the optional 4th routing-key component to
 * de-collide Permit2 `permitWitnessTransferFrom` payloads.
 *
 * Convention-based: it finds the field literally NAMED `"witness"`. This is
 * the IPermit2 `PermitWitnessTransferFrom` convention — the witness struct is
 * always the field named `witness`. Returns `undefined` when there is no such
 * field (every non-witness payload), keeping the WASM bridge key a 3-tuple.
 *
 * Kept VERBATIM (the exact EIP-712 type name) — the WASM compares it without
 * lowercasing, exactly like `primaryType`.
 */
function extractWitnessType(
  typedData: EIP712TypedData,
  primaryType: string,
): string | undefined {
  const fields = typedData.types?.[primaryType];
  if (!Array.isArray(fields)) return undefined;
  const witness = fields.find((f) => f && f.name === "witness");
  return typeof witness?.type === "string" && witness.type.length > 0
    ? witness.type
    : undefined;
}

/**
 * Wallets serialise `domain.chainId` inconsistently: viem keeps it a number,
 * others send the EIP-712 raw as a hex or decimal string. Normalise to a
 * plain `number`; return `null` for shapes we can't safely interpret (caller
 * misses).
 */
function parseDomainChainId(raw: number | string | undefined): number | null {
  if (raw === undefined) return null;
  if (typeof raw === "number") return Number.isFinite(raw) ? raw : null;
  if (typeof raw === "string") {
    if (raw.startsWith("0x") || raw.startsWith("0X")) {
      try {
        const n = Number.parseInt(raw, 16);
        return Number.isFinite(n) ? n : null;
      } catch {
        return null;
      }
    }
    const n = Number.parseInt(raw, 10);
    return Number.isFinite(n) ? n : null;
  }
  return null;
}

// ───────────────────────────────────────────────────────────────────────────
// Convenience adapter — TypedSignaturePayload → routeTypedData()
// ───────────────────────────────────────────────────────────────────────────

/**
 * Orchestrator-facing helper (async). Pulls the typed-data payload out of the
 * SW `Message` envelope and forwards to {@link routeTypedData}. Lives here so
 * the orchestrator stays agnostic of the EIP-712 shape.
 */
export async function routeTypedSignaturePayload(args: {
  payload: TypedSignaturePayload;
  submittedAt?: number;
}): Promise<TypedDataRouteResult | null> {
  const td = args.payload.typedData as EIP712TypedData | undefined;
  if (!td || typeof td !== "object") return null;
  // EIP-712 typed-data carries `domain`/`primaryType`/`types`/`message`;
  // anything missing → caller misses.
  if (!td.domain || !td.primaryType || !td.types) return null;
  return routeTypedData({
    typedData: td,
    submitter: args.payload.address,
    ...(args.submittedAt !== undefined
      ? { submittedAt: args.submittedAt }
      : {}),
  });
}
