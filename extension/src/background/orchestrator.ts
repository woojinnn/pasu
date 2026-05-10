import Browser from "webextension-polyfill";
import { ensureDefaultPoliciesInstalled } from "./policies-loader";
import { fetchTier1, intoHostSnapshot } from "./facts/tier1-fetcher";
import { tokenKey } from "./oracle/token-key";
import {
  committedForActor,
  pendingForActor,
  reservePending,
  setTxHash,
} from "./pending-deltas";
import {
  auditAppend,
  pendingDelete,
  pendingPut,
  type PendingRequest,
} from "./storage";
import {
  buildAction,
  evaluate,
  EngineError,
  tier1FactPlan,
  tier2WindowKeys,
} from "./wasm-bridge";
import type {
  DexAction,
  ParsedAction,
  Tier1Plan,
  VerdictDto,
} from "./wasm-bridge.types";
import {
  isTransaction,
  isTypedSignature,
  isUntypedSignature,
  type Message,
} from "@lib/types";
import type { OracleEntry } from "./types/host-snapshot";

// Caps `runLifecycle` (action build + tier-1 fact fetch + tier-2 window
// keys + Cedar evaluate). Three independent 1.5 s tier-1 dimension
// races (oracle/balances/allowances) plus a Cedar evaluation can take
// ~2-3 s on a cold service-worker boot. Set the cap higher so the
// timeout-warn fallback only fires on genuinely stuck engine work,
// not on routine cold-cache lifecycles. Aligned to be a few seconds
// shorter than the proxy's PHASE1_MS so the SW always has time to
// post back a real verdict (or an `awaiting-user` extension request)
// before the proxy gives up.
const HARD_TIMEOUT_MS = 8_000;

interface DecisionResult {
  ok: boolean;
  verdict: VerdictDto;
}

interface DecisionOptions {
  onAwaitingUser?: () => void;
}

interface LifecycleResult {
  verdict: VerdictDto;
  dexWindowEntries?: WindowEntryDelta[];
}

interface WindowEntryDelta {
  name: string;
  value: string;
}

/**
 * Per-actor mutex chain. The read-evaluate-reserve sequence (read pending
 * + committed → evaluate → reserve a delta on Pass) is non-atomic at the
 * storage layer; without serialization two concurrent decisions for the
 * same wallet could each see the same baseline and both pass an over-the-
 * cap swap. We serialize lifecycles per `actor` (lowercased) by chaining
 * promises so the second decision strictly waits for the first to commit
 * its reservation.
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
      { verdict: buildTimeoutVerdict() },
    );
    const { verdict } = lifecycle;

    let ok = false;
    if (verdict.kind === "pass") {
      await reserveDexDeltaIfNeeded(message, lifecycle.dexWindowEntries);
      ok = true;
    } else if (verdict.kind === "fail") {
      // Surface the matched policies in a popup so the user understands
      // why the dApp's transaction returned 4001. The popup is
      // informational — Fail decisions don't take user input.
      void openVerdictWindow(message.requestId, message.data.hostname, verdict);
    } else {
      // Warn: open the modal and await the user's Trust-and-proceed / Cancel.
      ok = await openVerdictWindowAndAwait(
        message.requestId,
        message.data.hostname,
        verdict,
        options.onAwaitingUser,
      );
      if (ok) {
        await reserveDexDeltaIfNeeded(message, lifecycle.dexWindowEntries);
      }
    }

    await appendAudit(message, pending.type, verdict);
    return { ok, verdict };
  } catch (err) {
    const verdict = engineErrorVerdict(err);
    await appendAudit(message, pending.type, verdict);
    return { ok: false, verdict };
  } finally {
    await pendingDelete(message.requestId);
  }
}

async function appendAudit(
  message: Message,
  type: PendingRequest["type"],
  verdict: VerdictDto,
): Promise<void> {
  await auditAppend({
    requestId: message.requestId,
    hostname: message.data.hostname,
    type,
    verdict: verdict.kind,
    matchedPolicies:
      verdict.matched?.map((m) => ({
        id: m.policy_id,
        severity: m.severity,
      })) ?? [],
    decidedAtMs: Date.now(),
  });
}

async function reserveDexDeltaIfNeeded(
  message: Message,
  windowEntries: WindowEntryDelta[] | undefined,
): Promise<void> {
  const actor = inferActor(message);
  if (!windowEntries || !actor || !isTransaction(message)) return;
  await reservePending({
    requestId: message.requestId,
    chainId: message.data.chainId,
    actor,
    windowEntries,
    enqueuedAtMs: Date.now(),
  });
}

async function runLifecycle(message: Message): Promise<LifecycleResult> {
  const requestJson = encodeRequestForEngine(message);
  if (!requestJson) {
    if (isUntypedSignature(message)) {
      return { verdict: unsupportedUntypedSignatureVerdict() };
    }
    return {
      verdict: engineErrorVerdict(
        new EngineError("unsupported", "request type is not yet evaluable"),
      ),
    };
  }

  // Phase A: build action (no host needed).
  const actionParsed: ParsedAction = await buildAction(JSON.parse(requestJson));

  // Phase B: derive Tier-1 plan and fetch facts.
  const plan: Tier1Plan = await tier1FactPlan(actionParsed);
  const tier1 = await fetchTier1(plan);
  warnMissingOracleEntries(message, plan, tier1.oracle);

  // Phase C: tier-2 window keys derived from oracle snapshot.
  const tier2 = await tier2WindowKeys(actionParsed, tier1.oracle);

  // Merge committed + pending window state for the actor.
  const actor = inferActor(message);
  const actorLower = actor ? actor.toLowerCase() : null;
  const windowsMap = new Map<string, string>();
  if (actorLower) {
    for (const e of await committedForActor(actorLower)) {
      windowsMap.set(e.name, e.value);
    }
    for (const e of await pendingForActor(actorLower)) {
      const previous = windowsMap.get(e.name);
      windowsMap.set(
        e.name,
        previous === undefined
          ? e.value
          : addWindowValues(e.name, previous, e.value),
      );
    }
  }
  for (const k of tier2.keys) {
    if (!windowsMap.has(k.name))
      windowsMap.set(k.name, zeroWindowValue(k.name));
  }
  const windows = actorLower
    ? [...windowsMap.entries()].map(([name, value]) => ({
        actor: actorLower,
        name,
        value,
      }))
    : [];

  const snapshot = intoHostSnapshot(tier1, windows);

  // Phase D: evaluate.
  const verdict = await evaluate(JSON.parse(requestJson), snapshot);
  const dexWindowEntries = computeDexWindowEntries(actionParsed, tier1.oracle);
  return dexWindowEntries ? { verdict, dexWindowEntries } : { verdict };
}

function warnMissingOracleEntries(
  message: Message,
  plan: Tier1Plan,
  oracleEntries: OracleEntry[],
): void {
  const requested = plannedOracleTokenKeys(plan);
  if (requested.length === 0) return;

  const resolved = new Set(
    oracleEntries.map((entry) => entry.token_key.toLowerCase()),
  );
  const missing = requested.filter((tokenKey) => !resolved.has(tokenKey));
  if (missing.length === 0) return;

  console.warn(
    "[Scopeball SW] oracle_requirements declared but no entries returned — dex/USD policies will silently miss",
    {
      requestId: message.requestId,
      hostname: message.data.hostname,
      requested,
      missing,
    },
  );
}

function plannedOracleTokenKeys(plan: Tier1Plan): string[] {
  const keys = new Set<string>();
  const addToken = (token: {
    chain_id: number;
    address: string;
    is_native?: boolean;
  }): void => {
    keys.add(
      tokenKey({
        chainId: token.chain_id,
        address: token.address,
      }),
    );
  };

  for (const token of plan.tokens_for_oracle) addToken(token);
  for (const requirement of plan.sig_oracle_requirements) {
    if ("token" in requirement) {
      addToken(requirement.token);
    } else {
      keys.add(tokenKey(requirement));
    }
  }

  return [...keys];
}

function inferActor(message: Message): string | undefined {
  if (isTransaction(message)) return message.data.transaction.from;
  if (isTypedSignature(message)) return message.data.address;
  return undefined;
}

function computeDexWindowEntries(
  actionParsed: ParsedAction,
  oracleEntries: OracleEntry[],
): WindowEntryDelta[] | undefined {
  if (!isParsedDexAction(actionParsed)) return undefined;
  const dexInputUsd = computeDexInputUsd(actionParsed.dex, oracleEntries);
  const entries: WindowEntryDelta[] = [];
  if (dexInputUsd) {
    entries.push({ name: "swapVolumeUsd24h", value: dexInputUsd });
  }
  entries.push({ name: "swapCount24h", value: "1" });
  return entries;
}

function isParsedDexAction(
  actionParsed: ParsedAction,
): actionParsed is ParsedAction & { readonly dex: DexAction } {
  return "dex" in actionParsed && actionParsed.dex !== undefined;
}

function computeDexInputUsd(
  dex: DexAction,
  oracleEntries: OracleEntry[],
): string | undefined {
  const prices = new Map(
    oracleEntries.map((entry) => [
      entry.token_key.toLowerCase(),
      entry.usd_per_unit,
    ]),
  );

  let total = 0n;
  let found = false;
  for (const requirement of dex.oracle_requirements) {
    if (requirement.kind !== "input") continue;
    const requirementTokenKey = tokenKey({
      chainId: requirement.token.chain_id,
      address: requirement.token.address,
    });
    const raw = parseUnsignedDecimal(requirement.raw_amount);
    const price = prices.get(requirementTokenKey);
    if (raw === undefined || !price) continue;

    const fixedUsd = multiplyRawByUsd(raw, requirement.token.decimals, price);
    if (fixedUsd === undefined) continue;
    total += fixedUsd;
    found = true;
  }

  return found ? fixedToDecimal(total) : undefined;
}

function parseUnsignedDecimal(value: unknown): bigint | undefined {
  if (typeof value !== "string" || !/^\d+$/.test(value)) return undefined;
  try {
    return BigInt(value);
  } catch {
    return undefined;
  }
}

function multiplyRawByUsd(
  raw: bigint,
  decimals: number,
  price: string,
): bigint | undefined {
  if (!Number.isSafeInteger(decimals) || decimals < 0 || decimals > 255)
    return undefined;
  const priceFixed = decimalToFixed(price);
  if (priceFixed === undefined) return undefined;
  const scaled = (raw * priceFixed) / 10n ** BigInt(decimals);
  return scaled <= 9223372036854775807n ? scaled : undefined;
}

function decimalToFixed(value: string): bigint | undefined {
  const parts = value.split(".");
  if (parts.length > 2) return undefined;
  const [whole, fraction = ""] = parts;
  if (whole === "" && fraction === "") return undefined;
  if (!/^\d*$/.test(whole) || !/^\d*$/.test(fraction)) return undefined;
  const padded = `${fraction}0000`.slice(0, 4);
  try {
    return BigInt(`${whole || "0"}${padded}`);
  } catch {
    return undefined;
  }
}

function fixedToDecimal(value: bigint): string {
  const raw = value.toString().padStart(5, "0");
  const whole = raw.slice(0, -4);
  const fraction = raw.slice(-4);
  return `${whole}.${fraction}`;
}

function addWindowValues(name: string, left: string, right: string): string {
  if (name === "swapVolumeUsd24h") {
    const leftFixed = decimalToFixed(left);
    const rightFixed = decimalToFixed(right);
    if (leftFixed === undefined) return right;
    if (rightFixed === undefined) return left;
    return fixedToDecimal(leftFixed + rightFixed);
  }
  return (BigInt(left) + BigInt(right)).toString();
}

function zeroWindowValue(name: string): string {
  return name === "swapVolumeUsd24h" ? "0.0000" : "0";
}

function hexToBytes(hex: string | undefined): number[] {
  if (!hex) return [];
  const clean = hex.startsWith("0x") ? hex.slice(2) : hex;
  if (clean.length % 2 !== 0) return [];
  const out: number[] = new Array(clean.length / 2);
  for (let i = 0; i < out.length; i++)
    out[i] = parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  return out;
}

function encodeRequestForEngine(message: Message): string | null {
  if (isTransaction(message)) {
    return JSON.stringify({
      Tx: {
        chain_id: message.data.chainId,
        from: message.data.transaction.from,
        to: message.data.transaction.to,
        value_wei: quantityToDecimal(message.data.transaction.value),
        data: hexToBytes(message.data.transaction.data),
        gas: null,
        nonce: null,
      },
    });
  }
  if (isTypedSignature(message)) {
    let typedData = message.data.typedData;
    if (typeof typedData === "string") {
      try {
        typedData = JSON.parse(typedData);
      } catch {
        return null;
      }
    }
    return JSON.stringify({
      Sig: {
        chainId: message.data.chainId,
        signer: message.data.address,
        typedData,
      },
    });
  }
  return null;
}

function quantityToDecimal(value: string | undefined): string {
  if (!value) return "0";
  if (!value.toLowerCase().startsWith("0x")) return value;
  try {
    return BigInt(value).toString();
  } catch {
    return value;
  }
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

/** Receive tx-hash reports from the inpage proxy and stamp them onto pending deltas. */
export async function recordTxHash(
  requestId: string,
  txHash: string,
): Promise<void> {
  if (!/^0x[0-9a-fA-F]{64}$/.test(txHash)) return;
  await setTxHash(requestId, txHash);
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
      width: 520,
      height: 480,
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
      width: 520,
      height: 480,
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
