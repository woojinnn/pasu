import { priceLastUpdatedAt } from "./coingecko-client";

const TTL_MS = 60_000;
const hitUpdatedAt = new WeakMap<
  ReadonlyMap<string, number>,
  Map<string, number>
>();

interface CachedPrice {
  usd_price: number;
  last_updated_at: number;
  cached_at: number;
}

export interface CacheLookup {
  hits: Map<string, number>;
  misses: string[];
}

function storageKey(chainId: number, address: string): string {
  return `price:${chainId}:${address.toLowerCase()}`;
}

function storageArea(): chrome.storage.LocalStorageArea | undefined {
  return globalThis.chrome?.storage?.local;
}

async function storageGet(
  keys: string[],
): Promise<Record<string, CachedPrice | undefined>> {
  const area = storageArea();
  if (!area || keys.length === 0) return {};
  try {
    return (await area.get(keys)) as Record<string, CachedPrice | undefined>;
  } catch {
    return {};
  }
}

async function storageSet(entries: Record<string, CachedPrice>): Promise<void> {
  const area = storageArea();
  if (!area || Object.keys(entries).length === 0) return;
  try {
    await area.set(entries);
  } catch {
    // Cache writes are best-effort; callers must not fail closed on storage.
  }
}

async function storageRemove(keys: string[]): Promise<void> {
  const area = storageArea();
  if (!area || keys.length === 0) return;
  try {
    await area.remove(keys);
  } catch {
    // Best-effort test/helper cleanup.
  }
}

export function cachedPriceLastUpdatedAt(
  hits: ReadonlyMap<string, number>,
  address: string,
): number | undefined {
  return hitUpdatedAt.get(hits)?.get(address.toLowerCase());
}

export async function lookup(
  chainId: number,
  addresses: readonly string[],
  nowMs: number = Date.now(),
): Promise<CacheLookup> {
  const normalized = [
    ...new Set(addresses.map((address) => address.toLowerCase())),
  ];
  const stored = await storageGet(
    normalized.map((address) => storageKey(chainId, address)),
  );
  const hits = new Map<string, number>();
  const updated = new Map<string, number>();
  const misses: string[] = [];

  for (const address of normalized) {
    const entry = stored[storageKey(chainId, address)];
    if (entry && nowMs - entry.cached_at <= TTL_MS) {
      hits.set(address, entry.usd_price);
      updated.set(address, entry.last_updated_at);
    } else {
      misses.push(address);
    }
  }

  hitUpdatedAt.set(hits, updated);
  return { hits, misses };
}

export async function store(
  chainId: number,
  priceMap: ReadonlyMap<string, number>,
  nowMs: number = Date.now(),
  lastUpdatedAtByAddress?: ReadonlyMap<string, number>,
): Promise<void> {
  const entries: Record<string, CachedPrice> = {};
  for (const [address, usdPrice] of priceMap) {
    const lower = address.toLowerCase();
    entries[storageKey(chainId, lower)] = {
      usd_price: usdPrice,
      last_updated_at:
        lastUpdatedAtByAddress?.get(lower) ??
        priceLastUpdatedAt(priceMap, lower) ??
        nowMs,
      cached_at: nowMs,
    };
  }
  await storageSet(entries);
}

export async function clearAll(
  chainId: number,
  addresses: readonly string[],
): Promise<void> {
  await storageRemove(addresses.map((address) => storageKey(chainId, address)));
}

export const __test_internals__ = { TTL_MS, storageKey };
