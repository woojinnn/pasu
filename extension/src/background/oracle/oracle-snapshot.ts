import {
  OracleFetchError,
  fetchNativeUsdPrices,
  fetchUsdPrices,
  nativePriceLastUpdatedAt,
  priceLastUpdatedAt,
} from "./coingecko-client";
import { cachedPriceLastUpdatedAt, lookup, store } from "./price-cache";
import { nativeFallbackTokenKey, tokenKey } from "./token-key";
import type { OracleEntry } from "../types/host-snapshot";

export const NATIVE_TOKEN_ADDRESS =
  "0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE";

export interface OracleNeed {
  chainId: number;
  address: string;
  isNative?: boolean;
}

interface NormalizedNeed {
  chainId: number;
  address: string;
  isNative: boolean;
}

function warnCoinGeckoFetchFailed(
  err: OracleFetchError,
  tokenKeys?: readonly string[],
): void {
  console.warn("[Scopeball SW] CoinGecko fetch failed", {
    tokenKeys: err.tokenKeys ?? tokenKeys,
    status: err.status,
    cause: String(err.cause),
  });
}

async function fetchUsdPricesOrEmpty(
  chainId: number,
  addresses: readonly string[],
  fetchImpl: typeof fetch,
): Promise<Map<string, number>> {
  try {
    return await fetchUsdPrices(chainId, addresses, fetchImpl);
  } catch (err) {
    if (err instanceof OracleFetchError) {
      warnCoinGeckoFetchFailed(
        err,
        addresses.map((address) => tokenKey({ chainId, address })),
      );
      return new Map<string, number>();
    }
    throw err;
  }
}

async function fetchNativeUsdPricesOrEmpty(
  nativeMisses: ReadonlyMap<number, string>,
  fetchImpl: typeof fetch,
): Promise<Map<number, number>> {
  if (nativeMisses.size === 0) return new Map<number, number>();

  const chainIds = [...nativeMisses.keys()];
  const tokenKeys = chainIds.map((chainId) => {
    const address = nativeMisses.get(chainId);
    return address
      ? tokenKey({ chainId, address, isNative: true })
      : nativeFallbackTokenKey(chainId);
  });

  try {
    return await fetchNativeUsdPrices(chainIds, fetchImpl, tokenKeys);
  } catch (err) {
    if (err instanceof OracleFetchError) {
      warnCoinGeckoFetchFailed(err, tokenKeys);
      return new Map<number, number>();
    }
    throw err;
  }
}

function staleSec(nowMs: number, lastUpdatedAtMs: number): number {
  return Math.max(0, Math.floor((nowMs - lastUpdatedAtMs) / 1000));
}

function entry(
  chainId: number,
  address: string,
  usdPrice: number,
  lastUpdatedAtMs: number,
  nowMs: number,
  sources: string[],
): OracleEntry {
  return {
    token_key: tokenKey({ chainId, address }),
    usd_price: usdPrice,
    usd_per_unit: String(usdPrice),
    as_of_ts: Math.floor(lastUpdatedAtMs / 1000),
    stale_sec: staleSec(nowMs, lastUpdatedAtMs),
    sources,
  };
}

export async function buildOracleSnapshot(
  needs: readonly OracleNeed[],
  fetchImpl: typeof fetch = fetch,
  nowMs: number = Date.now(),
): Promise<OracleEntry[]> {
  if (needs.length === 0) return [];

  const dedup = new Map<string, NormalizedNeed>();
  for (const need of needs) {
    const address = need.address.toLowerCase();
    dedup.set(tokenKey({ ...need, address }), {
      chainId: need.chainId,
      address,
      isNative: Boolean(need.isNative),
    });
  }

  const needsByChain = new Map<number, NormalizedNeed[]>();
  for (const need of dedup.values()) {
    const chainNeeds = needsByChain.get(need.chainId) ?? [];
    chainNeeds.push(need);
    needsByChain.set(need.chainId, chainNeeds);
  }

  const out: OracleEntry[] = [];
  const erc20Misses = new Map<number, string[]>();
  const nativeMisses = new Map<number, string>();

  await Promise.all(
    [...needsByChain.entries()].map(async ([chainId, chainNeeds]) => {
      const addresses = chainNeeds.map((need) => need.address);
      const { hits, misses } = await lookup(chainId, addresses, nowMs);
      for (const [address, usdPrice] of hits) {
        out.push(
          entry(
            chainId,
            address,
            usdPrice,
            cachedPriceLastUpdatedAt(hits, address) ?? nowMs,
            nowMs,
            ["coingecko"],
          ),
        );
      }

      for (const address of misses) {
        const need = chainNeeds.find(
          (candidate) => candidate.address === address,
        );
        if (!need) continue;
        if (need.isNative) {
          nativeMisses.set(chainId, address);
        } else {
          const chainMisses = erc20Misses.get(chainId) ?? [];
          chainMisses.push(address);
          erc20Misses.set(chainId, chainMisses);
        }
      }
    }),
  );

  const [erc20Results, nativeResults] = await Promise.all([
    Promise.all(
      [...erc20Misses.entries()].map(async ([chainId, addresses]) => ({
        chainId,
        prices: await fetchUsdPricesOrEmpty(chainId, addresses, fetchImpl),
      })),
    ),
    fetchNativeUsdPricesOrEmpty(nativeMisses, fetchImpl),
  ]);

  await Promise.all(
    erc20Results.map(async ({ chainId, prices }) => {
      const lastUpdated = new Map<string, number>();
      for (const [address, usdPrice] of prices) {
        const updatedAt = priceLastUpdatedAt(prices, address) ?? nowMs;
        lastUpdated.set(address, updatedAt);
        out.push(
          entry(chainId, address, usdPrice, updatedAt, nowMs, ["coingecko"]),
        );
      }
      await store(chainId, prices, nowMs, lastUpdated);
    }),
  );

  await Promise.all(
    [...nativeResults.entries()].map(async ([chainId, usdPrice]) => {
      const address = nativeMisses.get(chainId);
      if (!address) return;
      const updatedAt =
        nativePriceLastUpdatedAt(nativeResults, chainId) ?? nowMs;
      out.push(
        entry(chainId, address, usdPrice, updatedAt, nowMs, [
          "coingecko-native",
        ]),
      );
      await store(
        chainId,
        new Map([[address, usdPrice]]),
        nowMs,
        new Map([[address, updatedAt]]),
      );
    }),
  );

  return out;
}
