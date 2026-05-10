import {
  readAllowances,
  readBalances,
  type Address,
} from "../chains/rpc-client";
import {
  buildOracleSnapshot,
  type OracleNeed,
} from "../oracle/oracle-snapshot";
import { tokenKey } from "../oracle/token-key";
import type { OracleRequirement, Tier1Plan, Token } from "../wasm-bridge.types";
import type {
  AllowanceEntry,
  BalanceEntry,
  HostSnapshot,
  OracleEntry,
  WindowEntry,
} from "../types/host-snapshot";

export type TokenLite = Token;
export type OracleRequirementLite = OracleRequirement;
export type SigOracleRequirement = OracleRequirement;
export type { Tier1Plan } from "../wasm-bridge.types";

export interface Tier1FetchResult {
  oracle: OracleEntry[];
  balances: BalanceEntry[];
  allowances: AllowanceEntry[];
  now_ts: number;
}

// Keep each dimension below the former 2s outer cap while preserving siblings.
const DIM_BUDGET_MS = 1_500;
type Tier1Dimension = "oracle" | "balances" | "allowances";

type DimensionOutcome<T> =
  | { status: "fulfilled"; value: T }
  | { status: "rejected"; reason: unknown };

async function raceWithTimeout<T>(
  work: Promise<T>,
  budgetMs: number,
): Promise<{ timedOut: false; value: T } | { timedOut: true }> {
  let timeoutId: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<{ timedOut: true }>((resolve) => {
    timeoutId = setTimeout(() => resolve({ timedOut: true }), budgetMs);
  });
  const wrappedWork = work.then(
    (value): { timedOut: false; value: T } => ({ timedOut: false, value }),
  );

  try {
    return await Promise.race([wrappedWork, timeout]);
  } finally {
    if (timeoutId !== undefined) clearTimeout(timeoutId);
  }
}

function fallbackReason(reason: unknown): string {
  if (reason instanceof Error) return `${reason.name}: ${reason.message}`;
  return String(reason);
}

function warnDimensionFallback(
  dimension: Tier1Dimension,
  reason: string,
): void {
  console.warn("[Scopeball SW] tier1 dimension fell back", {
    dimension,
    reason,
  });
}

function warnOracleEntryFallback(need: OracleNeed, reason: string): void {
  console.warn("[Scopeball SW] tier1 oracle entry fell back", {
    dimension: "oracle",
    token_key: tokenKey(need),
    reason,
  });
}

async function withDimensionTimeout<T>(
  dimension: Tier1Dimension,
  promise: Promise<T>,
  fallback: T,
  onTimeout?: () => void,
): Promise<T> {
  const work = promise.then(
    (value): DimensionOutcome<T> => ({ status: "fulfilled", value }),
    (reason): DimensionOutcome<T> => ({ status: "rejected", reason }),
  );
  const outcome = await raceWithTimeout(work, DIM_BUDGET_MS);

  if (outcome.timedOut) {
    onTimeout?.();
    warnDimensionFallback(dimension, "timeout");
    return fallback;
  }

  if (outcome.value.status === "fulfilled") return outcome.value.value;

  warnDimensionFallback(dimension, fallbackReason(outcome.value.reason));
  return fallback;
}

function dedupeOracleNeeds(needs: readonly OracleNeed[]): OracleNeed[] {
  const dedup = new Map<string, OracleNeed>();
  for (const need of needs) {
    const address = need.address.toLowerCase();
    dedup.set(tokenKey({ ...need, address }), { ...need, address });
  }
  return [...dedup.values()];
}

async function fetchOracleSnapshotPartial(
  needs: readonly OracleNeed[],
  fetchImpl: typeof fetch,
  nowMs: number,
  budgetMs: number,
  onTimeout?: () => void,
): Promise<OracleEntry[]> {
  const uniqueNeeds = dedupeOracleNeeds(needs);
  if (uniqueNeeds.length === 0) return [];

  const collected: OracleEntry[] = [];
  const fetches = uniqueNeeds.map(async (need) => {
    try {
      const entries = await buildOracleSnapshot([need], fetchImpl, nowMs);
      const expectedTokenKey = tokenKey(need);
      collected.push(
        ...entries.filter(
          (entry) => entry.token_key.toLowerCase() === expectedTokenKey,
        ),
      );
    } catch (reason) {
      warnOracleEntryFallback(need, fallbackReason(reason));
    }
  });

  const outcome = await raceWithTimeout(Promise.allSettled(fetches), budgetMs);
  if (outcome.timedOut) {
    onTimeout?.();
    warnDimensionFallback("oracle", "timeout");
  }
  return collected.slice();
}

function oracleNeedFromToken(token: Token): OracleNeed {
  return {
    chainId: token.chain_id,
    address: token.address,
    isNative: token.is_native,
  };
}

function oracleNeedFromRequirement(
  requirement: OracleRequirement,
): OracleNeed {
  return oracleNeedFromToken(requirement.token);
}

async function fetchBalances(plan: Tier1Plan): Promise<(bigint | undefined)[]> {
  const out: (bigint | undefined)[] = new Array(plan.balances.length).fill(
    undefined,
  );
  const groups = new Map<
    string,
    { chainId: number; owner: string; indexes: number[]; tokens: Address[] }
  >();

  plan.balances.forEach((fact, index) => {
    const key = `${fact.token.chain_id}:${fact.owner.toLowerCase()}`;
    const group = groups.get(key) ?? {
      chainId: fact.token.chain_id,
      owner: fact.owner,
      indexes: [],
      tokens: [],
    };
    group.indexes.push(index);
    group.tokens.push(fact.token.address as Address);
    groups.set(key, group);
  });

  await Promise.all(
    [...groups.values()].map(async (group) => {
      try {
        const values = await readBalances(
          group.chainId,
          group.owner as Address,
          group.tokens,
        );
        values.forEach((value, offset) => {
          out[group.indexes[offset]] = value;
        });
      } catch {
        // Leave this group undefined.
      }
    }),
  );
  return out;
}

async function fetchAllowances(
  plan: Tier1Plan,
): Promise<(bigint | undefined)[]> {
  const out: (bigint | undefined)[] = new Array(plan.allowances.length).fill(
    undefined,
  );
  const groups = new Map<
    string,
    {
      chainId: number;
      owner: string;
      indexes: number[];
      tokens: Address[];
      spenders: Address[];
    }
  >();

  plan.allowances.forEach((fact, index) => {
    const key = `${fact.token.chain_id}:${fact.owner.toLowerCase()}`;
    const group = groups.get(key) ?? {
      chainId: fact.token.chain_id,
      owner: fact.owner,
      indexes: [],
      tokens: [],
      spenders: [],
    };
    group.indexes.push(index);
    group.tokens.push(fact.token.address as Address);
    group.spenders.push(fact.spender as Address);
    groups.set(key, group);
  });

  await Promise.all(
    [...groups.values()].map(async (group) => {
      try {
        const values = await readAllowances(
          group.chainId,
          group.owner as Address,
          group.tokens,
          group.spenders,
        );
        values.forEach((value, offset) => {
          out[group.indexes[offset]] = value;
        });
      } catch {
        // Leave this group undefined.
      }
    }),
  );
  return out;
}

async function fetchTier1Work(
  plan: Tier1Plan,
  fetchImpl: typeof fetch,
  nowMs: number,
  signal: AbortSignal,
): Promise<Tier1FetchResult> {
  const oracleController = new AbortController();
  const abortOracle = () => oracleController.abort();
  if (signal.aborted) abortOracle();
  else signal.addEventListener("abort", abortOracle, { once: true });

  const guardedFetch: typeof fetch = (input, init) =>
    fetchImpl(input, { ...init, signal: oracleController.signal });

  const oracleNeeds = [
    ...plan.tokens_for_oracle.map(oracleNeedFromToken),
    ...plan.sig_oracle_requirements.map(oracleNeedFromRequirement),
  ];

  const [oracle, balances, allowances] = await Promise.all([
    fetchOracleSnapshotPartial(
      oracleNeeds,
      guardedFetch,
      nowMs,
      DIM_BUDGET_MS,
      abortOracle,
    ),
    withDimensionTimeout("balances", fetchBalances(plan), []),
    withDimensionTimeout("allowances", fetchAllowances(plan), []),
  ]);

  signal.removeEventListener("abort", abortOracle);

  const balanceEntries: BalanceEntry[] = [];
  plan.balances.forEach((fact, index) => {
    const value = balances[index];
    if (value === undefined) return;
    balanceEntries.push({
      owner: fact.owner.toLowerCase(),
      token_key: tokenKey({
        chainId: fact.token.chain_id,
        address: fact.token.address,
        isNative: fact.token.is_native,
      }),
      balance: value.toString(),
    });
  });

  const allowanceEntries: AllowanceEntry[] = [];
  plan.allowances.forEach((fact, index) => {
    const value = allowances[index];
    if (value === undefined) return;
    allowanceEntries.push({
      owner: fact.owner.toLowerCase(),
      token_key: tokenKey({
        chainId: fact.token.chain_id,
        address: fact.token.address,
        isNative: fact.token.is_native,
      }),
      spender: fact.spender.toLowerCase(),
      allowance: value.toString(),
    });
  });

  return {
    oracle,
    balances: balanceEntries,
    allowances: allowanceEntries,
    now_ts: Math.floor(nowMs / 1000),
  };
}

export async function fetchTier1(
  plan: Tier1Plan,
  fetchImpl: typeof fetch = fetch,
  nowMs: number = Date.now(),
): Promise<Tier1FetchResult> {
  return fetchTier1Work(plan, fetchImpl, nowMs, new AbortController().signal);
}

export function intoHostSnapshot(
  tier1: Tier1FetchResult,
  windows: WindowEntry[] = [],
): HostSnapshot {
  return {
    oracle: tier1.oracle,
    balances: tier1.balances,
    allowances: tier1.allowances,
    now_ts: tier1.now_ts,
    windows,
  };
}
