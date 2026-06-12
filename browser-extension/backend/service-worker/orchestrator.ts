import Browser from "webextension-polyfill";
import { markPhase, captureTimeout } from "./diagnostics";
import {
  tryDeclarativeRouteV3,
  type DeclarativeRouteV3Outcome,
} from "./adapter-loader/declarative-route";
import {
  auditAppend,
  pendingDelete,
  pendingPut,
  type PendingRequest,
} from "./storage";
import { appendVerdict, type VerdictInsert } from "./verdict-storage";
import { refreshBadge } from "./mascot-badge";
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
  evaluate as pasuEvaluate,
  getAccessToken as pasuGetAccessToken,
  ServerError as PasuServerError,
} from "./pasu-auth";
import { getCurrentUserId } from "./dashboard/current-user";
import {
  collectActionMetas,
  defRefForPolicyId,
  filterForAction,
  isWalletRegistered,
  resolveBundlesForWallet,
} from "./policy-store/resolve";
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
import { reportPermitIfApplicable } from "./permit-report";
import { collectTokenDecimals } from "./registry/collect-token-decimals";
import {
  collectHlLeverage,
  noteHlLeverageUpdate,
} from "./venue/collect-hl-leverage";
import { collectOrderEnrichment } from "./venue/collect-order-enrichment";
import { resolveOrderSymbol } from "./venue/resolve-order-symbol";
import { resolveHlMaster } from "./venue/resolve-hl-master";
import {
  normalizeTypedDataPayload,
  routeTypedSignaturePayload,
} from "./sig-routing";

/**
 * Submission-shape classifier. Maps the SW `Message` envelope onto the
 * `ActionNature` discriminator:
 *
 *   - `"onchain_tx"`   ⇒ `eth_sendTransaction` (TransactionPayload)
 *   - `"offchain_sig"` ⇒ `eth_signTypedData{,_v3,_v4}` (TypedSignaturePayload)
 *   - `"untyped_sig"`  ⇒ `personal_sign` / `eth_sign` (UntypedSignaturePayload)
 *     — body falls back to `ActionBody::Unknown` because there is no structured
 *     domain to decode.
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
  /** 표시 전용 advisory 사이드이펙트. 라이브 위험(fail|warn) verdict 에
   *  fire-and-forget 으로 호출. `ok`/결정 흐름·confirm 창을 절대 건드리지 않고
   *  `pasu:verdict-decision` 도 발신하지 않는다. 구현(데스크톱 알림)은 호출자
   *  (index.ts)가 소유 — orchestrator 는 추상 이벤트만 방출(순환 import 회피). */
  onRiskyVerdict?: (args: {
    scenario: "tx" | "approval";
    title?: string | undefined;
    message?: string | undefined;
  }) => void;
}

/**
 * Audit metadata for the v3 declarative route. Also serves as the input
 * for the v2 ActionBody verdict path when the decoder emits real, non-Unknown bodies.
 */
export interface DeclarativeV3AuditMeta {
  outcome: DeclarativeRouteV3Outcome["kind"]; // "hit" | "miss" | "fault"
  nature: ActionNatureKind;
  decoder_id?: string;
  action_count?: number;
  reason?: string;
}

/**
 * Which pipeline produced the final verdict.
 *
 * `"declarative-v2"` ⇒ the v3 route hit with a real (non-`Unknown`) `ActionBody`,
 *   verdict driven by `plan_action_rpc_v2_json` → host dispatch → `evaluate_action_v2_json`.
 * `"fail_closed"` ⇒ no decoder produced an evaluable verdict (v3 miss/fault,
 *   all-Unknown bodies, zero v2 bundles, v2 plan/dispatch throw, typed-sig route miss,
 *   hard-timeout, or untyped-sig short-circuit) — engine warns and requires user approval.
 */
export type VerdictSource = "declarative-v2" | "fail_closed";

interface LifecycleResult {
  verdict: VerdictDto;
  verdictSource: VerdictSource;
  policyRpc?: PolicyRpcAuditMeta;
  /**
   * v3 declarative route audit metadata. Used for observability and, on the v2
   * verdict path, as the input the verdict is driven from.
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

    // 표시 전용 advisory 데스크톱 알림 — 라이브 위험(fail|warn) verdict 에서만.
    // fire-and-forget: `ok`·confirm 창·`pasu:verdict-decision` 채널 어디에도
    // 영향을 주지 않는다. decideMessage 는 index.ts 의 라이브 호출부 한 곳에서만
    // 불리고(시뮬레이션 핸들러는 여기 도달 안 함) onRiskyVerdict 콜백도 그곳에서만
    // 주입되므로, 시뮬레이션 경로에선 구조적으로 발사되지 않는다.
    // 한 decideMessage(= 한 지갑 요청) = 알림 1건 — appendVerdict 의 정책별 N회
    // 루프와 달리 verdict 객체 기준이라 중복 없음.
    if (verdict.kind === "fail" || verdict.kind === "warn") {
      try {
        const matched = verdict.matched[0];
        const scenario = matched?.origin === "action" ? "approval" : "tx";
        const addr = inferContractSelector(message).contract?.addr;
        const reason = matched?.reason ?? null;
        options.onRiskyVerdict?.({
          scenario,
          // 둘 다 optional — 없으면 시나리오 기본 카피로 폴백.
          message:
            reason ??
            (addr
              ? `상호작용한 주소(${addr})가 위험 목록과 일치해요.`
              : undefined),
        });
      } catch {
        /* advisory 전용 — 알림 준비 실패가 결정에 영향 주지 않게 */
      }
    }

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
    logAt("[Pasu] decideMessage threw", {
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

/**
 * TEST-ONLY verdict tap. When `chrome.storage.local["pasu_e2e_tap"]` is set,
 * writes one row per venue verdict keyed by requestId — captured at
 * decision-time (before any warn modal), so a high-volume policy e2e can read
 * every verdict uniformly. Additive, default-off, prod no-op.
 */
async function tapVenueVerdict(
  requestId: string,
  master: string | null,
  verdict: VerdictDto,
): Promise<void> {
  try {
    const flag = (await Browser.storage.local.get("pasu_e2e_tap")) as Record<
      string,
      unknown
    >;
    if (!flag["pasu_e2e_tap"]) return;
    // Key by the resolved MASTER (the e2e gives each case a unique synthetic
    // master), not requestId — identical order bodies (differing only by
    // vaultAddress) hash to the SAME requestId, which would clobber the row.
    await Browser.storage.local.set({
      [`pasu:e2e-tap:${master ?? requestId}`]: {
        master: master ?? null,
        requestId,
        kind: verdict.kind,
        matched: verdict.matched?.map((m) => m.policy_id) ?? [],
        ts: Date.now(),
      },
    });
  } catch {
    /* best-effort test instrumentation — never affects the verdict */
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
    // Route through `formatAuditMatched` so a `__system__` match keeps its
    // policy id + reason; the dashboard reads this list as a first-class verdict.
    matchedPolicies: formatAuditMatched(verdict),
    ...(policyRpc ? { policyRpc } : {}),
    ...(declarativeV3 ? { declarativeV3 } : {}),
    ...(verdictSource ? { verdictSource } : {}),
    decidedAtMs: Date.now(),
  });

  // Keep the user-facing verdict log on-device. The server returns simulated
  // state for policy evaluation; the extension owns policy verdicts and audit
  // history, so this replaces the old server `/verdicts` write path.
  //
  // After the verdict row is written, refresh the toolbar mascot badge so it
  // reflects the latest 24h fail/warn count (safe → warn → fail). Chained
  // after the append so `countVerdicts` sees this verdict; best-effort.
  void appendVerdictsForMessage(message, verdict, userDecision)
    .then(() => refreshBadge())
    .catch((err) => {
      console.warn("[Pasu] verdict-storage append / badge refresh failed", err);
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
    // Reuse `message.requestId` as the per-decision id so N verdict rows (one
    // per matched policy) link to the same `state-deltas:log` row. Rows that
    // miss the server round-trip leave `delta_id` set but the lookup returns
    // null — dashboard renders "no delta data" in that case.
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

  // 매칭된 정책의 def 참조를 박제 — 이름 변경/삭제 후에도 과거 기록이 유효하다. best-effort.
  const uid = (await getCurrentUserId()) ?? "anonymous";
  const refCache = new Map<string, { defId: string; displayName: string } | null>();
  for (const matched of verdict.matched) {
    let ref = refCache.get(matched.policy_id);
    if (ref === undefined) {
      ref = await defRefForPolicyId(uid, matched.policy_id).catch(() => null);
      refCache.set(matched.policy_id, ref);
    }
    await appendVerdict({
      ...base,
      severity: matched.severity,
      policy: {
        id: null,
        name: matched.policy_id,
        severity: matched.severity,
        def_id: ref?.defId ?? null,
        display_name: ref?.displayName ?? null,
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
    console.info("[Pasu] tx.incoming", {
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
    console.info("[Pasu] typed-sig.incoming", {
      ...common,
      chainId: message.data.chainId,
      address: message.data.address,
      primaryType: typedData?.primaryType,
      typedData: message.data.typedData,
    });
    return;
  }

  if (isUntypedSignature(message)) {
    console.info("[Pasu] personal-sign.incoming", {
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
    console.info("[Pasu] tx", {
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
    console.info("[Pasu] typed-sig", {
      ...common,
      chainId: message.data.chainId,
      primaryType: typedData?.primaryType,
    });
    return;
  }

  if (isUntypedSignature(message)) {
    console.info("[Pasu] personal-sign", {
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

  const nature = classifyMessage(message);
  if (isUntypedSignature(message)) {
    return {
      verdict: unsupportedUntypedSignatureVerdict(),
      verdictSource: "fail_closed",
      declarativeV3: {
        outcome: "miss",
        nature,
        reason: "untyped_signature_short_circuit",
      },
    };
  }

  // v3 route: call the WASM entry to decode the tx into the `Action[]` tree.
  // This is the sole input the verdict is driven from (via the v2 pipeline below).
  // Failures must never throw out of the lifecycle — fenced and fails closed downstream.
  let declarativeV3Meta: DeclarativeV3AuditMeta | undefined;
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
      console.info("[Pasu] declarative-route-v3", {
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
                // Include fault details so decode/install errors can be traced.
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
      // Pretty-printed dump so the decoded ActionBody[] is human-readable in DevTools.
      if (v3Outcome.kind === "hit") {
        console.info(
          `[Pasu] decoded ActionBody[] (${v3Outcome.value.actions.length})\n` +
            JSON.stringify(v3Outcome.value.actions, null, 2),
        );
      }
    } catch (err) {
      console.warn("[Pasu] declarative-route-v3 threw", {
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
    // Defensive: unreachable — all non-transaction natures short-circuit above.
    // Kept fail-safe for future message types added without a dedicated branch.
    declarativeV3Meta = { outcome: "miss", nature, reason: "unrouted" };
  }

  // v2 (ActionBody-model) verdict path. When the v3 route hits with one or more
  // real (non-`Unknown`) `ActionBody` elements, the stateless v2 pipeline drives
  // the verdict. `tryV2VerdictPath` returns `undefined` (NOT a Fail verdict) when
  // there is no real action, no v2 bundle, or a plan/dispatch throw; the lifecycle
  // then fails closed below so a flaky WASM/RPC call cannot waive a tx through.
  let v2Fault: EngineError | null = null;
  if (v3Outcome && v3Outcome.kind === "hit" && isTransaction(message)) {
    const v2 = await tryV2VerdictPath(message, v3Outcome.value.actions);
    if (v2.verdict) {
      console.info("[Pasu] declarative-verdict", {
        requestId: message.requestId,
        verdictSource: "declarative-v2",
        verdict: v2.verdict.kind,
        decoderId: v3Outcome.value.decoderId,
        matched:
          v2.verdict.matched?.map((m) => ({
            id: m.policy_id,
            severity: m.severity,
          })) ?? [],
      });
      return {
        verdict: v2.verdict,
        verdictSource: "declarative-v2",
        ...(declarativeV3Meta ? { declarativeV3: declarativeV3Meta } : {}),
      };
    }
    // No verdict → no real action to evaluate, no v2 bundle, or a
    // plan/dispatch/evaluate throw (carried in `fault`). Fall through to the
    // fail-closed tail, surfacing the explicit engine error when there is one.
    v2Fault = v2.fault ?? null;
  }

  // FAIL-CLOSED tail: no decoder produced an evaluable verdict. Emit a warn
  // verdict requiring explicit user approval rather than waiving the request through.
  console.info("[Pasu] declarative-verdict", {
    requestId: message.requestId,
    verdictSource: "fail_closed",
    verdict: "warn",
    nature,
    ...(v2Fault ? { engineError: { kind: v2Fault.kind, message: v2Fault.message } } : {}),
  });
  return {
    verdict: evaluateFaultVerdict(v2Fault),
    verdictSource: "fail_closed",
    ...(declarativeV3Meta ? { declarativeV3: declarativeV3Meta } : {}),
  };
}

/**
 * Venue-order (e.g. Hyperliquid `/exchange`) verdict lifecycle.
 *
 * Sources the `ActionBody` from {@link hlOrderToAction} and is **deny-closed**:
 * any conversion / plan / dispatch / evaluate fault resolves to a `fail` verdict.
 * A venue order we cannot fully evaluate must be blocked, not waved through.
 * `pass` is returned only when no policy matched.
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
    console.info("[Pasu] HL /exchange parsed →", {
      requestId: message.requestId,
      venue: message.data.venue,
      wireKind: message.data.hlAction?.kind,
      action,
      submitter: meta.submitter,
      submitted_at: meta.submitted_at,
    });
  } catch (err) {
    // Malformed order wire → deny-closed (do NOT let an unparseable order pass).
    console.warn("[Pasu] venue-order convert threw", {
      requestId: message.requestId,
      err: err instanceof Error ? err.message : String(err),
    });
    return {
      verdict: venueDenyVerdict("order could not be decoded"),
      verdictSource: "fail_closed",
      declarativeV3: { outcome: "fault", nature, reason: "convert_failed" },
    };
  }

  const tx = {
    chain_id: "hl-mainnet",
    from: String(meta.submitter ?? HL_TO_SENTINEL),
    to: HL_TO_SENTINEL,
  } as const;

  // 주문 제출자 지갑의 effective 바인딩을 가져온다. HL 주문은 sentinel
  // submitter(`meta.submitter`)로 들어오므로 그대로 쓰면 미등록 지갑 →
  // defaults.enabled 전역 폴백이 걸려 per-wallet 토글이 무시된다. fetch-hook이
  // `eth_accounts`에서 읽어 payload에 stamp한 연결 master(또는 vaultAddress /
  // per-origin store)를 해석해 그 지갑의 effective 바인딩으로 평가한다 →
  // 대시보드의 per-policy 토글이 HL 주문에도 적용된다. 해석 실패 시 sentinel로
  // degrade(= 기존 defaults.enabled 폴백, best-effort, never throws).
  const venueUid = (await getCurrentUserId()) ?? "anonymous";
  const master = await resolveHlMaster(message.data);
  const evalAddress = master ?? tx.from;
  const resolved = await resolveBundlesForWallet(venueUid, evalAddress);
  // 진단: registered=false 면 evalAddress가 미등록 → per-wallet 토글이 아니라
  // defaults.enabled 전역 폴백으로 평가된 것(= 토글이 이 주문에 안 먹은 경우).
  // master=null(주소 미해석)이나 venueUid="anonymous"(미로그인)면 대개 여기로 샌다.
  const registered = await isWalletRegistered(venueUid, evalAddress);
  console.info("[Pasu] HL venue policy-set resolved", {
    requestId: message.requestId,
    venueUid,
    master,
    evalAddress,
    registered,
    bundleCount: resolved.length,
  });
  // 액션-단위 사전 필터(최적화) — 정밀 게이트는 엔진의 trigger 매칭.
  const bundles = filterForAction(resolved, collectActionMetas(action)).map(
    ({ policy, manifest }) => ({ policy, manifest }),
  );
  // No policies loaded ⇒ baseline pass: blocking requires an explicit deny policy.
  if (bundles.length === 0) {
    return {
      verdict: { kind: "pass" },
      verdictSource: "declarative-v2",
      declarativeV3: { outcome: "miss", nature, reason: "no_v2_bundles" },
    };
  }
  const manifests = bundles.map((b) => b.manifest);
  const policyRpcUrl = process.env.POLICY_RPC_URL ?? "http://127.0.0.1:8787";

  // Best-effort venue account-state enrichment: resolve this order's effective
  // leverage (HL `activeAssetData`) so an order-leverage policy can fire — the
  // order wire carries none. `collectHlLeverage` NEVER throws and is NOT part of
  // the deny-closed fault surface below: a miss / timeout / unknown master just
  // omits the leverage (a `context has leverage` policy stays dormant) rather
  // than blocking the order. When this IS an `updateLeverage`, refresh the cache
  // (fire-and-forget) so the next order on that asset sees the just-set value.
  // Two best-effort collectors, fired CONCURRENTLY (they share the HL info-client
  // caches, so `activeAssetData` is fetched once): `account_leverage` (the bare
  // leverage field) and `order_enrichment` (maxLeverage / notional / margin
  // health / position state — the order-risk policy surface). Both NEVER throw
  // and are NOT part of the deny-closed fault surface below: any miss / timeout /
  // unknown master just omits the affected field (a `context has <field>` policy
  // stays dormant) rather than blocking the order. When this IS an
  // `updateLeverage`, refresh the leverage cache (fire-and-forget) so the next
  // order on that asset sees the just-set value.
  // Resolve the human asset symbol (HL meta universe) and patch it into the
  // built body BEFORE the enrichment collectors run. The order wire carries only
  // a numeric asset index, so the body is built with an `ASSET-<index>`
  // placeholder; this overwrites it with the real name (e.g. "BTC") so (a) a
  // symbol-matching policy (e.g. an order-symbol allowlist) sees the real symbol
  // and (b) the collectors — which key their per-market enrichment by
  // `market.symbol` — key by the SAME resolved symbol the lowering looks up by.
  // Best-effort + NEVER throws: a miss leaves the placeholder (the body stays
  // internally consistent; only symbol-specific policies stay dormant) and is
  // NOT part of the deny-closed fault surface.
  await resolveOrderSymbol(action, message.data);

  const [account_leverage, order_enrichment] = await Promise.all([
    collectHlLeverage(action, message.data),
    collectOrderEnrichment(action, message.data),
  ]);
  void noteHlLeverageUpdate(action, message.data);

  try {
    // Only dispatch when something is planned; the common HL case needs no policy-rpc call.
    const planned = await planActionRpcV2({
      manifests,
      action,
      meta,
      tx,
      account_leverage,
      order_enrichment,
    });
    // Server state-load identity: the eval `tx.from` is the submitter
    // SENTINEL, but a server-state method (`perp.*`) must read the MASTER's
    // synced wallet — pass the resolved master as the dispatch identity
    // override. No master → omit the key (server falls back to tx.from →
    // empty wallet → methods return nothing → stateful policies dormant,
    // never blocking).
    const results =
      planned.length > 0
        ? await dispatchCallsV2(planned, policyRpcUrl, {
            action,
            meta,
            tx,
            ...(master !== null ? { walletAddress: master } : {}),
          })
        : {};
    const verdict = await evaluateActionV2({
      action,
      meta,
      tx,
      bundles,
      results,
      account_leverage,
      order_enrichment,
    });
    console.info("[Pasu] venue-order-verdict", {
      requestId: message.requestId,
      venue: message.data.venue,
      verdict: verdict.kind,
      // Injected order-time leverage (empty `{}` means enrichment was dormant for this order).
      account_leverage,
      matched: verdict.matched?.map((m) => ({ id: m.policy_id, severity: m.severity })) ?? [],
    });
    // TEST-ONLY verdict tap (storage flag `pasu_e2e_tap`): record this venue
    // verdict at decision-time — BEFORE the warn modal blocks `decideMessage` —
    // so a high-volume e2e reads every verdict (pass/warn/fail) uniformly from
    // storage without driving the modal. Additive + default-off (no key in
    // prod → no-op). Per-requestId key avoids a read-modify-write race across
    // concurrent (non-actor-locked) venue orders.
    await tapVenueVerdict(message.requestId, master, verdict);
    // DENY → capture the exact diagnosis context so the dashboard can re-run
    // "which clause blocked this" against the real context (Option B). Mirrors
    // the EVM path in `evaluateActionRpcV2`; without it an HL deny would only
    // ever show the policy structure (no red culprit highlight). Best-effort,
    // keyed by requestId (= the verdict log's delta_id).
    if (verdict.kind === "fail") {
      void appendDiagnosisContext({
        id: message.requestId,
        ts: Math.floor(Date.now() / 1000),
        action,
        meta,
        tx,
        results,
      }).catch((err) =>
        console.warn(
          "[Pasu] diagnosis-context append failed (venue order)",
          err instanceof Error ? err.message : err,
        ),
      );
    }
    return {
      verdict,
      verdictSource: "declarative-v2",
      declarativeV3: {
        outcome: "hit",
        nature,
        decoder_id: "hl_place_order",
        action_count: 1,
      },
    };
  } catch (err) {
    // A plan/dispatch/evaluate throw is a fault → deny-closed (a flaky
    // WASM/RPC call must NOT waive a venue order through).
    console.warn("[Pasu] venue-order-verdict threw", {
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
 * the Pasu DevTools console: the EIP-712 `domain` / `primaryType` that was
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
    `[Pasu] off-chain signature parsed — ${domainName} / ${primaryType} ` +
      `(${leaves.length} action${leaves.length === 1 ? "" : "s"}) via ${routed.decoderId}\n` +
      lines.join("\n"),
  );
}

/**
 * EIP-712 typed-data signature verdict lifecycle.
 *
 * Sources `Action[]` from the manifest-driven typed-data router
 * ({@link routeTypedSignaturePayload}), then drives the same stateless v2 pipeline
 * (plan → dispatch → evaluate) as the transaction path.
 *
 * **warn-closed**: a route miss or fault yields a `noDecoderVerdict()` warn the
 * user must approve. A decoded signature with no matching policy baseline-passes.
 */
async function typedSignatureLifecycle(
  message: Message,
): Promise<LifecycleResult> {
  const nature: ActionNatureKind = "offchain_sig";
  if (!isTypedSignature(message)) {
    // Unreachable (caller guards); keep the type narrow + fail-closed.
    return { verdict: noDecoderVerdict(), verdictSource: "fail_closed" };
  }

  // Route the typed-data payload. `null` = no manifest match or decode miss;
  // a throw is an unexpected fault. Both warn-close.
  let routed: Awaited<ReturnType<typeof routeTypedSignaturePayload>>;
  try {
    routed = await routeTypedSignaturePayload({
      payload: message.data,
      submittedAt: Math.floor(Date.now() / 1000),
    });
  } catch (err) {
    console.warn("[Pasu] typed-sig route threw", {
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

  logParsedSignature(message, routed);
  console.info(
    `[Pasu] decoded ActionBody[] (${routed.actions.length})\n` +
      JSON.stringify(routed.actions, null, 2),
  );
  const declarativeV3: DeclarativeV3AuditMeta = {
    outcome: "hit",
    nature,
    decoder_id: routed.decoderId,
    action_count: routed.actions.length,
  };

  // 서명자 지갑의 effective 바인딩을 가져온다.
  const sigUid = (await getCurrentUserId()) ?? "anonymous";
  const resolved = await resolveBundlesForWallet(sigUid, message.data.address);
  // No policies ⇒ baseline pass (blocking requires an explicit deny policy).
  if (resolved.length === 0) {
    // Report any permit/permit2 sig for tracking (fire-and-forget; never blocks signing).
    void reportPermitIfApplicable(routed.actions, message);
    return {
      verdict: { kind: "pass" },
      verdictSource: "declarative-v2",
      declarativeV3,
    };
  }

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

  // v2 `tx` context for a signature: `to` is the EIP-712 verifyingContract,
  // `from` the signer. Only `{chain_id, from, to}` is consumed by the WASM;
  // a missing verifyingContract degrades to the zero sentinel without affecting
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
  let firstEngineError: EngineError | null = null;
  for (const a of realActions) {
    const action = (a as { body: unknown }).body;
    const meta = (a as { meta?: unknown }).meta;
    // 액션-단위 사전 필터(최적화) — 정밀 게이트는 엔진의 trigger 매칭.
    const bundles = filterForAction(resolved, collectActionMetas(action)).map(
      ({ policy, manifest }) => ({ policy, manifest }),
    );
    const manifests = bundles.map((b) => b.manifest);
    try {
      // Resolve token decimals once per body so each fungible amount gets its
      // `amountNano` sibling (non-fatal — a miss just omits that token's nano).
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
      // A plan/dispatch/evaluate throw makes this leg unevaluable. Record the
      // fault but keep aggregating siblings — a sibling's computed Fail must not
      // be demoted to an approvable warn. Resolution below honours deny-overrides.
      console.warn("[Pasu] typed-sig-verdict leg threw", {
        requestId: message.requestId,
        chainId: message.data.chainId,
        err: err instanceof Error ? err.message : String(err),
      });
      anyLegThrew = true;
      if (firstEngineError === null && err instanceof EngineError) {
        firstEngineError = err;
      }
    }
  }

  // Deny-overrides with a fault floor: a real `fail` outranks a sibling fault;
  // otherwise a fault with no computed deny warn-closes; otherwise the real
  // pass/warn aggregate stands.
  const aggregate = aggregateV2Verdicts(verdicts);
  if (aggregate.kind !== "fail" && anyLegThrew) {
    return {
      verdict: evaluateFaultVerdict(firstEngineError),
      verdictSource: "fail_closed",
      declarativeV3: { outcome: "fault", nature, reason: "evaluate_failed" },
    };
  }
  console.info("[Pasu] typed-sig-verdict", {
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
  // On PASS only, report any permit/permit2 sig for tracking (fire-and-forget).
  if (aggregate.kind === "pass") {
    void reportPermitIfApplicable(routed.actions, message);
  }
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
 * it is a `Multicall` — recurse into each inner child as its own evaluate envelope.
 * Returns the flat list of per-position verdicts for the caller to aggregate by
 * deny-overrides.
 *
 * `Outer`-scoped policies fire on the multicall batch; `Inner`-scoped (default)
 * policies fire on each child. Without this recursion an `Inner` slippage/recipient
 * policy would never see a multicall-wrapped swap.
 *
 * An `unknown`-domain body is skipped (not fail-closed) so one undecodable child
 * never blocks its siblings; the all-unknown case is still fail-closed by the
 * caller's `realActions` guard. Children share the parent `meta`.
 *
 * Throws only from `planActionRpcV2` / `dispatchCallsV2`; the caller's try/catch
 * turns that into a fail-closed verdict.
 */
async function evaluateBodyTree(
  body: unknown,
  meta: unknown,
  tx: ActionTxInputDto,
  bundles: readonly ActionBundleInputDto[],
  manifests: readonly unknown[],
  policyRpcUrl: string,
  requestId: string,
  // Per-token decimals collected once at the top level and threaded through
  // every child so each fungible amount's `amountNano` sibling is filled.
  tokenDecimals: Readonly<Record<string, number>>,
): Promise<VerdictDto[]> {
  const verdicts: VerdictDto[] = [];
  const domain =
    typeof body === "object" && body !== null
      ? (body as { domain?: unknown }).domain
      : undefined;

  if (domain !== undefined && domain !== "unknown") {
    // PLAN → DISPATCH (per-action map; `call_id` repeats across siblings) → EVALUATE.
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
    // On DENY, capture the exact context so the dashboard can re-run denial
    // diagnosis. Best-effort, keyed by requestId.
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
          "[Pasu] diagnosis-context append failed",
          err instanceof Error ? err.message : err,
        ),
      );
    }
  } else if (domain === "unknown") {
    // A nested batch position that decoded to nothing — contribute a warn so the
    // parent batch cannot aggregate to PASS on its legible siblings alone;
    // a sibling DENY still outranks this warn via deny-overrides.
    console.debug("[Pasu] per-child unknown leg → partial-decode warn", {
      requestId,
    });
    verdicts.push(partialDecodeVerdict());
  }

  // Recurse into multicall children — each its own envelope, parent meta shared.
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

/** {@link tryV2VerdictPath} 결과. `verdict`가 없으면 평가 불가 — 호출자가
 *  fail-closed 꼬리로 떨어지며, 원인이 명시적 엔진 오류였다면 `fault`로 전달. */
interface V2PathResult {
  verdict?: VerdictDto;
  fault?: EngineError;
}

async function tryV2VerdictPath(
  message: Message,
  actions: Record<string, unknown>[],
): Promise<V2PathResult> {
  if (!isTransaction(message)) return {};

  // Skip `Unknown` bodies — fall through to fail-closed handling.
  const realActions = actions.filter((a) => {
    const body = (a as { body?: unknown }).body;
    return (
      typeof body === "object" &&
      body !== null &&
      (body as { domain?: unknown }).domain !== "unknown"
    );
  });
  if (realActions.length === 0) return {};

  // `message.data.chainId` is a number; v2 `tx.chain_id` expects the CAIP-2
  // `eip155:<n>` form or the serde/trigger match fails.
  const tx = {
    chain_id: `eip155:${message.data.chainId}`,
    from: message.data.transaction.from ?? "0x" + "0".repeat(40),
    to: message.data.transaction.to ?? "0x" + "0".repeat(40),
  } as const;

  // tx.from 지갑의 effective 바인딩을 가져온다. 미등록 지갑은 defaults.enabled 적용.
  const uid = (await getCurrentUserId()) ?? "anonymous";
  const resolved = await resolveBundlesForWallet(uid, tx.from);
  if (resolved.length === 0) return {};
  const policyRpcUrl = process.env.POLICY_RPC_URL ?? "http://127.0.0.1:8787";

  const verdicts: VerdictDto[] = [];
  let anyLegThrew = false;
  let firstEngineError: EngineError | null = null;
  for (const a of realActions) {
    const action = (a as { body: unknown }).body;
    // 액션-단위 사전 필터(최적화) — 정밀 게이트는 엔진의 trigger 매칭.
    // The plan phase must see the identical manifest set the bundles carry —
    // `evaluate_action_v2_json` re-plans from `bundles[].manifest`, so a
    // divergent manifest list would mis-key the planned `call_id`s.
    const bundles = filterForAction(resolved, collectActionMetas(action)).map(
      ({ policy, manifest }) => ({ policy, manifest }),
    );
    const manifests = bundles.map((b) => b.manifest);
    const meta = (a as { meta?: unknown }).meta;
    try {
      // Resolve token decimals once per body (non-fatal — a miss omits that token's nano).
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

      // Replay the simulation against the server so the action + state-delta land
      // in the authenticated user's history. Best-effort — the WASM verdict is the
      // source of truth; recording is purely for the dashboard history view.
      // The server's `deltas[0]` is also cached locally so the history page's
      // `delta_id` join works without an extra server round-trip.
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
      // A plan/dispatch throw makes this leg unevaluable. Record the fault but
      // keep evaluating siblings — a sibling's computed Fail must not be demoted
      // to an approvable warn. Resolution below honours deny-overrides.
      console.warn("[Pasu] declarative-verdict-v2 leg threw", {
        requestId: message.requestId,
        chainId: message.data.chainId,
        err: err instanceof Error ? err.message : String(err),
      });
      anyLegThrew = true;
      if (firstEngineError === null && err instanceof EngineError) {
        firstEngineError = err;
      }
    }
  }

  // Deny-overrides with a fault floor:
  //   - a real `fail` from any leg outranks the fault → return it,
  //   - a fault with no computed deny falls through to the caller's fail-closed
  //     tail, carrying the explicit EngineError(예: 깨진 정책의 install_failed)
  //     so the warn shows the real reason instead of a generic no_decoder,
  //   - otherwise the real pass/warn aggregate stands.
  const aggregate = aggregateV2Verdicts(verdicts);
  if (aggregate.kind === "fail") return { verdict: aggregate };
  if (anyLegThrew) {
    return firstEngineError ? { fault: firstEngineError } : {};
  }
  return { verdict: aggregate };
}

/** Fail-closed verdict for an unevaluable leg. Same approvable-warn semantics
 *  as {@link noDecoderVerdict}, but an explicit `EngineError` (예: 깨진 정책의
 *  install_failed) surfaces its kind + message verbatim — 일반 no_decoder로
 *  뭉개면 어떤 정책이 문제인지 알 수 없다. */
function evaluateFaultVerdict(err: EngineError | null): VerdictDto {
  if (!err) return noDecoderVerdict();
  return {
    kind: "warn",
    matched: [
      {
        policy_id: `__engine::${err.kind}`,
        reason: err.message,
        severity: "warn",
        origin: "engine_error",
      },
    ],
  };
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

/** Map a v3 route outcome into the audit shape. */
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
 * Floor verdict for a decoded batch with an undecodable leg. A partially-decoded
 * batch must not PASS on its legible siblings alone; a sibling DENY still outranks
 * this warn via deny-overrides.
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
    console.error("[Pasu] openVerdictWindow failed", {
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
      if (!m || m.type !== "pasu:verdict-decision") return;
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

    // Extend the inpage stream timer so the user has time to read and decide.
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
 * Replay the just-evaluated simulation against the server so the action + state-delta
 * land in the authenticated user's history. Best-effort — failures are logged but
 * never affect the WASM verdict. Silent skip when the user is signed out.
 */
async function recordSimulationOnServer(input: {
  readonly action: unknown;
  readonly meta: unknown;
  readonly tx: {
    readonly chain_id: string;
    readonly from: string;
    readonly to: string;
  };
  /** Re-used as the server's idempotency key and the local state-delta row id
   *  so the verdict log's `delta_id` joins cleanly. When `undefined`, no local
   *  capture happens. */
  readonly decisionId?: string;
  /** Raw `0x`-prefixed calldata from the originating tx, persisted on the local
   *  state-delta row for the history page's simulation replay. */
  readonly calldata?: string;
  /** `msg.value` as a base-10 decimal string. Optional same as calldata. */
  readonly value?: string;
}): Promise<void> {
  // Skip silently for signed-out users — recording is opt-in via login.
  const hasToken = await pasuGetAccessToken().catch(() => null);
  if (!hasToken) return;

  // Build the server's `EvaluateRequest` shape. `eval_context` fields must match
  // the server's `EvalContext` exactly (camelCase `request_kind`, snake_case
  // `simulation`, required `action_index`) — a mismatch causes a 422 and the
  // record silently no-ops.
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
    const response = await pasuEvaluate({
      wallet_id: walletId,
      envelopes: [envelope as unknown as Record<string, unknown>],
      eval_context: evalContext,
      call_specs: [],
    });

    // Cache the first delta from the server response onto the local ring buffer
    // so the history page can render it without an extra server round-trip.
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
        // Local storage failure doesn't affect the verdict; the server retains the
        // canonical delta and the dashboard can fetch it directly.
        console.warn(
          "[Pasu] state-delta local append failed",
          storageErr instanceof Error ? storageErr.message : storageErr,
        );
      }
    }
  } catch (err) {
    if (err instanceof PasuServerError && err.isUnauthorized) {
      // Token expired between getAccessToken() and the call — swallow.
      console.debug("[Pasu] record skipped: server returned 401");
      return;
    }
    console.warn("[Pasu] record on server failed (non-fatal)", {
      chain: input.tx.chain_id,
      from: input.tx.from,
      err: err instanceof Error ? err.message : String(err),
    });
  }
}
