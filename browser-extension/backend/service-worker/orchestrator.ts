import Browser from "webextension-polyfill";
import { ensureSeedBundlesInstalled } from "./marketplace/declarative-adapter-loader";
import {
  tryDeclarativeRoute,
  type DeclarativeRouteOutcome,
} from "./marketplace/declarative-route";
import {
  ensureDefaultPoliciesInstalled,
  getActivePolicyRpcManifests,
} from "./policies-loader";
import { getAllManifests } from "./manifests/store";
import {
  auditAppend,
  pendingDelete,
  pendingPut,
  type PendingRequest,
} from "./storage";
import { EngineError, evaluateWithEnvelopes } from "./wasm-bridge";
import {
  evaluateWithPolicyRpc,
  formatAuditMatched,
  type PolicyRpcAuditMeta,
} from "./policy-rpc";
import type { VerdictDto } from "./wasm-bridge.types";
import {
  isTransaction,
  isTypedSignature,
  isUntypedSignature,
  type Message,
} from "@lib/types";

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
 * Phase 6 — audit telemetry capturing the declarative pipeline's contribution
 * to a single decision. Surfaced in the audit log so we can tell which
 * marketplace bundle handled (or failed to handle) a given tx.
 *
 * Phase 7F update: when `outcome === "hit"` AND `envelope_count > 0`, the
 * declarative path now drives the Cedar verdict via
 * `evaluate_with_envelopes_json` (Phase 7A). The static `evaluateWithPolicyRpc`
 * remains the fallback for miss/fault outcomes and the legacy ground truth
 * for cases the declarative path does not yet cover.
 */
export interface DeclarativeAuditMeta {
  outcome: DeclarativeRouteOutcome["kind"]; // "hit" | "miss" | "fault"
  source?: "layer1" | "layer2" | "jit";
  decoder_id?: string;
  bundle_id?: string;
  envelope_count?: number;
  reason?: string;
}

/**
 * Phase 7F — which Cedar pipeline produced the final verdict.
 *
 * `"declarative"` ⇒ envelopes from the declarative router were fed to
 *   `evaluate_with_envelopes_json` (Phase 7A WASM entry).
 * `"static"` ⇒ verdict came from the legacy `evaluateWithPolicyRpc` path,
 *   either because the declarative path missed/faulted, the message is a
 *   typed signature, or the declarative path produced zero envelopes.
 */
export type VerdictSource = "declarative" | "static";

interface LifecycleResult {
  verdict: VerdictDto;
  verdictSource: VerdictSource;
  policyRpc?: PolicyRpcAuditMeta;
  declarative?: DeclarativeAuditMeta;
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
  // Phase 1B — mount declarative adapter seed bundles after the policy
  // engine is warm. `ensureSeedBundlesInstalled` is idempotent within a
  // single SW lifetime; subsequent calls return the cached promise.
  await ensureSeedBundlesInstalled();
  return withActorLock(inferActor(message), () =>
    decideInner(message, options),
  );
}

async function decideInner(
  message: Message,
  options: DecisionOptions,
): Promise<DecisionResult> {
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
    const { result: lifecycle } = await withTimeout(
      runLifecycle(message),
      HARD_TIMEOUT_MS,
      { verdict: buildTimeoutVerdict(), verdictSource: "static" as const },
    );
    const { verdict } = lifecycle;

    let ok = false;
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
    }

    await appendAudit(
      message,
      pending.type,
      verdict,
      lifecycle.verdictSource,
      lifecycle.policyRpc,
      lifecycle.declarative,
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
    logAt("[Scopeball] decideMessage threw", {
      requestId: message.requestId,
      hostname: message.data.hostname,
      type: pending.type,
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
  declarative?: DeclarativeAuditMeta,
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
    ...(declarative ? { declarative } : {}),
    ...(verdictSource ? { verdictSource } : {}),
    decidedAtMs: Date.now(),
  });
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
    console.info("[Scopeball] typed-sig", {
      ...common,
      chainId: message.data.chainId,
      primaryType: (message.data.typedData as { primaryType?: string })
        ?.primaryType,
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
  if (isUntypedSignature(message)) {
    return {
      verdict: unsupportedUntypedSignatureVerdict(),
      verdictSource: "static",
    };
  }

  // Phase 6 → Phase 7F — declarative path is now a verdict driver, not
  // observability-only. For transactions we hand off
  // `(chainId, to, calldata)` to the marketplace router. A hit with one or
  // more enriched envelopes lets us run `evaluate_with_envelopes_json`
  // directly, skipping `plan_policy_rpc_json` and the RPC enrichment hop.
  //
  // The static `evaluateWithPolicyRpc` remains the fallback for miss/fault
  // outcomes, hit-with-zero-envelopes edges, and any unexpected throw
  // inside `tryDeclarativeRoute`. We deliberately fence both call sites in
  // try/catch so a glitch (registry server down, malformed bundle, race)
  // cannot block a verdict.
  let declarativeMeta: DeclarativeAuditMeta | undefined;
  let declarativeHit: {
    envelopes: Record<string, unknown>[];
    decoderId: string;
  } | undefined;
  if (isTransaction(message)) {
    try {
      const outcome = await tryDeclarativeRoute({
        chainId: message.data.chainId,
        from: message.data.transaction.from ?? "0x" + "0".repeat(40),
        to: message.data.transaction.to ?? "0x" + "0".repeat(40),
        valueWei: txValueToWeiDecimal(message.data.transaction.value),
        calldataHex: message.data.transaction.data,
      });
      declarativeMeta = auditFromDeclarativeOutcome(outcome);
      console.info("[Scopeball] declarative-route", {
        requestId: message.requestId,
        chainId: message.data.chainId,
        outcome: outcome.kind,
        ...(outcome.kind === "hit"
          ? {
              decoderId: outcome.value.decoderId,
              bundleId: outcome.value.bundleId,
              source: outcome.value.source,
              envelopeCount: outcome.value.envelopes.length,
            }
          : outcome.kind === "miss"
            ? { reason: outcome.reason }
            : { reason: outcome.reason }),
      });
      if (outcome.kind === "hit" && outcome.value.envelopes.length > 0) {
        declarativeHit = {
          envelopes: outcome.value.envelopes,
          decoderId: outcome.value.decoderId,
        };
      }
    } catch (err) {
      // tryDeclarativeRoute already classifies known errors. Anything
      // reaching here is truly unexpected — log and continue with the
      // static path.
      console.warn("[Scopeball] declarative-route threw", {
        requestId: message.requestId,
        err: err instanceof Error ? err.message : String(err),
      });
      declarativeMeta = { outcome: "fault", reason: "unexpected" };
    }
  }

  // Declarative verdict path — only taken when the declarative router
  // returned a hit with ≥1 enriched envelope AND the message is a
  // transaction. Failures here fall through to the static path so a flaky
  // WASM call does NOT take out a tx whose static path would have passed.
  if (declarativeHit && isTransaction(message)) {
    try {
      const verdict = await evaluateWithEnvelopes({
        envelopes: declarativeHit.envelopes,
        from: message.data.transaction.from ?? "0x" + "0".repeat(40),
        to: message.data.transaction.to ?? "0x" + "0".repeat(40),
        value_wei: txValueToWeiDecimal(message.data.transaction.value),
        chain_id: message.data.chainId,
        block_timestamp: Math.floor(Date.now() / 1000),
        manifests: getActivePolicyRpcManifests(),
        // Phase 7F MVP: declarative verdict path runs without RPC
        // enrichment. Manifests that declare `requires` are NOT yet
        // wired through this path — when they exist the WASM will fail
        // closed via `__engine::projection_failed`, which is the
        // desired conservative behaviour until 7G/7H wire policy-rpc
        // results into the declarative branch.
        rpc_response: {
          request_id: message.requestId,
          results: [],
        },
      });
      console.info("[Scopeball] declarative-verdict", {
        requestId: message.requestId,
        verdictSource: "declarative",
        verdict: verdict.kind,
        envelopeCount: declarativeHit.envelopes.length,
        decoderId: declarativeHit.decoderId,
        matched:
          verdict.matched?.map((m) => ({
            id: m.policy_id,
            severity: m.severity,
          })) ?? [],
      });
      return {
        verdict,
        verdictSource: "declarative",
        ...(declarativeMeta ? { declarative: declarativeMeta } : {}),
      };
    } catch (err) {
      // evaluateWithEnvelopes threw — most likely an EngineError on the
      // installed_manifest_hash_mismatch path. Log and fall through to
      // the static path so we don't lose a verdict.
      console.warn("[Scopeball] declarative-verdict threw", {
        requestId: message.requestId,
        decoderId: declarativeHit.decoderId,
        err: err instanceof Error ? err.message : String(err),
      });
      // Fall through to static path below.
    }
  }

  // Phase 7 codex carry-over H: at evaluate-time the orchestrator MUST
  // use the same manifest set the WASM engine was last installed with.
  // The post-Phase-6 source of truth is `manifests/store.ts` (the Map
  // shape); `atomicInstall` and `hydrateManifests` both push that Map
  // through `install_policies_json`. Forwarding the legacy
  // `getActivePolicyRpcManifests()` Vec (built from the embedded
  // `manifest`/`manifests` fields on default-policy JSONs) hashed
  // differently from the Map values and surfaced as a silent
  // `manifest_hash_mismatch` in WASM. We prefer the Map; if it's empty
  // we fall back to the legacy Vec so SW boots before any user-driven
  // install path runs still work end-to-end (default-policies-only).
  const mapManifests = Object.values(await getAllManifests()) as unknown[];
  const manifests =
    mapManifests.length > 0 ? mapManifests : getActivePolicyRpcManifests();

  const result = await evaluateWithPolicyRpc(message, { manifests });
  console.info("[Scopeball] declarative-verdict", {
    requestId: message.requestId,
    verdictSource: "static",
    verdict: result.verdict.kind,
    matched:
      result.verdict.matched?.map((m) => ({
        id: m.policy_id,
        severity: m.severity,
      })) ?? [],
  });
  return {
    verdict: result.verdict,
    verdictSource: "static",
    policyRpc: result.audit,
    ...(declarativeMeta ? { declarative: declarativeMeta } : {}),
  };
}

function auditFromDeclarativeOutcome(
  outcome: DeclarativeRouteOutcome,
): DeclarativeAuditMeta {
  if (outcome.kind === "hit") {
    return {
      outcome: "hit",
      source: outcome.value.source,
      decoder_id: outcome.value.decoderId,
      bundle_id: outcome.value.bundleId,
      envelope_count: outcome.value.envelopes.length,
    };
  }
  if (outcome.kind === "miss") {
    return { outcome: "miss", reason: outcome.reason };
  }
  return { outcome: "fault", reason: outcome.reason };
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
    return {
      primaryType: (message.data.typedData as { primaryType?: string })
        ?.primaryType,
      verifyingContract: (
        message.data.typedData as { domain?: { verifyingContract?: string } }
      )?.domain?.verifyingContract,
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
  } catch {
    /* user closed, popup blocked, etc. — best-effort UI */
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
