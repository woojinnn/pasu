/**
 * EIP-712 typed-data signature router (SW side), manifest-driven.
 *
 * Maps an `eth_signTypedData{,_v3,_v4}` request onto the v3 `Action` tree by
 * looking the manifest up in the `by-typed-data/` registry index on the routing
 * triple `(chainId, verifyingContract, primaryType)` and decoding through
 * `declarative_route_typed_data_v3_json`.
 *
 * `domain.name` is NOT part of the routing key — EIP-2612 token Permits carry the
 * token name there and it collides across tokens. It is passed to WASM for display only.
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
 * Permit2-shaped `TokenAction::Permit2SignAllowance`. Mirrors the Rust variant
 * tagged `"action": "permit2_sign_allowance"`.
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
 * Typed-data router entry (async, manifest-driven).
 *
 * Returns the decoded `Action` list when the request matches a manifest in the
 * `by-typed-data/` index AND the WASM decode succeeds, or `null` on a miss.
 * Match is strict — the triple must match a published manifest exactly; we never
 * fuzzy-match a benign signature onto a high-trust body.
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

  // Derive the optional `witness_type` 4th routing-key component.
  // Permit2 `permitWitnessTransferFrom` payloads all share the same 3-tuple
  // `(chainId, Permit2, "PermitWitnessTransferFrom")`; the actual order type is
  // the EIP-712 `witness` field's struct type, located by field name "witness"
  // (IPermit2 convention). Absent → `undefined` (3-tuple key for non-witness payloads).
  const witnessType = extractWitnessType(args.typedData, primaryType);

  // Thread witnessType into both the install/fetch key and the WASM route.
  // Spread conditionally so omission keeps the URL and cache key byte-identical
  // to the 3-tuple form for non-witness payloads.
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
 * Extract the EIP-712 `witness` field's struct type from `types[primaryType]`,
 * used as the optional 4th routing-key component to de-collide Permit2
 * `permitWitnessTransferFrom` payloads. Locates the field named `"witness"`.
 * Returns `undefined` for non-witness payloads. Value is kept verbatim —
 * the WASM compares without lowercasing.
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
  const td = normalizeTypedDataPayload(args.payload.typedData);
  if (!td) return null;
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

export function normalizeTypedDataPayload(raw: unknown): EIP712TypedData | null {
  if (typeof raw === "string") {
    try {
      const parsed: unknown = JSON.parse(raw);
      return normalizeTypedDataPayload(parsed);
    } catch {
      return null;
    }
  }
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  return raw as EIP712TypedData;
}
