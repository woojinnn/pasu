import Browser from "webextension-polyfill";
import { markPhase, captureTimeout } from "./diagnostics";
import {
  tryDeclarativeRouteV3,
  type DeclarativeRouteV3Outcome,
} from "./adapter-loader/declarative-route";
import { ensureDefaultPoliciesInstalled } from "./policies-loader";
import {
  auditAppend,
  pendingDelete,
  pendingPut,
  type PendingRequest,
} from "./storage";
import { appendVerdict, type VerdictInsert } from "./verdict-storage";
import { appendStateDelta } from "./state-delta-storage";
import { appendDiagnosisContext } from "./diagnosis-context-storage";
import {
  EngineError,
  evaluateActionV2,
  planActionRpcV2,
} from "./wasm-bridge";
import {
  dispatchCallsV2,
  formatAuditMatched,
  type PolicyRpcAuditMeta,
} from "./policy-rpc";
import {
  evaluate as scopeballEvaluate,
  getAccessToken as scopeballGetAccessToken,
  ServerError as ScopeballServerError,
} from "./scopeball-auth";
import {
  getDefaultPolicyBundlesV2,
  loadDefaultPolicySetV2,
} from "./policies-loader-v2";
import type {
  ActionBundleInputDto,
  ActionTxInputDto,
  MatchedPolicyDto,
  VerdictDto,
} from "./wasm-bridge.types";
import {
  isTransaction,
  isTypedSignature,
  isUntypedSignature,
  isVenueOrder,
  type Message,
} from "@lib/types";
import { hlOrderToAction, HL_TO_SENTINEL } from "./hl-order-to-action";
import { collectTokenDecimals } from "./registry/collect-token-decimals";
import {
  collectHlLeverage,
  noteHlLeverageUpdate,
} from "./venue/collect-hl-leverage";
import {
  normalizeTypedDataPayload,
  routeTypedSignaturePayload,
} from "./sig-routing";

/**
 * Phase 4A — submission-shape classifier. Maps the SW `Message` envelope
 * onto the `ActionNature` discriminator the v3 reducer uses:
 *
 *   - `"onchain_tx"`   ⇒ `eth_sendTransaction` (TransactionPayload).
 *     Carries the broadcast tx fields (chain, gas, value, nonce).
 *   - `"offchain_sig"` ⇒ `eth_signTypedData{,_v3,_v4}` (TypedSignaturePayload).
 *     Carries an `EIP-712` domain — verifying contract, name, chain id.
 *   - `"untyped_sig"`  ⇒ `personal_sign` / `eth_sign` (UntypedSignaturePayload).
 *     No structured domain — body falls back to `ActionBody::Unknown` in
 *     v3 because we cannot tell what the signer is approving.
 *
 * The classifier is a pure lookup. The orchestrator uses it to route into
 * the v3 WASM entry (`tryDeclarativeRouteV3`) for transactions, the
 * manifest-driven typed-data entry for EIP-712 signatures, and the legacy
 * untyped-sig short-circuit for personal_sign.
 */
export type ActionNatureKind = "onchain_tx" | "offchain_sig" | "untyped_sig";

export function classifyMessage(message: Message): ActionNatureKind {
  if (isTransaction(message)) return "onchain_tx";
  // A venue order is an off-chain signed action (agent-key signature over the
  // order, POSTed to the venue) — same nature bucket as a typed signature.
  if (isTypedSignature(message) || isVenueOrder(message)) return "offchain_sig";
  return "untyped_sig";
}

// Caps `runLifecycle`. Aligned to be a few seconds shorter than the
// proxy's PHASE1_MS so the SW always has time to post back a real verdict
// (or an `awaiting-user` extension request) before the proxy gives up.
const HARD_TIMEOUT_MS = 8_000;

interface DecisionResult {
  ok: boolean;
  verdict: VerdictDto;
}

interface DecisionOptions {
  onAwaitingUser?: () => void;
}

/**
 * v3 route audit meta. The same route result is also the input to the active
 * v2 ActionBody verdict path when the decoder emits real, non-Unknown bodies.
 */
export interface DeclarativeV3AuditMeta {
  outcome: DeclarativeRouteV3Outcome["kind"]; // "hit" | "miss" | "fault"
  nature: ActionNatureKind;
  decoder_id?: string;
  action_count?: number;
  reason?: string;
}

/**
 * Phase 1 / P3 — which pipeline produced the final verdict.
 *
 * `"declarative-v2"` ⇒ the v3 route hit with a real (non-`Unknown`)
 *   `ActionBody`, and the verdict was driven by the stateless v2 pipeline
 *   (`plan_action_rpc_v2_json` → host dispatch → `evaluate_action_v2_json`).
 *   This is the ONLY real verdict driver for transactions after the legacy
 *   declarative/static fallbacks were removed.
 * `"fail_closed"` ⇒ no decoder produced an evaluable verdict, so the engine
 *   fails closed with a warn-and-proceed verdict. Covers: a v3 route
 *   miss/fault, a v3 hit whose bodies were all `Unknown`, zero v2 bundles
 *   loaded, a v2 plan/dispatch throw, typed-signature route/evaluate misses,
 *   hard-timeout fallback, and the untyped-signature short-circuit.
 */
export type VerdictSource = "declarative-v2" | "fail_closed";

interface LifecycleResult {
  verdict: VerdictDto;
  verdictSource: VerdictSource;
  policyRpc?: PolicyRpcAuditMeta;
  /**
   * Phase 4B — v3 declarative route audit meta. Observability + (for the v2
   * verdict path) the input the verdict is driven from.
   */
  declarativeV3?: DeclarativeV3AuditMeta;
}

/**
 * Per-actor mutex chain. The read-evaluate-reserve sequence is non-atomic
 * at the storage layer; we serialize lifecycles per `actor` (lowercased)
 * by chaining promises so the second decision strictly waits for the
 * first to commit.
 */
const actorChain = new Map<string, Promise<unknown>>();

function withActorLock<T>(
  actor: string | undefined,
  fn: () => Promise<T>,
): Promise<T> {
  if (!actor) return fn();
  const key = actor.toLowerCase();
  const prev = actorChain.get(key) ?? Promise.resolve();
  const next = prev.then(
    () => fn(),
    () => fn(),
  );
  actorChain.set(
    key,
    next.finally(() => {
      // Release the slot only when this is still the most recent waiter.
      if (actorChain.get(key) === next) actorChain.delete(key);
    }),
  );
  return next;
}

export async function decideMessage(
  message: Message,
  options: DecisionOptions = {},
): Promise<DecisionResult> {
  await ensureDefaultPoliciesInstalled();
  return withActorLock(inferActor(message), () =>
    decideInner(message, options),
  );
}

function txSummaryForDiag(message: Message): Record<string, unknown> {
  const summary: Record<string, unknown> = {
    requestId: message.requestId,
    hostname: message.data.hostname,
    type: message.data.type,
  };
  if (isTransaction(message)) {
    const t = message.data.transaction;
    summary.chainId = message.data.chainId;
    summary.to = t.to;
    summary.from = t.from;
    if (typeof t.data === "string") {
      summary.selector = t.data.slice(0, 10);
      summary.dataLen = Math.max(0, Math.floor((t.data.length - 2) / 2));
    }
  }
  return summary;
}

async function decideInner(
  message: Message,
  options: DecisionOptions,
): Promise<DecisionResult> {
  logIncoming(message);
  markPhase(message.requestId, "lifecycle_start");
  const pending: PendingRequest = {
    requestId: message.requestId,
    hostname: message.data.hostname,
    type: message.data.type as PendingRequest["type"],
    bypassed: "bypassed" in message.data && !!message.data.bypassed,
    envelope: redactEnvelope(message),
    enqueuedAtMs: Date.now(),
  };
  await pendingPut(pending);

  try {
    const { result: lifecycle, timedOut } = await withTimeout(
      runLifecycle(message),
      HARD_TIMEOUT_MS,
      {
        // Deny-closed for venue orders: a timeout must BLOCK, not offer a
        // warn-and-proceed (the tx/sig paths keep the approvable warn).
        verdict: isVenueOrder(message)
          ? venueDenyVerdict("engine timeout")
          : buildTimeoutVerdict(),
        verdictSource: "fail_closed" as const,
      },
    );
    if (timedOut) {
      // Durably snapshot the in-flight fetches + phase timeline so the
      // (intermittent, not-reproducible-on-demand) 8s overrun is diagnosable
      // after the fact — even if no one was watching and the SW later evicts.
      await captureTimeout(message.requestId, txSummaryForDiag(message));
    }
    const { verdict } = lifecycle;

    let ok = false;
    // user_decision is only meaningful for WARN — PASS auto-passes and FAIL's
    // popup is informational only. WARN's `ok` boolean (trust vs. cancel/X)
    // is mapped to the storage enum and persisted on the audit row so the
    // history view can render agree/deny without round-tripping.
    let userDecision: "trusted" | "cancelled" | null = null;
    if (verdict.kind === "pass") {
      ok = true;
    } else if (verdict.kind === "fail") {
      // Surface the matched policies in a popup so the user understands
      // why the dApp's transaction returned 4001. The popup is
      // informational — Fail decisions don't take user input.
      await openVerdictWindow(
        message.requestId,
        message.data.hostname,
        verdict,
      );
    } else {
      // Warn: open the modal and await the user's Trust-and-proceed / Cancel.
      ok = await openVerdictWindowAndAwait(
        message.requestId,
        message.data.hostname,
        verdict,
        options.onAwaitingUser,
      );
      userDecision = ok ? "trusted" : "cancelled";
    }

    await appendAudit(
      message,
      pending.type,
      verdict,
      lifecycle.verdictSource,
      lifecycle.policyRpc,
      lifecycle.declarativeV3,
      userDecision,
    );
    return { ok, verdict };
  } catch (err) {
    const errInfo =
      err instanceof Error
        ? {
            name: err.name,
            message: err.message,
            kind:
              err instanceof EngineError
                ? err.kind
                : (err as { kind?: string }).kind,
            stack: err.stack,
          }
        : { raw: String(err) };
    // route_failed is a known no-op outcome (we pass it through in
    // engineErrorVerdict), so log at warn to avoid noisy red counters
    // on chrome://extensions. Other errors stay at error.
    const logAt =
      err instanceof EngineError && err.kind === "route_failed"
        ? console.warn
        : console.error;
    // Surface `to`/`chainId`/`selector` so `route_failed` logs let us
    // tell at a glance whether the unknown router was a new UR deployment,
    // an off-chain settlement contract, or a different chain entirely.
    const txCtx = isTransaction(message)
      ? {
          to: message.data.transaction.to,
          chainId: message.data.chainId,
          selector:
            typeof message.data.transaction.data === "string"
              ? message.data.transaction.data.slice(0, 10)
              : undefined,
          dataLen:
            typeof message.data.transaction.data === "string"
              ? message.data.transaction.data.length
              : undefined,
          data: message.data.transaction.data,
        }
      : undefined;
    logAt("[Scopeball] decideMessage threw", {
      requestId: message.requestId,
      hostname: message.data.hostname,
      type: pending.type,
      ...(txCtx ?? {}),
      ...errInfo,
      err,
    });
    const verdict = engineErrorVerdict(err);
    await appendAudit(message, pending.type, verdict);
    // `engineErrorVerdict` may downgrade some failures (e.g. route_failed)
    // to a pass — in that case the inpage proxy must be told to forward the
    // request to the wallet, so `ok` has to track the verdict, not be
    // hard-coded to false.
    return { ok: verdict.kind === "pass", verdict };
  } finally {
    await pendingDelete(message.requestId);
  }
}

async function appendAudit(
  message: Message,
  type: PendingRequest["type"],
  verdict: VerdictDto,
  verdictSource?: VerdictSource,
  policyRpc?: PolicyRpcAuditMeta,
  declarativeV3?: DeclarativeV3AuditMeta,
  userDecision: "trusted" | "cancelled" | null = null,
): Promise<void> {
  logDecision(message, verdict);
  await auditAppend({
    requestId: message.requestId,
    hostname: message.data.hostname,
    type,
    bypassed: "bypassed" in message.data && !!message.data.bypassed,
    verdict: verdict.kind,
    // D9: route through `formatAuditMatched` so a `__system__` match
    // keeps its policy id + reason. The dashboard reads this list as a
    // first-class verdict.
    matchedPolicies: formatAuditMatched(verdict),
    ...(policyRpc ? { policyRpc } : {}),
    ...(declarativeV3 ? { declarativeV3 } : {}),
    ...(verdictSource ? { verdictSource } : {}),
    decidedAtMs: Date.now(),
  });

  // Keep the user-facing verdict log on-device. The server returns simulated
  // state for policy evaluation; the extension owns policy verdicts and audit
  // history, so this replaces the old server `/verdicts` write path.
  void appendVerdictsForMessage(message, verdict, userDecision).catch((err) => {
    console.warn("[Scopeball] verdict-storage append failed", err);
  });
}

async function appendVerdictsForMessage(
  message: Message,
  verdict: VerdictDto,
  userDecision: "trusted" | "cancelled" | null = null,
): Promise<void> {
  const ts = Math.floor(Date.now() / 1000);
  const { contract, selector } = inferContractSelector(message);
  const base: Omit<VerdictInsert, "severity" | "policy" | "reason"> = {
    ts,
    wallet: inferActor(message)?.toLowerCase() ?? null,
    verdict: verdict.kind,
    method: inferMethod(message),
    decoded_fn: null,
    dapp_origin: message.data.hostname ?? null,
    ...(contract ? { contract } : {}),
    ...(selector ? { selector } : {}),
    // Per-decision id linking N verdict rows (one per matched policy) to a
    // single row in `state-deltas:log`. We reuse `message.requestId` (already
    // a unique UUID per request, threaded through `pendingPut` and audit) so
    // we don't generate a parallel identifier. The state-delta row is
    // populated by `recordSimulationOnServer` keyed by the same requestId;
    // rows that miss the server roundtrip leave `delta_id` set but the
    // lookup returns null — dashboard renders "no delta data" in that case.
    delta_id: message.requestId,
    ...(userDecision !== null ? { user_decision: userDecision } : {}),
  };

  if (verdict.kind === "pass" || !verdict.matched?.length) {
    await appendVerdict({
      ...base,
      severity: verdict.kind === "fail" ? "deny" : "info",
      reason: { ko: null, en: null },
    });
    return;
  }

  for (const matched of verdict.matched) {
    await appendVerdict({
      ...base,
      severity: matched.severity,
      policy: {
        id: null,
        name: matched.policy_id,
        severity: matched.severity,
      },
      reason: { ko: null, en: matched.reason ?? null },
    });
  }
}

function inferMethod(message: Message): string | null {
  if (isTransaction(message)) return "eth_sendTransaction";
  if (isTypedSignature(message)) return "eth_signTypedData_v4";
  if (isUntypedSignature(message)) return "personal_sign";
  if (isVenueOrder(message)) return `venue:${message.data.venue}`;
  return null;
}

function inferContractSelector(message: Message): {
  contract?: { addr: string; symbol: null };
  selector?: { sig: string; decoded: null };
} {
  if (isTransaction(message)) {
    const to = message.data.transaction.to?.toLowerCase();
    const data = message.data.transaction.data;
    const sig =
      typeof data === "string" && data.length >= 10 ? data.slice(0, 10) : null;
    return {
      ...(to ? { contract: { addr: to, symbol: null } } : {}),
      ...(sig ? { selector: { sig, decoded: null } } : {}),
    };
  }

  if (isTypedSignature(message)) {
    const verifyingContract = normalizeTypedDataPayload(
      message.data.typedData,
    )?.domain.verifyingContract?.toLowerCase();
    return verifyingContract
      ? { contract: { addr: verifyingContract, symbol: null } }
      : {};
  }

  return {};
}

function logIncoming(message: Message): void {
  const common = {
    requestId: message.requestId,
    hostname: message.data.hostname,
    bypassed: "bypassed" in message.data && !!message.data.bypassed,
  };

  if (isTransaction(message)) {
    const data = message.data.transaction.data;
    console.info("[Scopeball] tx.incoming", {
      ...common,
      chainId: message.data.chainId,
      to: message.data.transaction.to,
      from: message.data.transaction.from,
      value: message.data.transaction.value,
      selector: typeof data === "string" ? data.slice(0, 10) : undefined,
      dataLen: typeof data === "string" ? data.length : undefined,
      data,
    });
    return;
  }

  if (isTypedSignature(message)) {
    const typedData = normalizeTypedDataPayload(message.data.typedData);
    console.info("[Scopeball] typed-sig.incoming", {
      ...common,
      chainId: message.data.chainId,
      address: message.data.address,
      primaryType: typedData?.primaryType,
      typedData: message.data.typedData,
    });
    return;
  }

  if (isUntypedSignature(message)) {
    console.info("[Scopeball] personal-sign.incoming", {
      ...common,
      messageLen: message.data.message.length,
      message: message.data.message,
    });
  }
}

function logDecision(message: Message, verdict: VerdictDto): void {
  const matchedPolicies =
    verdict.matched?.map((m) => ({
      id: m.policy_id,
      severity: m.severity,
    })) ?? [];
  const common = {
    requestId: message.requestId,
    hostname: message.data.hostname,
    bypassed: "bypassed" in message.data && !!message.data.bypassed,
    verdict: verdict.kind,
    matchedPolicies,
  };

  if (isTransaction(message)) {
    const data = message.data.transaction.data;
    console.info("[Scopeball] tx", {
      ...common,
      chainId: message.data.chainId,
      to: message.data.transaction.to,
      selector: data?.slice(0, 10),
      dataLen: data?.length,
      data,
    });
    return;
  }

  if (isTypedSignature(message)) {
    const typedData = normalizeTypedDataPayload(message.data.typedData);
    console.info("[Scopeball] typed-sig", {
      ...common,
      chainId: message.data.chainId,
      primaryType: typedData?.primaryType,
    });
    return;
  }

  if (isUntypedSignature(message)) {
    console.info("[Scopeball] personal-sign", {
      ...common,
      messageLen: message.data.message.length,
    });
  }
}

async function runLifecycle(message: Message): Promise<LifecycleResult> {
  // Venue request (Hyperliquid `/exchange`, ...) — its own v2 path. It never
  // carries EVM calldata, so it does not go through the `tryDeclarativeRouteV3`
  // calldata branch below; instead the JSON intent is converted to
  // `ActionBody::HyperliquidCore` and evaluated by the same stateless v2
  // pipeline.
  if (isVenueOrder(message)) {
    return venueOrderLifecycle(message);
  }

  // EIP-712 typed-data signature — its own manifest-driven decode→verdict
  // path (`sig-routing.ts` → WASM `declarative_route_typed_data_v3_json`),
  // analogous to the venue-order path above: no EVM calldata, the typed-data
  // `message` is decoded into the same `Action[]` tree and driven through the
  // same stateless v2 pipeline. Routed early (before `classifyMessage` /
  // the tx branch) since it owns its own lifecycle + audit row.
  if (isTypedSignature(message)) {
    return typedSignatureLifecycle(message);
  }

  // Phase 4A classifier — `nature` lets us tell the v3 path apart at a
  // glance (audit telemetry + the upcoming v3 verdict driver). Untyped
  // sigs short-circuit just as before; the new classifier doesn't move
  // any verdict decisions, it only labels the audit row.
  const nature = classifyMessage(message);
  if (isUntypedSignature(message)) {
    return {
      verdict: unsupportedUntypedSignatureVerdict(),
      verdictSource: "fail_closed",
      // Phase 4B audit: record the v3 nature even when we short-circuit
      // so the audit log shows we *saw* a personal_sign / eth_sign tx.
      declarativeV3: {
        outcome: "miss",
        nature,
        reason: "untyped_signature_short_circuit",
      },
    };
  }

  // Phase 4B → Phase 1/P3 — v3 route. Calls the WASM v3 entry to decode the
  // tx into the PDF-FSM `Action[]` tree. After the legacy declarative/static
  // fallbacks were removed this is the SOLE input the verdict is
  // driven from (via the v2 pipeline below). Failures here must never throw
  // out of the lifecycle — we fence the call and fail closed downstream.
  let declarativeV3Meta: DeclarativeV3AuditMeta | undefined;
  // Hoisted so the Phase 1 / P2 v2 verdict branch below can read the v3
  // route outcome (its `actions[]`) after the observability logging.
  let v3Outcome: DeclarativeRouteV3Outcome | undefined;
  if (isTransaction(message)) {
    try {
      markPhase(message.requestId, "route_start");
      v3Outcome = await tryDeclarativeRouteV3({
        chainId: message.data.chainId,
        from: message.data.transaction.from ?? "0x" + "0".repeat(40),
        to: message.data.transaction.to ?? "0x" + "0".repeat(40),
        valueWei: txValueToWeiDecimal(message.data.transaction.value),
        calldataHex: message.data.transaction.data,
        submittedAt: Math.floor(Date.now() / 1000),
      });
      markPhase(message.requestId, "route_done", { outcome: v3Outcome.kind });
      declarativeV3Meta = auditFromDeclarativeV3Outcome(v3Outcome, nature);
      // DevTools audit log for the v3 route outcome. On route hits this includes
      // the decoded ActionBody tree so the service-worker log shows each
      // action's domain, tag, payload, and live_inputs.
      console.info("[Scopeball] declarative-route-v3", {
        requestId: message.requestId,
        chainId: message.data.chainId,
        outcome: v3Outcome.kind,
        nature,
        ...(v3Outcome.kind === "hit"
          ? {
              decoderId: v3Outcome.value.decoderId,
              actionCount: v3Outcome.value.actions.length,
              actions: v3Outcome.value.actions,
            }
          : v3Outcome.kind === "miss"
            ? { reason: v3Outcome.reason }
            : {
                reason: v3Outcome.reason,
                // Include fault details so decode/install errors can be traced
                // to the failing opcode, placeholder, serde path, or engine
                // error kind.
                cause:
                  v3Outcome.cause instanceof Error
                    ? {
                        name: v3Outcome.cause.name,
                        message: v3Outcome.cause.message,
                        ...("kind" in v3Outcome.cause
                          ? { kind: (v3Outcome.cause as { kind: unknown }).kind }
                          : {}),
                      }
                    : v3Outcome.cause,
              }),
      });
      // Pretty-printed dump so the decoded ActionBody[] is readable as text in
      // DevTools (the object above collapses to `[{…}]`). Hex string fields
      // (amounts/addresses) serialize cleanly — no BigInt in the v3 envelope.
      if (v3Outcome.kind === "hit") {
        console.info(
          `[Scopeball] decoded ActionBody[] (${v3Outcome.value.actions.length})\n` +
            JSON.stringify(v3Outcome.value.actions, null, 2),
        );
      }
    } catch (err) {
      console.warn("[Scopeball] declarative-route-v3 threw", {
        requestId: message.requestId,
        err: err instanceof Error ? err.message : String(err),
      });
      declarativeV3Meta = {
        outcome: "fault",
        nature,
        reason: "unexpected",
      };
    }
  } else {
    // Defensive: unreachable. Venue orders, typed signatures, and untyped
    // signatures all short-circuit above (typed sigs route through
    // `typedSignatureLifecycle`); the only remaining nature here is a
    // transaction, handled by the `if` branch. Kept fail-safe in case a new
    // message nature is added without a dedicated branch.
    declarativeV3Meta = { outcome: "miss", nature, reason: "unrouted" };
  }

  // Phase 1 / P2 — v2 (ActionBody-model) verdict path. When the v3 route HIT
  // with one or more real (non-`Unknown`) `ActionBody` elements, the
  // stateless v2 pipeline drives the verdict. This is the ONLY real verdict
  // driver after the legacy declarative/static fallbacks were removed.
  // Fail-safe: `tryV2VerdictPath` returns `undefined` (NOT a Fail verdict —
  // that is a real verdict we honour) when there is no real action, no v2
  // bundle, or a plan/dispatch throw; the lifecycle then fails closed below
  // so a flaky WASM/RPC call cannot waive a tx through.
  if (v3Outcome && v3Outcome.kind === "hit" && isTransaction(message)) {
    const v2 = await tryV2VerdictPath(message, v3Outcome.value.actions);
    if (v2) {
      console.info("[Scopeball] declarative-verdict", {
        requestId: message.requestId,
        verdictSource: "declarative-v2",
        verdict: v2.kind,
        decoderId: v3Outcome.value.decoderId,
        matched:
          v2.matched?.map((m) => ({
            id: m.policy_id,
            severity: m.severity,
          })) ?? [],
      });
      return {
        verdict: v2,
        verdictSource: "declarative-v2",
        ...(declarativeV3Meta ? { declarativeV3: declarativeV3Meta } : {}),
      };
    }
    // tryV2VerdictPath returned undefined → no real action to evaluate, no v2
    // bundle, or a plan/dispatch throw. Fall through to the fail-closed tail.
  }

  // Phase 1 / P3 — FAIL-CLOSED tail. We reach here when no decoder produced
  // an evaluable verdict:
  //   - a transaction whose v3 route missed/faulted, whose decoded bodies
  //     were all `Unknown`, had zero v2 bundles, or whose v2 plan/dispatch
  //     threw (`tryV2VerdictPath` → undefined), OR
  //   - a typed signature route/evaluate miss, OR
  //   - an unsupported untyped signature.
  // Rather than waive the request through, we emit a warn verdict that the
  // user must explicitly approve via the verdict window (mirrors the untyped
  // signature short-circuit). This replaces the deleted legacy
  // `evaluateWithPolicyRpc` fallback.
  console.info("[Scopeball] declarative-verdict", {
    requestId: message.requestId,
    verdictSource: "fail_closed",
    verdict: "warn",
    nature,
  });
  return {
    verdict: noDecoderVerdict(),
    verdictSource: "fail_closed",
    ...(declarativeV3Meta ? { declarativeV3: declarativeV3Meta } : {}),
  };
}

/**
 * Phase 1 / P2 — drive the verdict through the stateless v2 pipeline from the
 * v3 route's `actions[]`.
 *
 * For EACH action element with a real (non-`Unknown`) `ActionBody`:
 *   1. split `action = a.body`, `meta = a.meta` (the v3 `Action` shape is
 *      `{ meta, body }`, `action/mod.rs`),
 *   2. `planActionRpcV2({ manifests, action, meta, tx })` — `manifests` are the
 *      SAME `ManifestV2` list as the bundles' (`evaluate_action_v2_json`
 *      re-plans from `bundles[].manifest` and ignores any side list, so the
 *      two MUST match or the planned `call_id`s diverge and required results
 *      go missing),
 *   3. dispatch the planned calls to 127.0.0.1:8787 via `dispatchCallsV2`,
 *      yielding a fresh `{ call_id: value }` map PER action (the `call_id`
 *      `manifest_id::spec_id` repeats across actions, so a shared map would
 *      clobber),
 *   4. `evaluateActionV2({ action, meta, tx, bundles, results })` → one
 *      `VerdictDto`.
 *
 * The per-action verdicts are aggregated by deny-overrides (mirrors Rust
 * `Verdict::aggregate`: fail > warn > pass, matched lists concatenated).
 *
 * Returns `undefined` (→ caller fails closed) when:
 *   - no action element carries a real `ActionBody` (all `Unknown` / empty),
 *   - there are zero v2 bundles loaded (nothing to evaluate against), or
 *   - any `planActionRpcV2` / `dispatchCallsV2` call THROWS.
 *
 * A `Fail` / `__system__` `VerdictDto` is a REAL verdict and is returned, NOT
 * treated as a fall-through (only throws fall through). `evaluateActionV2`
 * itself never throws for policy/system faults (always `ok: true`, Fail
 * inside).
 */
/**
 * Venue-order (e.g. Hyperliquid `/exchange`) verdict lifecycle.
 *
 * Mirrors {@link tryV2VerdictPath} but sources the `ActionBody` from
 * {@link hlOrderToAction} instead of an EVM calldata decode, and is
 * **deny-closed**: unlike the tx path (which falls through to a *warn* the user
 * can approve when a decoder/plan fails), any conversion / plan / dispatch /
 * evaluate fault here resolves to a `fail` verdict. A venue order we cannot
 * fully evaluate must be blocked, not waved through.
 *
 * `pass` is still returned when no policy matched (you cannot deny without a
 * policy) and the real engine `fail` / `warn` verdicts are honoured verbatim.
 */
async function venueOrderLifecycle(message: Message): Promise<LifecycleResult> {
  const nature: ActionNatureKind = "offchain_sig";
  if (!isVenueOrder(message)) {
    // Unreachable (caller guards), but keeps the type narrow + fail-closed.
    return { verdict: venueDenyVerdict("not a venue order"), verdictSource: "fail_closed" };
  }

  let action: Record<string, unknown>;
  let meta: Record<string, unknown>;
  try {
    ({ action, meta } = hlOrderToAction(message.data));
    // Devtools: the canonical parsed representation (the `ActionBody` the policy
    // engine evaluates). Visible in the service-worker console
    // (chrome://extensions → ScopeBall → "Inspect views: service worker").
    console.info("[Scopeball] HL /exchange parsed →", {
      requestId: message.requestId,
      venue: message.data.venue,
      wireKind: message.data.hlAction?.kind,
      action,
      submitter: meta.submitter,
      submitted_at: meta.submitted_at,
    });
  } catch (err) {
    // Malformed order wire → deny-closed (do NOT let an unparseable order pass).
    console.warn("[Scopeball] venue-order convert threw", {
      requestId: message.requestId,
      err: err instanceof Error ? err.message : String(err),
    });
    return {
      verdict: venueDenyVerdict("order could not be decoded"),
      verdictSource: "fail_closed",
      declarativeV3: { outcome: "fault", nature, reason: "convert_failed" },
    };
  }

  // Ensure the v2 bundle cache is warmed before reading it. SW boot warms it
  // fire-and-forget; awaiting the idempotent loader here closes the race where
  // a venue order arriving pre-boot would see [] and (wrongly) baseline-pass a
  // policy-relevant order. The loader returns the cached set on warm calls.
  await loadDefaultPolicySetV2();
  const bundles = getDefaultPolicyBundlesV2();
  // No policies loaded ⇒ baseline pass: blocking requires an explicit deny
  // policy (matches the engine's permit-baseline). This is NOT a fault.
  if (bundles.length === 0) {
    return {
      verdict: { kind: "pass" },
      verdictSource: "declarative-v2",
      declarativeV3: { outcome: "miss", nature, reason: "no_v2_bundles" },
    };
  }
  const manifests = bundles.map((b) => b.manifest);

  const tx = {
    chain_id: "hl-mainnet",
    from: String(meta.submitter ?? HL_TO_SENTINEL),
    to: HL_TO_SENTINEL,
  } as const;
  const policyRpcUrl = process.env.POLICY_RPC_URL ?? "http://127.0.0.1:8787";

  // Best-effort venue account-state enrichment: resolve this order's effective
  // leverage (HL `activeAssetData`) so an order-leverage policy can fire — the
  // order wire carries none. `collectHlLeverage` NEVER throws and is NOT part of
  // the deny-closed fault surface below: a miss / timeout / unknown master just
  // omits the leverage (a `context has leverage` policy stays dormant) rather
  // than blocking the order. When this IS an `updateLeverage`, refresh the cache
  // (fire-and-forget) so the next order on that asset sees the just-set value.
  const account_leverage = await collectHlLeverage(action, message.data);
  void noteHlLeverageUpdate(action, message.data);

  try {
    // PLAN: HL deny conditions read base context, so the planned set is usually
    // empty (no policy-RPC). Only dispatch when there is something to fetch, so
    // the common case needs no policy-rpc server.
    const planned = await planActionRpcV2({
      manifests,
      action,
      meta,
      tx,
      account_leverage,
    });
    const results =
      planned.length > 0
        ? await dispatchCallsV2(planned, policyRpcUrl, { action, meta, tx })
        : {};
    const verdict = await evaluateActionV2({
      action,
      meta,
      tx,
      bundles,
      results,
      account_leverage,
    });
    console.info("[Scopeball] venue-order-verdict", {
      requestId: message.requestId,
      venue: message.data.venue,
      verdict: verdict.kind,
      // Injected order-time leverage (empty `{}` ⇒ enrichment dormant for this
      // order — see the `[Scopeball] HL order-leverage ...` line for why).
      account_leverage,
      matched: verdict.matched?.map((m) => ({ id: m.policy_id, severity: m.severity })) ?? [],
    });
    return {
      verdict,
      verdictSource: "declarative-v2",
      declarativeV3: {
        outcome: "hit",
        nature,
        decoder_id: "hl_order",
        action_count: 1,
      },
    };
  } catch (err) {
    // A plan/dispatch/evaluate throw is a fault → deny-closed (a flaky
    // WASM/RPC call must NOT waive a venue order through).
    console.warn("[Scopeball] venue-order-verdict threw", {
      requestId: message.requestId,
      err: err instanceof Error ? err.message : String(err),
    });
    return {
      verdict: venueDenyVerdict("policy evaluation failed"),
      verdictSource: "fail_closed",
      declarativeV3: { outcome: "fault", nature, reason: "evaluate_failed" },
    };
  }
}

// ── Off-chain signature decode logging helpers ─────────────────────────────

/** Abbreviate a long `0x` hex (address / 32-byte hash) for log readability. */
function abbrevHex(value: string): string {
  return /^0x[0-9a-fA-F]{40,}$/.test(value)
    ? `${value.slice(0, 8)}…${value.slice(-6)}`
    : value;
}

/**
 * Flatten an `ActionBody`'s scalar leaves into compact `path=value` pairs for a
 * one-line, human-readable summary. Skips the `domain` / `action` discriminants
 * (shown in the line header) and the live-input plumbing (`source` /
 * `synced_at` / `ttl`), and abbreviates long hex so the security-relevant fields
 * (spender / token / amount / deadline) read at a glance.
 */
function summarizeBodyFields(body: unknown): string {
  const pairs: string[] = [];
  const walk = (value: unknown, path: string): void => {
    if (value === null || value === undefined) return;
    if (typeof value === "object") {
      if (Array.isArray(value)) {
        value.forEach((item, i) => walk(item, `${path}[${i}]`));
        return;
      }
      for (const [key, val] of Object.entries(value as Record<string, unknown>)) {
        if (path === "" && (key === "domain" || key === "action")) continue;
        if (key === "source" || key === "synced_at" || key === "ttl") continue;
        walk(val, path ? `${path}.${key}` : key);
      }
      return;
    }
    const rendered = typeof value === "string" ? abbrevHex(value) : String(value);
    pairs.push(`${path}=${rendered}`);
  };
  walk(body, "");
  return pairs.join("  ");
}

/**
 * Unwrap a (possibly `Multicall`) `ActionBody` into its leaf bodies, so a
 * batched signature (Permit2 `PermitBatch`) summarizes one line per inner
 * permit rather than a single opaque `multicall` entry.
 */
function leafBodies(body: unknown): unknown[] {
  if (
    typeof body === "object" &&
    body !== null &&
    (body as { domain?: unknown }).domain === "multicall" &&
    Array.isArray((body as { actions?: unknown }).actions)
  ) {
    return ((body as { actions: unknown[] }).actions).flatMap(leafBodies);
  }
  return [body];
}

/**
 * Emit a signature-tailored, readable summary of a decoded off-chain payload to
 * the ScopeBall DevTools console: the EIP-712 `domain` / `primaryType` that was
 * signed, the routing decoder, and one `domain/action  field=value …` line per
 * decoded (leaf) `ActionBody`. Complements the full JSON dump.
 */
function logParsedSignature(message: Message, routed: { actions: unknown[]; decoderId: string }): void {
  const td = isTypedSignature(message)
    ? normalizeTypedDataPayload(message.data.typedData)
    : null;
  const domainName = td?.domain?.name ?? "?";
  const primaryType = td?.primaryType ?? "?";
  const leaves = routed.actions.flatMap((a) =>
    leafBodies((a as { body?: unknown }).body),
  );
  const lines = leaves.map((body, i) => {
    const b = body as { domain?: string; action?: string };
    return `  #${i} ${b?.domain ?? "?"}/${b?.action ?? "?"}  ${summarizeBodyFields(body)}`;
  });
  console.info(
    `[Scopeball] off-chain signature parsed — ${domainName} / ${primaryType} ` +
      `(${leaves.length} action${leaves.length === 1 ? "" : "s"}) via ${routed.decoderId}\n` +
      lines.join("\n"),
  );
}

/**
 * EIP-712 typed-data signature verdict lifecycle.
 *
 * Mirrors {@link venueOrderLifecycle} but sources the `Action[]` from the
 * manifest-driven typed-data router ({@link routeTypedSignaturePayload} →
 * registry `by-typed-data/` lookup → WASM `declarative_route_typed_data_v3_json`)
 * instead of an EVM calldata decode, then drives the SAME stateless v2 pipeline
 * (plan → dispatch → evaluate) the transaction path uses.
 *
 * **warn-closed** (like the tx path, NOT deny-closed like venue orders): a
 * route miss / decode-or-evaluate fault yields a `noDecoderVerdict()` warn the
 * user must approve — a benign signature we cannot decode must not be hard
 * blocked. A decoded signature with no matching policy baseline-passes.
 */
async function typedSignatureLifecycle(
  message: Message,
): Promise<LifecycleResult> {
  const nature: ActionNatureKind = "offchain_sig";
  if (!isTypedSignature(message)) {
    // Unreachable (caller guards); keep the type narrow + fail-closed.
    return { verdict: noDecoderVerdict(), verdictSource: "fail_closed" };
  }

  // Route the typed-data payload through the registry-v2 `by-typed-data/`
  // index + WASM decode. `null` = no published manifest / decode miss; a throw
  // is an unexpected fault. Both warn-close (mirrors the tx fail-closed tail).
  let routed: Awaited<ReturnType<typeof routeTypedSignaturePayload>>;
  try {
    routed = await routeTypedSignaturePayload({
      payload: message.data,
      submittedAt: Math.floor(Date.now() / 1000),
    });
  } catch (err) {
    console.warn("[Scopeball] typed-sig route threw", {
      requestId: message.requestId,
      err: err instanceof Error ? err.message : String(err),
    });
    return {
      verdict: noDecoderVerdict(),
      verdictSource: "fail_closed",
      declarativeV3: {
        outcome: "fault",
        nature,
        reason: "typed_sig_route_threw",
      },
    };
  }
  if (!routed) {
    return {
      verdict: noDecoderVerdict(),
      verdictSource: "fail_closed",
      declarativeV3: { outcome: "miss", nature, reason: "typed_sig_no_manifest" },
    };
  }

  // Off-chain signature observability: a readable per-action summary (EIP-712
  // domain / primaryType + each leaf body's security fields), then the full
  // ActionBody[] JSON dump (mirrors the tx-path dump) for complete detail.
  logParsedSignature(message, routed);
  console.info(
    `[Scopeball] decoded ActionBody[] (${routed.actions.length})\n` +
      JSON.stringify(routed.actions, null, 2),
  );
  const declarativeV3: DeclarativeV3AuditMeta = {
    outcome: "hit",
    nature,
    decoder_id: routed.decoderId,
    action_count: routed.actions.length,
  };

  // Warm the v2 bundle cache (idempotent) before reading it — closes the
  // pre-boot race where a sig arriving early sees [] and baseline-passes.
  await loadDefaultPolicySetV2();
  const bundles = getDefaultPolicyBundlesV2();
  // No policies ⇒ baseline pass (you cannot deny without a policy).
  if (bundles.length === 0) {
    return {
      verdict: { kind: "pass" },
      verdictSource: "declarative-v2",
      declarativeV3,
    };
  }
  const manifests = bundles.map((b) => b.manifest);

  // Skip `Unknown` bodies — only real ActionBody variants drive a verdict.
  const realActions = routed.actions.filter((a) => {
    const body = (a as { body?: unknown }).body;
    return (
      typeof body === "object" &&
      body !== null &&
      (body as { domain?: unknown }).domain !== "unknown"
    );
  });
  if (realActions.length === 0) {
    return {
      verdict: noDecoderVerdict(),
      verdictSource: "fail_closed",
      declarativeV3,
    };
  }

  // v2 `tx` context for a signature: `to` is the EIP-712 verifyingContract
  // (e.g. Permit2), `from` the signer. Only `{chain_id, from, to}` is consumed
  // by the WASM (`ActionTxInputDto`); `to` is NOT a trigger-match field
  // (`TriggerField` = action.domain/tag/venue + tx.chain_id), so a missing
  // verifyingContract degrades to the zero sentinel without affecting
  // action-tag-based policies.
  const verifyingContract =
    normalizeTypedDataPayload(
      message.data.typedData,
    )?.domain.verifyingContract?.toLowerCase() ??
    "0x" + "0".repeat(40);
  const tx = {
    chain_id: `eip155:${message.data.chainId}`,
    from: message.data.address,
    to: verifyingContract,
  } as const;
  const policyRpcUrl = process.env.POLICY_RPC_URL ?? "http://127.0.0.1:8787";

  const verdicts: VerdictDto[] = [];
  let anyLegThrew = false;
  for (const a of realActions) {
    const action = (a as { body: unknown }).body;
    const meta = (a as { meta?: unknown }).meta;
    try {
      // Per-child fan-out: evaluate this body and (if a multicall) each inner
      // child. No `recordSimulationOnServer` on the sig path (it never recorded).
      // Resolve token decimals once for the whole (possibly multicall) body so
      // each fungible amount gets its `amountNano` Long sibling (registry
      // lookups; non-fatal — a miss just omits that token's nano).
      const tokenDecimals = await collectTokenDecimals(
        action,
        message.data.chainId,
      );
      verdicts.push(
        ...(await evaluateBodyTree(
          action,
          meta,
          tx,
          bundles,
          manifests,
          policyRpcUrl,
          message.requestId,
          tokenDecimals,
        )),
      );
    } catch (err) {
      // A plan/dispatch/evaluate throw makes THIS leg unevaluable. Record the
      // fault but KEEP aggregating siblings — the old early-return discarded
      // every already-computed verdict and demoted a sibling leg's computed Fail
      // to an approvable warn (WASM-1). Resolution below honours deny-overrides.
      console.warn("[Scopeball] typed-sig-verdict leg threw", {
        requestId: message.requestId,
        chainId: message.data.chainId,
        err: err instanceof Error ? err.message : String(err),
      });
      anyLegThrew = true;
    }
  }

  // Deny-overrides with a fault floor (mirrors tryV2VerdictPath): a real `fail`
  // from any leg outranks a sibling fault; otherwise a fault with no computed
  // deny warn-closes (preserving the fail_closed labeling); otherwise the real
  // pass/warn aggregate stands.
  const aggregate = aggregateV2Verdicts(verdicts);
  if (aggregate.kind !== "fail" && anyLegThrew) {
    return {
      verdict: noDecoderVerdict(),
      verdictSource: "fail_closed",
      declarativeV3: { outcome: "fault", nature, reason: "evaluate_failed" },
    };
  }
  console.info("[Scopeball] typed-sig-verdict", {
    requestId: message.requestId,
    verdictSource: "declarative-v2",
    verdict: aggregate.kind,
    decoderId: routed.decoderId,
    matched:
      aggregate.matched?.map((m) => ({
        id: m.policy_id,
        severity: m.severity,
      })) ?? [],
  });
  return { verdict: aggregate, verdictSource: "declarative-v2", declarativeV3 };
}

/** Synthetic deny verdict for the venue-order deny-closed paths. */
function venueDenyVerdict(reason: string): VerdictDto {
  return {
    kind: "fail",
    matched: [
      {
        policy_id: "__venue::deny_closed",
        reason: `Venue order blocked (fail-closed): ${reason}`,
        severity: "deny",
        origin: "engine_error",
      },
    ],
  };
}

/**
 * Evaluate one decoded `ActionBody` against the installed v2 bundles, then — if
 * it is a `Multicall` — recurse into each inner child as its own evaluate
 * envelope. Returns the FLAT list of per-position verdicts (this body's, then
 * each descendant's) for the caller to aggregate by deny-overrides.
 *
 * This is the SW half of the multicall per-child fan-out (mirrors the Rust
 * `sync::actions::walk::walk_body` recursion). The WASM `evaluate_action_v2`
 * scope gate decides which bundles fire at each position: `Outer`-scoped
 * policies fire on the multicall batch here; `Inner`-scoped (default) policies
 * fire when this function re-enters with each child. Without the recursion an
 * `Inner` slippage/recipient policy would never see a UR-`execute`-wrapped swap.
 *
 * An `unknown`-domain body is skipped (NOT fail-closed) so one undecodable child
 * never blocks its siblings or the outer batch; the top-level all-unknown case
 * is still fail-closed by each caller's `realActions` guard. Children share the
 * parent `meta` (the decoded `Multicall` carries no per-child meta).
 * `recordSimulationOnServer` is intentionally NOT called here — recording stays
 * at the top-level caller so its granularity is unchanged.
 *
 * Throws only what `planActionRpcV2` / `dispatchCallsV2` throw; the caller's
 * try/catch turns that into a fail-closed verdict.
 */
async function evaluateBodyTree(
  body: unknown,
  meta: unknown,
  tx: ActionTxInputDto,
  bundles: readonly ActionBundleInputDto[],
  manifests: readonly unknown[],
  policyRpcUrl: string,
  requestId: string,
  // Host-resolved per-token decimals for the WHOLE top-level body (collected
  // once by the caller, then threaded through every child position) so each
  // fungible amount's `amountNano` Long sibling is filled in the lowering.
  tokenDecimals: Readonly<Record<string, number>>,
): Promise<VerdictDto[]> {
  const verdicts: VerdictDto[] = [];
  const domain =
    typeof body === "object" && body !== null
      ? (body as { domain?: unknown }).domain
      : undefined;

  if (domain !== undefined && domain !== "unknown") {
    // PLAN → DISPATCH (only when something is planned; a shared results map
    // would clobber since `call_id` repeats across siblings) → EVALUATE.
    const planned = await planActionRpcV2({
      manifests,
      action: body,
      meta,
      tx,
      token_decimals: tokenDecimals,
    });
    const results =
      planned.length > 0
        ? await dispatchCallsV2(planned, policyRpcUrl, { action: body, meta, tx })
        : {};
    const verdict = await evaluateActionV2({
      action: body,
      meta,
      tx,
      bundles,
      results,
      token_decimals: tokenDecimals,
    });
    verdicts.push(verdict);
    // DENY → capture the exact diagnosis context (action + materialized
    // enrichment results) so the dashboard can re-run "which clause blocked
    // this" against the real context (Option B). Best-effort, keyed by
    // requestId (= the verdict log's delta_id). Last denying leg wins.
    if (verdict.kind === "fail") {
      void appendDiagnosisContext({
        id: requestId,
        ts: Math.floor(Date.now() / 1000),
        action: body,
        meta,
        tx,
        results,
      }).catch((err) =>
        console.warn(
          "[Scopeball] diagnosis-context append failed",
          err instanceof Error ? err.message : err,
        ),
      );
    }
  } else if (domain === "unknown") {
    // N2: a nested batch position that decoded to NOTHING — the WASM decoder
    // surfaces an all-empty nested opcode stream as an `Unknown` child (rather
    // than an empty Multicall that would PASS). Contribute a warn so the parent
    // batch cannot aggregate to PASS on its legible siblings alone; a sibling
    // DENY still outranks this warn via deny-overrides. (A single dropped opcode
    // in an otherwise-decoded stream is NOT surfaced — registry `warn`=skip
    // intent — so this fires only for the all-empty case.)
    console.debug("[Scopeball] per-child unknown leg → partial-decode warn", {
      requestId,
    });
    verdicts.push(partialDecodeVerdict());
  }

  // Recurse into multicall children — each its own envelope, parent meta shared.
  // A nested multicall is evaluated as a batch (Outer policies) AND recursed.
  if (domain === "multicall") {
    const children = (body as { actions?: unknown }).actions;
    if (Array.isArray(children)) {
      for (const child of children) {
        verdicts.push(
          ...(await evaluateBodyTree(
            child,
            meta,
            tx,
            bundles,
            manifests,
            policyRpcUrl,
            requestId,
            tokenDecimals,
          )),
        );
      }
    }
  }

  return verdicts;
}

async function tryV2VerdictPath(
  message: Message,
  actions: Record<string, unknown>[],
): Promise<VerdictDto | undefined> {
  if (!isTransaction(message)) return undefined;

  // Skip `Unknown` bodies. They mean the v3 decoder could not produce a real
  // ActionBody, so the request must fall through to fail-closed handling.
  const realActions = actions.filter((a) => {
    const body = (a as { body?: unknown }).body;
    return (
      typeof body === "object" &&
      body !== null &&
      (body as { domain?: unknown }).domain !== "unknown"
    );
  });
  if (realActions.length === 0) return undefined;

  // Warm the v2 bundle cache (idempotent) before reading it — closes the
  // post-SW-restart boot race where a tx arriving early sees [] and degrades an
  // enforced deny to an approvable warn (F1-1). Mirrors the venue + typed-sig
  // paths, which already await the loader.
  await loadDefaultPolicySetV2();
  const bundles = getDefaultPolicyBundlesV2();
  if (bundles.length === 0) return undefined;
  // The plan phase MUST see the identical manifest set the bundles carry —
  // `evaluate_action_v2_json` re-plans from `bundles[].manifest`, so a
  // divergent plan-manifest list would mis-key the planned `call_id`s.
  const manifests = bundles.map((b) => b.manifest);

  // CAIP-2 string: `message.data.chainId` is a NUMBER; v2 `tx.chain_id`
  // expects `eip155:<n>` or the serde/trigger match fails.
  const tx = {
    chain_id: `eip155:${message.data.chainId}`,
    from: message.data.transaction.from ?? "0x" + "0".repeat(40),
    to: message.data.transaction.to ?? "0x" + "0".repeat(40),
  } as const;
  const policyRpcUrl = process.env.POLICY_RPC_URL ?? "http://127.0.0.1:8787";

  const verdicts: VerdictDto[] = [];
  let anyLegThrew = false;
  for (const a of realActions) {
    const action = (a as { body: unknown }).body;
    const meta = (a as { meta?: unknown }).meta;
    try {
      // Per-child fan-out: evaluate this action AND, if it is a multicall, each
      // inner child as its own envelope. The WASM scope gate (A1) decides which
      // bundles fire at the batch vs the child positions; this supplies both.
      // `evaluateActionV2` never throws for policy/system faults (Fail inside) —
      // only plan/dispatch can throw.
      // Resolve token decimals once for the whole (possibly multicall) body so
      // each fungible amount gets its `amountNano` Long sibling (registry
      // lookups; non-fatal — a miss just omits that token's nano).
      const tokenDecimals = await collectTokenDecimals(
        action,
        message.data.chainId,
      );
      verdicts.push(
        ...(await evaluateBodyTree(
          action,
          meta,
          tx,
          bundles,
          manifests,
          policyRpcUrl,
          message.requestId,
          tokenDecimals,
        )),
      );

      // RECORD (Phase 8B/8C): replay the simulation against the Scopeball
      // server so the action + state-delta land in the authenticated user's
      // server-side state. Best-effort — the verdict above is the source of
      // truth for fail-closed decisions; recording is purely for the
      // dashboard's history view. Skipped silently when the user isn't signed
      // in to Scopeball. Recorded per TOP-LEVEL action (granularity unchanged).
      // Phase 8C also captures the server's `deltas[0]` onto the SW's
      // `state-deltas:log` keyed by `message.requestId` so the verdict log's
      // `delta_id` joins without a server round-trip on the HistoryPage.
      // Calldata / value come straight off the originating tx envelope (only
      // present on transaction messages; signature paths have neither).
      const tx0 = isTransaction(message)
        ? message.data.transaction
        : undefined;
      void recordSimulationOnServer({
        action,
        meta,
        tx,
        decisionId: message.requestId,
        calldata: tx0?.data ?? "",
        value: tx0?.value ?? "0",
      });
    } catch (err) {
      // A plan/dispatch throw makes THIS leg unevaluable. Record the fault but
      // KEEP evaluating siblings — the old `return undefined` here discarded
      // every already-computed verdict and dropped the whole tx into the warn
      // tail, silently demoting a sibling leg's computed Fail to an approvable
      // warn (WASM-1). Resolution below honours deny-overrides.
      console.warn("[Scopeball] declarative-verdict-v2 leg threw", {
        requestId: message.requestId,
        chainId: message.data.chainId,
        err: err instanceof Error ? err.message : String(err),
      });
      anyLegThrew = true;
    }
  }

  // Deny-overrides resolution with a fault floor:
  //   - a real `fail` from ANY leg outranks the fault → return it (never let a
  //     sibling fault demote a computed deny — WASM-1),
  //   - otherwise a fault with no computed deny falls through to the caller's
  //     fail-closed WARN tail (`undefined`), preserving the prior fail_closed
  //     semantics/labeling for a pure plan/dispatch fault,
  //   - otherwise the real pass/warn aggregate stands.
  const aggregate = aggregateV2Verdicts(verdicts);
  if (aggregate.kind === "fail") return aggregate;
  if (anyLegThrew) return undefined;
  return aggregate;
}

/**
 * Aggregate per-action [`VerdictDto`]s by deny-overrides — a faithful TS
 * mirror of Rust `Verdict::aggregate` (`policy/verdict.rs`): concatenate every
 * verdict's `matched` list, then `fail` if any verdict failed, else `warn` if
 * any warned, else `pass`. An empty input aggregates to `pass`.
 */
function aggregateV2Verdicts(verdicts: VerdictDto[]): VerdictDto {
  const matched: MatchedPolicyDto[] = [];
  let anyFail = false;
  let anyWarn = false;
  for (const v of verdicts) {
    if (v.kind === "fail") {
      anyFail = true;
      matched.push(...v.matched);
    } else if (v.kind === "warn") {
      anyWarn = true;
      matched.push(...v.matched);
    }
  }
  if (anyFail) return { kind: "fail", matched };
  if (anyWarn) return { kind: "warn", matched };
  return { kind: "pass" };
}

/**
 * Phase 4B — map a v3 outcome into the audit shape.
 */
function auditFromDeclarativeV3Outcome(
  outcome: DeclarativeRouteV3Outcome,
  nature: ActionNatureKind,
): DeclarativeV3AuditMeta {
  if (outcome.kind === "hit") {
    return {
      outcome: "hit",
      nature,
      decoder_id: outcome.value.decoderId,
      action_count: outcome.value.actions.length,
    };
  }
  if (outcome.kind === "miss") {
    return { outcome: "miss", nature, reason: outcome.reason };
  }
  return { outcome: "fault", nature, reason: outcome.reason };
}

/**
 * Convert a `0x…` hex wei value (the wallet RPC convention) to a base-10
 * decimal string the engine expects in `ctx.value_wei`. Empty / undefined
 * defaults to "0".
 */
function txValueToWeiDecimal(value: string | undefined): string {
  if (!value) return "0";
  if (value.startsWith("0x") || value.startsWith("0X")) {
    const hex = value.slice(2);
    if (hex.length === 0) return "0";
    try {
      return BigInt("0x" + hex).toString(10);
    } catch {
      return "0";
    }
  }
  // Already decimal (uncommon for wallet RPC but tolerated).
  return value;
}

function inferActor(message: Message): string | undefined {
  if (isTransaction(message)) return message.data.transaction.from;
  if (isTypedSignature(message)) return message.data.address;
  return undefined;
}

function redactEnvelope(message: Message): unknown {
  if (isTransaction(message)) {
    return {
      to: message.data.transaction.to,
      chainId: message.data.chainId,
      value: message.data.transaction.value,
    };
  }
  if (isTypedSignature(message)) {
    const typedData = normalizeTypedDataPayload(message.data.typedData);
    return {
      primaryType: typedData?.primaryType,
      verifyingContract: typedData?.domain.verifyingContract,
    };
  }
  if (isVenueOrder(message)) {
    const a = message.data.hlAction;
    // Redacted audit envelope: action kind + venue only. For order legs include
    // side/reduceOnly; NEVER persist destination addresses or amounts for the
    // fund-movement actions.
    return {
      venue: message.data.venue,
      action: a.kind,
      ...(a.kind === "order"
        ? {
            symbol: message.data.symbol,
            side: a.order.b ? "long" : "short",
            reduceOnly: a.order.r ?? false,
          }
        : {}),
    };
  }
  return {};
}

function buildTimeoutVerdict(): VerdictDto {
  return {
    kind: "warn",
    matched: [
      {
        policy_id: "__engine::timeout",
        reason: `Engine took longer than ${HARD_TIMEOUT_MS}ms`,
        severity: "warn",
        origin: "engine_error",
      },
    ],
  };
}

function unsupportedUntypedSignatureVerdict(): VerdictDto {
  return {
    kind: "warn",
    matched: [
      {
        policy_id: "__engine::unsupported_untyped_signature",
        reason: "Untyped signatures cannot be fully evaluated yet",
        severity: "warn",
        origin: "engine_error",
      },
    ],
  };
}

/**
 * Phase 1 / P3 — FAIL-CLOSED verdict for a request no decoder could evaluate.
 *
 * Emitted by the `runLifecycle` tail when the v3 route missed/faulted, decoded
 * only `Unknown` bodies, found no v2 bundles, the v2 pipeline threw, or typed /
 * untyped signature routing could not produce an evaluable result. `kind: "warn"`
 * so `decideInner` opens the verdict window and requires
 * the user to explicitly proceed (mirrors `unsupportedUntypedSignatureVerdict`),
   * rather than silently waiving the request through as the deleted legacy
   * `evaluateWithPolicyRpc` fallback would have.
 */
/**
 * N2/N3 — floor for a decoded batch carrying a leg the decoder could not map
 * (surfaced by the WASM decoder as an `Unknown` child rather than dropped). A
 * partially-decoded batch must not PASS on its legible siblings alone, so this
 * warn floors the aggregate; a sibling DENY still outranks it (deny-overrides).
 */
function partialDecodeVerdict(): VerdictDto {
  return {
    kind: "warn",
    matched: [
      {
        policy_id: "__engine::partial_decode",
        reason:
          "Part of this batch could not be decoded — review before signing",
        severity: "warn",
        origin: "engine_error",
      },
    ],
  };
}

function noDecoderVerdict(): VerdictDto {
  return {
    kind: "warn",
    matched: [
      {
        policy_id: "__engine::no_decoder",
        reason: "Transaction type not recognized by any installed decoder",
        severity: "warn",
        origin: "engine_error",
      },
    ],
  };
}

function engineErrorVerdict(err: unknown): VerdictDto {
  const kind = err instanceof EngineError ? err.kind : "unexpected";
  const message = err instanceof Error ? err.message : String(err);
  // Calls the engine has no registered adapter for (e.g. Permit2.approve,
  // unknown routers) carry no policy-relevant signal — let them through
  // instead of blocking everything outside the engine's known set.
  if (kind === "route_failed") {
    return { kind: "pass" };
  }
  return {
    kind: "fail",
    matched: [
      {
        policy_id: `__engine::${kind}`,
        reason: message,
        severity: "deny",
        origin: "engine_error",
      },
    ],
  };
}

async function withTimeout<T>(
  p: Promise<T>,
  ms: number,
  fallback: T,
): Promise<{ result: T; timedOut: boolean }> {
  let timedOut = false;
  const timeoutPromise = new Promise<{ result: T; timedOut: true }>((resolve) =>
    setTimeout(() => {
      timedOut = true;
      resolve({ result: fallback, timedOut: true });
    }, ms),
  );
  const wrapped = p.then((result) => ({ result, timedOut }));
  return Promise.race([wrapped, timeoutPromise]);
}

const PENDING_DECISION_KEY = "requests:pending-decisions";

/**
 * Open the verdict modal as a separate Chrome window. Caller is informational —
 * use this for Fail verdicts where the user has nothing to decide; the popup
 * just explains why the dApp's request returned 4001.
 */
async function openVerdictWindow(
  requestId: string,
  hostname: string,
  verdict: VerdictDto,
): Promise<void> {
  const url = buildConfirmUrl(requestId, hostname, verdict);
  try {
    await Browser.windows.create({
      url,
      type: "popup",
      width: 480,
      height: 640,
      focused: true,
    });
  } catch (err) {
    console.error("[Scopeball] openVerdictWindow failed", {
      requestId,
      hostname,
      verdict: verdict.kind,
      urlLength: url.length,
      err,
    });
  }
}

/**
 * Open the verdict modal and await the user's choice. Used for Warn
 * verdicts. Survives SW restart via storage.session decision durability.
 */
async function openVerdictWindowAndAwait(
  requestId: string,
  hostname: string,
  verdict: VerdictDto,
  onAwaitingUser?: () => void,
): Promise<boolean> {
  const all =
    ((
      (await Browser.storage.session.get(PENDING_DECISION_KEY)) as Record<
        string,
        unknown
      >
    )[PENDING_DECISION_KEY] as
      | Record<string, { verdict: VerdictDto; status: string; ok?: boolean }>
      | undefined) ?? {};
  all[requestId] = { verdict, status: "awaiting" };
  await Browser.storage.session.set({ [PENDING_DECISION_KEY]: all });

  const url = buildConfirmUrl(requestId, hostname, verdict);
  let win: Browser.Windows.Window;
  try {
    win = await Browser.windows.create({
      url,
      type: "popup",
      width: 480,
      height: 640,
      focused: true,
    });
  } catch {
    return false;
  }

  return new Promise<boolean>((resolve) => {
    let settled = false;
    let pollHandle: ReturnType<typeof setInterval> | undefined;

    const settle = (ok: boolean): void => {
      if (settled) return;
      settled = true;
      Browser.runtime.onMessage.removeListener(messageListener);
      Browser.windows.onRemoved.removeListener(closeListener);
      if (pollHandle !== undefined) clearInterval(pollHandle);
      // Best-effort window cleanup (may already be closed).
      if (win.id !== undefined) {
        Browser.windows.remove(win.id).catch(() => {});
      }
      resolve(ok);
    };

    const messageListener = (msg: unknown): void => {
      const m = msg as {
        type?: string;
        requestId?: string;
        ok?: boolean;
      } | null;
      if (!m || m.type !== "scopeball:verdict-decision") return;
      if (m.requestId !== requestId) return;
      settle(!!m.ok);
    };
    const closeListener = (closedId: number): void => {
      if (closedId === win.id) settle(false);
    };
    Browser.runtime.onMessage.addListener(messageListener);
    Browser.windows.onRemoved.addListener(closeListener);

    // Backup poll for the persisted decision in case the runtime message
    // drops during a SW death window. 5-min deadline so we don't run
    // forever if the user walked away.
    const POLL_DEADLINE_MS = 5 * 60_000;
    const pollDeadline = Date.now() + POLL_DEADLINE_MS;
    pollHandle = setInterval(async () => {
      if (Date.now() > pollDeadline) {
        settle(false);
        return;
      }
      const fresh =
        ((
          (await Browser.storage.session.get(PENDING_DECISION_KEY)) as Record<
            string,
            unknown
          >
        )[PENDING_DECISION_KEY] as
          | Record<string, { status: string; ok?: boolean }>
          | undefined) ?? {};
      const rec = fresh[requestId];
      if (rec?.status === "decided") settle(!!rec.ok);
    }, 250);

    // Phase-2 timeout heartbeat: extend the inpage stream's 3s phase-1
    // timer so the user has time to read and decide.
    onAwaitingUser?.();
  });
}

function buildConfirmUrl(
  requestId: string,
  hostname: string,
  verdict: VerdictDto,
): string {
  const params = new URLSearchParams({
    requestId,
    hostname,
    verdict: JSON.stringify(verdict),
  });
  return Browser.runtime.getURL(`confirm.html?${params.toString()}`);
}

/**
 * Phase 8B — replay the just-evaluated simulation against the Scopeball
 * Rust server so the action + state-delta land in the authenticated
 * user's server-side state.
 *
 * Best-effort: failures are logged but never affect the WASM verdict.
 * Silent skip when the user isn't signed in (no JWT in chrome.storage).
 *
 * The server takes the same `(envelopes, eval_context, wallet_id)` triple
 * the SW already prepared for `dispatchCallsV2`; we wrap it into the
 * REST DTO shape (`POST /evaluate`) the server expects. The returned
 * `policyRequest` is discarded — WASM remains the verdict source.
 *
 * `wallet_id.chains` defaults to the single tx chain; richer wallet-level
 * chain sets land when the dashboard's wallet-management UI starts
 * driving the server's `POST /wallets`.
 */
async function recordSimulationOnServer(input: {
  readonly action: unknown;
  readonly meta: unknown;
  readonly tx: {
    readonly chain_id: string;
    readonly from: string;
    readonly to: string;
  };
  /** `message.requestId` — re-used as both the server's idempotency key
   *  and as the local state-delta row id (so the verdict log's
   *  `delta_id` joins cleanly). When `undefined`, no local capture
   *  happens (the only existing call site already passes it). */
  readonly decisionId?: string;
  /** Raw `0x`-prefixed calldata from the originating tx. Persisted on
   *  the local state-delta row so the HistoryPage's "다시 시뮬"
   *  button can hand it to `/simulation?…calldata=…` without re-decoding. */
  readonly calldata?: string;
  /** `msg.value` as a base-10 decimal string. Optional same as calldata. */
  readonly value?: string;
}): Promise<void> {
  // Skip silently for signed-out users — recording is opt-in via login.
  const hasToken = await scopeballGetAccessToken().catch(() => null);
  if (!hasToken) return;

  // Mirror the Rust `EvaluateRequest` shape:
  //   - wallet_id: from tx.from + tx.chain_id
  //   - envelopes: the typed action wrapped as { meta, body } (server
  //                accepts an opaque array; reducer dispatches on body.domain)
  //   - eval_context: minimal — must match the server's `EvalContext` field
  //                names + enum variants (camelCase `request_kind`, snake_case
  //                `simulation`, REQUIRED `action_index`); a mismatch makes the
  //                server reject with 422 and the record silently no-ops.
  //   - call_specs: empty (enrichment is rewritten LiveField-first per
  //                Phase 8B; server-side dispatcher remains intentionally
  //                unimplemented)
  const envelope = { meta: input.meta, body: input.action };
  const evalContext = {
    chain: input.tx.chain_id,
    now: Math.floor(Date.now() / 1000),
    action_index: 0,
    request_kind: "transaction",
    simulation: "preview",
  };
  const walletId = {
    address: input.tx.from,
    chains: [input.tx.chain_id],
  };

  try {
    const response = await scopeballEvaluate({
      wallet_id: walletId,
      envelopes: [envelope as unknown as Record<string, unknown>],
      eval_context: evalContext,
      call_specs: [],
    });

    // Local capture (Phase 8C): the server already folded the reducer
    // delta into `policyRequest.deltas`; we lift the first one onto the
    // SW's `state-deltas:log` ring buffer so the dashboard's HistoryPage
    // can render it without an extra server round-trip. Single-action
    // txs (the common case) produce one delta; multi-action multicall
    // batches surface only the FIRST action's delta here — fuller
    // per-leg storage lands when the verdict log carries an action index.
    if (input.decisionId) {
      try {
        const policyRequest = (response as { policyRequest?: unknown })
          .policyRequest;
        const deltasRaw =
          policyRequest &&
          typeof policyRequest === "object" &&
          !Array.isArray(policyRequest)
            ? (policyRequest as { deltas?: unknown }).deltas
            : undefined;
        const firstDelta = Array.isArray(deltasRaw) ? deltasRaw[0] : undefined;
        if (firstDelta !== undefined) {
          await appendStateDelta({
            id: input.decisionId,
            ts: Math.floor(Date.now() / 1000),
            chain: input.tx.chain_id,
            from: input.tx.from,
            to: input.tx.to,
            calldata: input.calldata ?? "",
            value: input.value ?? "0",
            delta: firstDelta,
          });
        }
      } catch (storageErr) {
        // Local storage failure shouldn't poison the audit/verdict path
        // — the server has the canonical delta, dashboard just won't
        // join until a future record succeeds.
        console.warn(
          "[Scopeball] state-delta local append failed",
          storageErr instanceof Error ? storageErr.message : storageErr,
        );
      }
    }
  } catch (err) {
    if (err instanceof ScopeballServerError && err.isUnauthorized) {
      // Token expired between getAccessToken() and the call — swallow.
      console.debug("[Scopeball] record skipped: server returned 401");
      return;
    }
    console.warn("[Scopeball] record on server failed (non-fatal)", {
      chain: input.tx.chain_id,
      from: input.tx.from,
      err: err instanceof Error ? err.message : String(err),
    });
  }
}
