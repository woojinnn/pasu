import Browser from 'webextension-polyfill';

const STORAGE_KEY = 'oracle:price-cache';
const TTL_MS = 60_000;

interface CachedEntry {
  usd: string;
  asOfTs: number;
  cachedAtMs: number;
}

type CacheShape = Record<string, CachedEntry>; // key = `${chainId}:${addressLower}`

function key(chainId: number, address: string): string {
  return `${chainId}:${address.toLowerCase()}`;
}

async function load(): Promise<CacheShape> {
  const stored = await Browser.storage.local.get(STORAGE_KEY);
  return ((stored as Record<string, unknown>)[STORAGE_KEY] as CacheShape) ?? {};
}

async function save(cache: CacheShape): Promise<void> {
  await Browser.storage.local.set({ [STORAGE_KEY]: cache });
}

export interface CacheLookup {
  hits: Map<string, CachedEntry>;
  misses: string[];
}

export async function lookup(
  facts: ReadonlyArray<{ chainId: number; address: string }>,
  nowMs: number = Date.now(),
): Promise<CacheLookup> {
  const cache = await load();
  const hits = new Map<string, CachedEntry>();
  const misses: string[] = [];
  for (const f of facts) {
    const k = key(f.chainId, f.address);
    const entry = cache[k];
    if (entry && nowMs - entry.cachedAtMs < TTL_MS) {
      hits.set(k, entry);
    } else {
      misses.push(k);
    }
  }
  return { hits, misses };
}

export async function store(
  entries: ReadonlyArray<{ chainId: number; address: string; usd: string; asOfTs: number }>,
  nowMs: number = Date.now(),
): Promise<void> {
  if (entries.length === 0) return;
  const cache = await load();
  for (const e of entries) {
    cache[key(e.chainId, e.address)] = { usd: e.usd, asOfTs: e.asOfTs, cachedAtMs: nowMs };
  }
  await save(cache);
}

export async function clearAll(): Promise<void> {
  await Browser.storage.local.remove(STORAGE_KEY);
}

export const __test_internals__ = { TTL_MS };
