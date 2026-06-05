/**
 * RPC client + Multicall3 batching + disk cache layer.
 *
 * Defaults to publicnode endpoints (read-only operations); production
 * overrides via `RPC_URL_<chainId>` env vars (e.g. Alchemy / Infura keys).
 *
 * Cache layout: `registryV2/cache/protocol-sources/<protocol>/<chainId>.json`.
 * Entries are git-tracked so CI builds without RPC credentials still pass
 * after the initial dev fetch.
 */

import { createPublicClient, http, type Address, type Chain, type PublicClient } from "viem";
import { mainnet, optimism, base, arbitrum } from "viem/chains";
import { readFileSync, writeFileSync, mkdirSync, existsSync } from "fs";
import { dirname, join } from "path";

import type { CacheEntry, Hex, ProtocolSourceKind, RpcClient } from "./types.ts";

// ---------------------------------------------------------------------------
// Default RPC endpoints — publicnode (read-only, no key required)
// ---------------------------------------------------------------------------

const DEFAULT_RPC: Record<number, string> = {
  1: "https://ethereum-rpc.publicnode.com",
  10: "https://optimism-rpc.publicnode.com",
  8453: "https://base-rpc.publicnode.com",
  42161: "https://arbitrum-one.publicnode.com",
};

const CHAIN_DEFS: Record<number, { chain: Chain; name: string }> = {
  1: { chain: mainnet, name: "mainnet" },
  10: { chain: optimism, name: "optimism" },
  8453: { chain: base, name: "base" },
  42161: { chain: arbitrum, name: "arbitrum" },
};

function rpcUrlFor(chainId: number): string {
  const envKey = `RPC_URL_${chainId}`;
  const fromEnv = process.env[envKey];
  if (fromEnv && fromEnv.length > 0) return fromEnv;
  const def = DEFAULT_RPC[chainId];
  if (!def) {
    throw new Error(
      `rpc.ts: no default RPC configured for chainId ${chainId}; set env ${envKey} to override.`,
    );
  }
  return def;
}

// ---------------------------------------------------------------------------
// PublicClient cache (one per chain — viem amortizes Multicall3 batching)
// ---------------------------------------------------------------------------

class ViemRpcClient implements RpcClient {
  private clients = new Map<number, PublicClient>();

  private clientFor(chainId: number): PublicClient {
    const cached = this.clients.get(chainId);
    if (cached) return cached;
    const def = CHAIN_DEFS[chainId];
    if (!def) {
      throw new Error(`rpc.ts: unsupported chainId ${chainId} — add to CHAIN_DEFS`);
    }
    const client = createPublicClient({
      chain: def.chain,
      transport: http(rpcUrlFor(chainId), { batch: true }),
    }) as PublicClient;
    this.clients.set(chainId, client);
    return client;
  }

  async call(chainId: number, params: { to: Address; data: Hex }): Promise<Hex> {
    const client = this.clientFor(chainId);
    const result = await client.call({
      to: params.to,
      data: params.data as `0x${string}`,
    });
    return (result.data ?? "0x") as Hex;
  }

  async blockNumber(chainId: number): Promise<bigint> {
    return this.clientFor(chainId).getBlockNumber();
  }

  /**
   * Multicall3-batched read. viem's `multicall` automatically routes through
   * the deterministic Multicall3 deployment (`0xcA11bde05977b3631167028862bE2a173976CA11`)
   * present on all supported chains. Caller supplies viem-shaped contract reads.
   */
  async multicall<T extends readonly unknown[]>(
    chainId: number,
    contracts: Parameters<PublicClient["multicall"]>[0]["contracts"],
  ): Promise<T> {
    const client = this.clientFor(chainId);
    const results = await client.multicall({ contracts, allowFailure: false });
    return results as unknown as T;
  }
}

/** Singleton — resolver callers share one viem-backed client per chain. */
export const rpcClient: ViemRpcClient = new ViemRpcClient();

// ---------------------------------------------------------------------------
// Cache layer (disk JSON, git-tracked)
// ---------------------------------------------------------------------------

const CACHE_TTL_SECS = 30 * 24 * 60 * 60; // 30 days
const CACHE_ROOT = join(
  // resolved from `registryV2/scripts/resolvers/rpc.ts` → `registryV2/cache/`
  new URL("../../cache/protocol-sources/", import.meta.url).pathname,
);

function cachePathFor(scope: ProtocolSourceKind, chainId: number): string {
  // scope is `<protocol>:<inner>`; protocol-level subdir keeps related kinds together
  const protocol = scope.split(":")[0];
  return join(CACHE_ROOT, protocol, `${chainId}.${scope.split(":")[1]}.json`);
}

export function readCache(scope: ProtocolSourceKind, chainId: number): CacheEntry | undefined {
  const path = cachePathFor(scope, chainId);
  if (!existsSync(path)) return undefined;
  try {
    const raw = readFileSync(path, "utf8");
    const entry = JSON.parse(raw) as CacheEntry;
    if (entry.scope !== scope || entry.chainId !== chainId) {
      console.error(`[rpc] cache integrity warning at ${path} — scope/chainId mismatch, ignoring`);
      return undefined;
    }
    return entry;
  } catch (err) {
    console.error(`[rpc] cache read failed at ${path}: ${(err as Error).message}`);
    return undefined;
  }
}

export function isCacheFresh(entry: CacheEntry, ttlSecs = CACHE_TTL_SECS): boolean {
  const ageSecs = Math.floor(Date.now() / 1000) - entry.synced_at;
  return ageSecs >= 0 && ageSecs <= ttlSecs;
}

export function writeCache(entry: CacheEntry): void {
  const path = cachePathFor(entry.scope, entry.chainId);
  mkdirSync(dirname(path), { recursive: true });
  // Sorted address output for stable diffs
  const sorted: CacheEntry = {
    ...entry,
    addresses: [...entry.addresses].map((a) => a.toLowerCase()).sort() as Hex[],
  };
  writeFileSync(path, JSON.stringify(sorted, null, 2) + "\n", "utf8");
}

/**
 * Convenience wrapper for resolvers: returns cached addresses if fresh and
 * not force-refreshing, otherwise calls `fetchFresh` and persists the
 * resulting entry. On RPC failure with stale cache present, returns stale
 * cache + emits a warning (CI-friendly).
 */
export async function readOrFetch(
  scope: ProtocolSourceKind,
  chainId: number,
  forceRefresh: boolean,
  fetchFresh: () => Promise<CacheEntry>,
): Promise<CacheEntry> {
  const cached = readCache(scope, chainId);
  if (!forceRefresh && cached && isCacheFresh(cached)) {
    return cached;
  }
  try {
    const fresh = await fetchFresh();
    writeCache(fresh);
    return fresh;
  } catch (err) {
    if (cached) {
      console.error(
        `[rpc] ${scope} chain=${chainId} fetch failed (${(err as Error).message}); falling back to stale cache from ${new Date(cached.synced_at * 1000).toISOString()}`,
      );
      return cached;
    }
    throw new Error(
      `[rpc] ${scope} chain=${chainId} fetch failed and no cache present: ${(err as Error).message}`,
    );
  }
}
