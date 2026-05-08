import Browser from 'webextension-polyfill';
import { ensureDefaultPoliciesInstalled } from './policies-loader';
import {
  fetchTier1,
  intoHostSnapshot,
  type Tier1Plan,
} from './facts/tier1-fetcher';
import {
  committedForActor,
  pendingForActor,
  reservePending,
  setTxHash,
} from './pending-deltas';
import {
  auditAppend,
  pendingDelete,
  pendingPut,
  type PendingRequest,
} from './storage';
import {
  buildAction,
  evaluate,
  EngineError,
  tier1FactPlan,
  tier2WindowKeys,
  type VerdictDto,
} from './wasm-bridge';
import {
  isTransaction,
  isTypedSignature,
  type Message,
} from '@lib/types';

const HARD_TIMEOUT_MS = 3_000;

interface DecisionResult {
  ok: boolean;
  verdict: VerdictDto;
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

function withActorLock<T>(actor: string | undefined, fn: () => Promise<T>): Promise<T> {
  if (!actor) return fn();
  const key = actor.toLowerCase();
  const prev = actorChain.get(key) ?? Promise.resolve();
  const next = prev.then(() => fn(), () => fn());
  actorChain.set(
    key,
    next.finally(() => {
      // Release the slot only when this is still the most recent waiter.
      if (actorChain.get(key) === next) actorChain.delete(key);
    }),
  );
  return next;
}

export async function decideMessage(message: Message): Promise<DecisionResult> {
  await ensureDefaultPoliciesInstalled();
  return withActorLock(inferActor(message), () => decideInner(message));
}

async function decideInner(message: Message): Promise<DecisionResult> {
  const pending: PendingRequest = {
    requestId: message.requestId,
    hostname: message.data.hostname,
    type: message.data.type as PendingRequest['type'],
    bypassed: 'bypassed' in message.data && !!message.data.bypassed,
    envelope: redactEnvelope(message),
    enqueuedAtMs: Date.now(),
  };
  await pendingPut(pending);

  try {
    const { result: verdict, timedOut } = await withTimeout(
      runLifecycle(message),
      HARD_TIMEOUT_MS,
      buildTimeoutVerdict(),
    );
    if (timedOut) {
      await Browser.storage.session.set({
        [`requests:rejected:${message.requestId}`]: true,
      });
    }

    await auditAppend({
      requestId: message.requestId,
      hostname: message.data.hostname,
      type: pending.type,
      verdict: verdict.kind,
      matchedPolicies:
        verdict.matched?.map((m) => ({ id: m.policy_id, severity: m.severity })) ?? [],
      decidedAtMs: Date.now(),
    });

    if (verdict.kind === 'pass') return { ok: true, verdict };
    if (verdict.kind === 'fail') return { ok: false, verdict };
    // Warn: TODO Plan 5 verdict modal. v0 falls back to fail-closed.
    // (Plan 5 verdict modal lands in the next iteration.)
    return { ok: false, verdict };
  } catch (err) {
    const verdict = engineErrorVerdict(err);
    await auditAppend({
      requestId: message.requestId,
      hostname: message.data.hostname,
      type: pending.type,
      verdict: 'fail',
      matchedPolicies: [{ id: '__engine::error', severity: 'deny' }],
      decidedAtMs: Date.now(),
    });
    return { ok: false, verdict };
  } finally {
    await pendingDelete(message.requestId);
  }
}

async function runLifecycle(message: Message): Promise<VerdictDto> {
  const requestJson = encodeRequestForEngine(message);
  if (!requestJson) {
    return engineErrorVerdict(
      new EngineError('unsupported', 'untyped signatures are not yet evaluable'),
    );
  }

  // Phase A: build action (no host needed).
  const actionParsed = (await buildAction(JSON.parse(requestJson))) as Record<string, unknown>;

  // Phase B: derive Tier-1 plan and fetch facts.
  const plan = (await tier1FactPlan(actionParsed)) as Tier1Plan;
  const tier1 = await fetchTier1(plan);

  // Phase C: tier-2 window keys derived from oracle snapshot.
  const tier2 = await tier2WindowKeys(actionParsed, tier1.oracle);

  // Merge committed + pending window state for the actor.
  const actor = inferActor(message);
  const actorLower = actor ? actor.toLowerCase() : null;
  const windowsMap = new Map<string, bigint>();
  if (actorLower) {
    for (const e of await committedForActor(actorLower)) {
      windowsMap.set(e.name, BigInt(e.value));
    }
    for (const e of await pendingForActor(actorLower)) {
      windowsMap.set(e.name, (windowsMap.get(e.name) ?? 0n) + BigInt(e.value));
    }
  }
  for (const k of tier2.keys) {
    if (!windowsMap.has(k.name)) windowsMap.set(k.name, 0n);
  }
  const windows = actorLower
    ? [...windowsMap.entries()].map(([name, value]) => ({
        actor: actorLower,
        name,
        value: value.toString(),
      }))
    : [];

  const snapshot = intoHostSnapshot(tier1, windows);

  // Phase D: evaluate.
  const verdict = await evaluate(JSON.parse(requestJson), snapshot);

  // Pass / Warn → reserve pending DEX deltas.
  const rejectedKey = `requests:rejected:${message.requestId}`;
  const rejected = ((await Browser.storage.session.get(rejectedKey)) as Record<string, unknown>)[
    rejectedKey
  ];
  if (verdict.kind !== 'fail' && actor && !rejected && isTransaction(message)) {
    const dexUsd = extractDexInputUsd(actionParsed);
    if (dexUsd) {
      await reservePending({
        requestId: message.requestId,
        chainId: message.data.chainId,
        actor,
        windowEntries: [
          { name: 'swapVolumeUsd24h', value: dexUsd },
          { name: 'swapCount24h', value: '1' },
        ],
        enqueuedAtMs: Date.now(),
      });
    }
  }

  return verdict;
}

function inferActor(message: Message): string | undefined {
  if (isTransaction(message)) return message.data.transaction.from;
  if (isTypedSignature(message)) return message.data.address;
  return undefined;
}

function extractDexInputUsd(actionParsed: Record<string, unknown>): string | undefined {
  const dex = actionParsed.dex as Record<string, unknown> | undefined;
  if (!dex) return undefined;
  const facts = dex.facts as Record<string, unknown> | undefined;
  const total = facts?.totalInputUsd as Record<string, unknown> | undefined;
  if (total && typeof total.value === 'string') return total.value;
  return undefined;
}

function hexToBytes(hex: string | undefined): number[] {
  if (!hex) return [];
  const clean = hex.startsWith('0x') ? hex.slice(2) : hex;
  if (clean.length % 2 !== 0) return [];
  const out: number[] = new Array(clean.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  return out;
}

function encodeRequestForEngine(message: Message): string | null {
  if (isTransaction(message)) {
    return JSON.stringify({
      Tx: {
        chain_id: message.data.chainId,
        from: message.data.transaction.from,
        to: message.data.transaction.to,
        value_wei: message.data.transaction.value ?? '0',
        data: hexToBytes(message.data.transaction.data),
        gas: null,
        nonce: null,
      },
    });
  }
  if (isTypedSignature(message)) {
    let typedData = message.data.typedData;
    if (typeof typedData === 'string') {
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
      primaryType: (message.data.typedData as { primaryType?: string })?.primaryType,
      verifyingContract: (message.data.typedData as { domain?: { verifyingContract?: string } })
        ?.domain?.verifyingContract,
    };
  }
  return {};
}

function buildTimeoutVerdict(): VerdictDto {
  return {
    kind: 'fail',
    matched: [
      {
        policy_id: '__engine::timeout',
        reason: `Engine took longer than ${HARD_TIMEOUT_MS}ms`,
        severity: 'deny',
        origin: 'engine_error',
      },
    ],
  };
}

function engineErrorVerdict(err: unknown): VerdictDto {
  const kind = err instanceof EngineError ? err.kind : 'unexpected';
  const message = err instanceof Error ? err.message : String(err);
  return {
    kind: 'fail',
    matched: [
      {
        policy_id: `__engine::${kind}`,
        reason: message,
        severity: 'deny',
        origin: 'engine_error',
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
export async function recordTxHash(requestId: string, txHash: string): Promise<void> {
  if (!/^0x[0-9a-fA-F]{64}$/.test(txHash)) return;
  await setTxHash(requestId, txHash);
}
