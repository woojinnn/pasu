import { fetchNativeUsdPrices, fetchUsdPrices } from './coingecko-client';
import { lookup, store } from './price-cache';
import type { OracleEntry } from '@background/types/host-snapshot';

export interface OracleNeed {
  chainId: number;
  address: string;
  isNative?: boolean;
}

/**
 * Build a snapshot of oracle entries covering every (chainId, address) in
 * `needs`. Cache hits returned directly; misses split into ERC-20 contract
 * path and native /simple/price path. Tokens with no price available are
 * simply absent from the result (engine fail-open per optional-fact contract).
 */
export async function buildOracleSnapshot(
  needs: readonly OracleNeed[],
  fetchImpl: typeof fetch = fetch,
  nowMs: number = Date.now(),
): Promise<OracleEntry[]> {
  if (needs.length === 0) return [];

  // Dedup preserves the isNative flag — earlier draft dropped it, which
  // would silently lose USD coverage for native swaps.
  const dedup = new Map<string, OracleNeed>();
  for (const n of needs) {
    dedup.set(`${n.chainId}:${n.address.toLowerCase()}`, {
      chainId: n.chainId,
      address: n.address.toLowerCase(),
      isNative: !!n.isNative,
    });
  }
  const all = [...dedup.values()];

  const { hits, misses } = await lookup(all, nowMs);

  // Split misses into ERC-20 vs native paths. Native sentinel is preserved
  // so it threads back into OracleEntry.token_key for engine lookup.
  const missByChainErc20 = new Map<number, string[]>();
  const nativeMissChains = new Set<number>();
  const nativeSentinelByChain = new Map<number, string>();
  for (const k of misses) {
    const need = dedup.get(k);
    if (!need) continue;
    if (need.isNative) {
      nativeMissChains.add(need.chainId);
      nativeSentinelByChain.set(need.chainId, need.address);
    } else {
      const list = missByChainErc20.get(need.chainId) ?? [];
      list.push(need.address);
      missByChainErc20.set(need.chainId, list);
    }
  }

  const [erc20FetchResults, nativeFetchResults] = await Promise.all([
    Promise.all(
      [...missByChainErc20.entries()].map(async ([chainId, addrs]) => {
        const prices = await fetchUsdPrices(chainId, addrs, fetchImpl);
        return { chainId, prices };
      }),
    ),
    nativeMissChains.size > 0
      ? fetchNativeUsdPrices([...nativeMissChains], fetchImpl)
      : Promise.resolve([] as ReadonlyArray<Awaited<ReturnType<typeof fetchNativeUsdPrices>>[number]>),
  ]);

  const out: OracleEntry[] = [];
  const toStore: { chainId: number; address: string; usd: string; asOfTs: number }[] = [];
  const nowSec = Math.floor(nowMs / 1000);

  // Hits first.
  for (const [k, entry] of hits) {
    out.push({
      token_key: k,
      usd_per_unit: entry.usd,
      as_of_ts: entry.asOfTs,
      sources: ['coingecko'],
      stale_sec: Math.max(0, nowSec - entry.asOfTs),
    });
  }
  // ERC-20 misses.
  for (const { chainId, prices } of erc20FetchResults) {
    for (const p of prices) {
      const staleSec = Math.max(0, nowSec - p.asOfTs);
      out.push({
        token_key: `${chainId}:${p.address}`,
        usd_per_unit: p.usd,
        as_of_ts: p.asOfTs,
        sources: ['coingecko'],
        stale_sec: staleSec,
      });
      toStore.push({ chainId, address: p.address, usd: p.usd, asOfTs: p.asOfTs });
    }
  }
  // Native misses — thread sentinel address back into token_key.
  for (const np of nativeFetchResults) {
    const sentinel = nativeSentinelByChain.get(np.chainId);
    if (!sentinel) continue;
    const staleSec = Math.max(0, nowSec - np.asOfTs);
    out.push({
      token_key: `${np.chainId}:${sentinel}`,
      usd_per_unit: np.usd,
      as_of_ts: np.asOfTs,
      sources: ['coingecko-native'],
      stale_sec: staleSec,
    });
    toStore.push({ chainId: np.chainId, address: sentinel, usd: np.usd, asOfTs: np.asOfTs });
  }

  if (toStore.length > 0) await store(toStore, nowMs);
  return out;
}
