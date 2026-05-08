import {
  readAllowances,
  readBalances,
  type Address,
} from '@background/chains/rpc-client';
import { buildOracleSnapshot } from '@background/oracle/oracle-snapshot';
import type {
  AllowanceEntry,
  BalanceEntry,
  HostSnapshot,
  OracleEntry,
  WindowEntry,
} from '@background/types/host-snapshot';

export interface TokenLite {
  chain_id: number;
  address: string;
  symbol: string;
  decimals: number;
  is_native: boolean;
}

export interface OracleRequirementLite {
  kind: 'input' | 'minOutput';
  token: TokenLite;
  raw_amount: string;
}

export interface Tier1Plan {
  tokens_for_oracle: TokenLite[];
  balances: { owner: string; token: TokenLite }[];
  allowances: { owner: string; token: TokenLite; spender: string }[];
  clock_required: boolean;
  /** Mirrors the engine's HostFactPlan field (Codex round-3 finding). */
  sig_oracle_requirements: OracleRequirementLite[];
}

export interface Tier1FetchResult {
  oracle: OracleEntry[];
  balances: BalanceEntry[];
  allowances: AllowanceEntry[];
  now_ts: number;
}

const TIER1_OUTER_TIMEOUT_MS = 2_000;

/**
 * Run all Tier-1 host fetches in parallel and assemble a partial
 * HostSnapshot. Failures (RPC reverts, CoinGecko 429s) become absent
 * entries — never zero. The orchestrator merges in `windows` (Tier 2)
 * and the final `now_ts` later.
 *
 * An outer AbortController caps the entire fetch at TIER1_OUTER_TIMEOUT_MS
 * so a stalled fallback URL or saturated CoinGecko free tier doesn't blow
 * past the design's host-fact-fetch budget.
 */
export async function fetchTier1(
  plan: Tier1Plan,
  fetchImpl: typeof fetch = fetch,
  nowMs: number = Date.now(),
): Promise<Tier1FetchResult> {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), TIER1_OUTER_TIMEOUT_MS);
  const guardedFetch: typeof fetch = (input, init) =>
    fetchImpl(input, { ...init, signal: controller.signal });

  const oraclePromise = buildOracleSnapshot(
    plan.tokens_for_oracle.map((t) => ({
      chainId: t.chain_id,
      address: t.address,
      isNative: t.is_native,
    })),
    guardedFetch,
    nowMs,
  );

  const balancesPromise = readBalances(
    plan.balances.map((b) => ({
      owner: b.owner as Address,
      token: b.token.address as Address,
      chainId: b.token.chain_id,
    })),
  );

  const allowancesPromise = readAllowances(
    plan.allowances.map((a) => ({
      owner: a.owner as Address,
      token: a.token.address as Address,
      spender: a.spender as Address,
      chainId: a.token.chain_id,
    })),
  );

  let oracle: Awaited<typeof oraclePromise>;
  let balances: Awaited<typeof balancesPromise>;
  let allowances: Awaited<typeof allowancesPromise>;
  try {
    [oracle, balances, allowances] = await Promise.all([
      oraclePromise,
      balancesPromise,
      allowancesPromise,
    ]);
  } finally {
    clearTimeout(timeoutId);
  }
  if (controller.signal.aborted) {
    return {
      oracle: [],
      balances: [],
      allowances: [],
      now_ts: Math.floor(nowMs / 1000),
    };
  }

  const balanceEntries: BalanceEntry[] = [];
  plan.balances.forEach((b, i) => {
    const v = balances[i];
    if (v === undefined) return;
    balanceEntries.push({
      owner: b.owner.toLowerCase(),
      token_key: `${b.token.chain_id}:${b.token.address.toLowerCase()}`,
      balance: v.toString(),
    });
  });

  const allowanceEntries: AllowanceEntry[] = [];
  plan.allowances.forEach((a, i) => {
    const v = allowances[i];
    if (v === undefined) return;
    allowanceEntries.push({
      owner: a.owner.toLowerCase(),
      token_key: `${a.token.chain_id}:${a.token.address.toLowerCase()}`,
      spender: a.spender.toLowerCase(),
      allowance: v.toString(),
    });
  });

  return {
    oracle,
    balances: balanceEntries,
    allowances: allowanceEntries,
    now_ts: Math.floor(nowMs / 1000),
  };
}

/** Merge Tier-1 result + window entries + clock into a full HostSnapshot. */
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
