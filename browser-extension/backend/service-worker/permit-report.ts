/**
 * Backend tracking (Phase 3): report decoded off-chain permit / permit2
 * signatures to the policy-server so it records each outstanding signed permit
 * as a `PendingTx` (the sync reconciler later closes the lifecycle).
 *
 * This is purely additive observability — it never participates in the verdict
 * and never blocks signing. The caller (`typedSignatureLifecycle`) invokes
 * {@link reportPermitIfApplicable} fire-and-forget AFTER a PASS verdict.
 *
 * Coverage (decided, v1): extension-scoped + partial. Gated on a signed-in
 * server session — permits signed while signed-out are uncapturable and are
 * NOT buffered. Mobile / out-of-extension permits are inherently untrackable.
 * Decoded params only — the raw EIP-712 signature is never sent.
 *
 * Kept in its own module (no heavy static imports) so it stays cheap to unit
 * test and decoupled from the WASM-bridge-laden orchestrator.
 */

import { isTypedSignature, type Message } from "@lib/types";

import type { IngestPermitReq } from "./pasu-auth/client";

/** True when a signed-in server session token is present (lazy polyfill load). */
async function permitReportIsAuthed(): Promise<boolean> {
  try {
    const { getAccessToken } = await import("./pasu-auth/tokenStore");
    return (await getAccessToken()) != null;
  } catch {
    return false;
  }
}

/**
 * Pull a `LiveField`'s scalar value: a decoded `nonce` field deserializes as
 * `{ value, source, synced_at }`. Returns the `.value` (or the raw value if it's
 * already a scalar, defensively).
 */
function liveFieldValue(field: unknown): unknown {
  if (typeof field === "object" && field !== null && "value" in field) {
    return (field as { value: unknown }).value;
  }
  return field;
}

const asStr = (v: unknown): string | undefined =>
  typeof v === "string" ? v : typeof v === "number" ? String(v) : undefined;
const asNum = (v: unknown): number | undefined =>
  typeof v === "number" ? v : typeof v === "string" ? Number(v) : undefined;

/**
 * Map one decoded `token`-domain permit/permit2 `ActionBody` to the server's
 * `IngestPermitReq`, or `null` if the body is not a permit/permit2 signature
 * action. Field names mirror the Rust action structs (tsify serde names):
 * `erc20_permit` → `{token, spender, amount, deadline, nonce: LiveField<U256>}`;
 * `permit2_sign_allowance` → adds `expires_at`/`sig_deadline` + `nonce:
 * LiveField<[word, bit]>`; `permit2_sign_transfer` → adds `owner`/`witness_type`.
 */
export function permitBodyToIngestReq(
  body: unknown,
  chainId: number,
): IngestPermitReq | null {
  if (typeof body !== "object" || body === null) return null;
  const b = body as Record<string, unknown>;
  if (b.domain !== "token") return null;
  const chain_id = `eip155:${chainId}`;
  const tokenAddr = (b.token as { key?: { address?: unknown } } | undefined)?.key
    ?.address;
  const token = typeof tokenAddr === "string" ? tokenAddr : undefined;

  if (b.action === "erc20_permit") {
    const spender = asStr(b.spender);
    const amount = asStr(b.amount);
    const deadline = asNum(b.deadline);
    const nonce = asStr(liveFieldValue(b.nonce));
    if (
      !token ||
      !spender ||
      amount === undefined ||
      deadline === undefined ||
      nonce === undefined
    ) {
      return null;
    }
    return { kind: "eip2612", token, spender, amount, deadline, nonce, chain_id };
  }

  // Both permit2 sign variants carry a `(word, bit)` bitmap nonce as a 2-tuple.
  const noncePair = liveFieldValue(b.nonce);
  const word = Array.isArray(noncePair) ? asStr(noncePair[0]) : undefined;
  const bit = Array.isArray(noncePair) ? asNum(noncePair[1]) : undefined;

  if (b.action === "permit2_sign_allowance") {
    const spender = asStr(b.spender);
    const amount = asStr(b.amount);
    const expires_at = asNum(b.expires_at);
    const sig_deadline = asNum(b.sig_deadline);
    if (
      !token ||
      !spender ||
      amount === undefined ||
      expires_at === undefined ||
      sig_deadline === undefined ||
      word === undefined ||
      bit === undefined
    ) {
      return null;
    }
    return {
      kind: "permit2_allowance",
      token,
      spender,
      amount,
      expires_at,
      sig_deadline,
      nonce_word: word,
      nonce_bit: bit,
      chain_id,
    };
  }

  if (b.action === "permit2_sign_transfer") {
    const owner = asStr(b.owner);
    const spender = asStr(b.spender);
    const amount = asStr(b.amount);
    const sig_deadline = asNum(b.sig_deadline);
    const witness_type =
      typeof b.witness_type === "string" ? b.witness_type : null;
    if (
      !token ||
      !owner ||
      !spender ||
      amount === undefined ||
      sig_deadline === undefined ||
      word === undefined ||
      bit === undefined
    ) {
      return null;
    }
    return {
      kind: "permit2_transfer",
      token,
      owner,
      spender,
      amount,
      sig_deadline,
      nonce_word: word,
      nonce_bit: bit,
      witness_type,
      chain_id,
    };
  }

  return null;
}

/**
 * Unwrap a (possibly `Multicall`) decoded `ActionBody` into its leaf bodies, so a
 * batched signature (Permit2 `PermitBatch`) yields one permit per inner leaf.
 */
function leafBodies(body: unknown): unknown[] {
  if (
    typeof body === "object" &&
    body !== null &&
    (body as { domain?: unknown }).domain === "multicall" &&
    Array.isArray((body as { actions?: unknown }).actions)
  ) {
    return (body as { actions: unknown[] }).actions.flatMap(leafBodies);
  }
  return [body];
}

/**
 * After a PASS verdict on a permit/permit2 typed signature, fire-and-forget the
 * decoded params to the server. Best-effort and non-blocking:
 *   - GATED on a signed-in server session (`isAuthed`) — signed-out permits are
 *     uncapturable in v1 (accepted partial coverage); no buffering.
 *   - only fires for the three permit/permit2 sign actions; other signatures and
 *     non-token bodies are ignored (no POST).
 *   - never throws / never blocks signing — transport / auth errors are swallowed.
 */
export async function reportPermitIfApplicable(
  routedActions: readonly unknown[],
  message: Message,
): Promise<void> {
  if (!isTypedSignature(message)) return;
  const chainId = message.data.chainId;
  const address = message.data.address;

  // Decoded permit bodies (unwrap a Permit2 `PermitBatch` multicall into leaves).
  const reqs = routedActions
    .flatMap((a) => leafBodies((a as { body?: unknown }).body))
    .map((body) => permitBodyToIngestReq(body, chainId))
    .filter((r): r is IngestPermitReq => r !== null);
  if (reqs.length === 0) return;

  if (!(await permitReportIsAuthed())) return;

  try {
    const { ingestPermit } = await import("./pasu-auth/client");
    for (const req of reqs) {
      try {
        await ingestPermit(address, req);
      } catch (err) {
        // Best-effort: a single permit's ingest failure must not abort the rest
        // or surface to the signing flow.
        console.warn("[Pasu] permit ingest failed (swallowed)", {
          requestId: message.requestId,
          kind: req.kind,
          err: err instanceof Error ? err.message : String(err),
        });
      }
    }
  } catch (err) {
    // Importing the client (browser-only polyfill) can throw in odd contexts.
    console.warn("[Pasu] permit report skipped (client load failed)", {
      err: err instanceof Error ? err.message : String(err),
    });
  }
}
