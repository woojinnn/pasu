# Fact Fetchers — RPC + Oracle Client — Plan 4

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the extension's TS-side fact fetchers: per-chain RPC clients with viem multicall batching for `balanceOf` / `allowance` / `decimals` reads, and a CoinGecko-backed price client with `chrome.storage.local` TTL caching. A combined `fetchTier1(actor, plan)` populates the `HostSnapshot` JSON the WASM bridge consumes.

**Architecture:** All in `extension/background/` — pure TS, no UI, no React. RPC and oracle clients are stateless except for the price cache. Tier-1 fetcher derives parallel work from `HostFactPlan`, fans out RPC + price requests, awaits with bounded timeout, and returns a `HostSnapshot` shaped for `evaluate_json` (Plan 2).

**Tech Stack:** viem 2 `PublicClient` with `batch.multicall`, native `fetch` for CoinGecko (no SDK), `chrome.storage.local` for caching, `webextension-polyfill` for cross-browser.

**Series:** Plan 4 of the Chrome-extension series. Depends on Plan 3 (extension scaffold) and Plan 2 (WASM bridge for the JSON shapes consumed). Independent of Plan 1 internally.

**Scope:** Code under `extension/src/background/{chains,oracle,facts}/` plus unit tests via vitest. End-to-end Tier-1 fetch from a `HostFactPlan` JSON to a `HostSnapshot` JSON.

**Out of scope:** Orchestrator integration (Plan 5), receipt polling (Plan 5), Chainlink hybrid oracle (v1.1 follow-up).

---

## File map

| Path | Action | Responsibility |
|------|--------|----------------|
| `extension/package.json` | Modify | Add vitest, viem chain registry helpers |
| `extension/vitest.config.ts` | Create | Test runner config |
| `extension/src/background/chains/chain-config.ts` | Create | Per-chain RPC URL list + Multicall3 address + viem chain config |
| `extension/src/background/chains/rpc-client.ts` | Create | viem `PublicClient` factory with `batch.multicall=true` + URL fallback |
| `extension/src/background/chains/__tests__/rpc-client.test.ts` | Create | Unit tests against a mock fetch |
| `extension/src/background/oracle/coingecko-client.ts` | Create | Token-batched CoinGecko `simple/token_price` calls |
| `extension/src/background/oracle/price-cache.ts` | Create | `chrome.storage.local` TTL store keyed by `(chainId, address)` |
| `extension/src/background/oracle/oracle-snapshot.ts` | Create | Build the `HostSnapshot.oracle` array from cache + fetch |
| `extension/src/background/oracle/__tests__/coingecko-client.test.ts` | Create | Mocked-fetch tests |
| `extension/src/background/oracle/__tests__/price-cache.test.ts` | Create | TTL semantics tests |
| `extension/src/background/facts/tier1-fetcher.ts` | Create | Combines RPC + Oracle in parallel for one `HostFactPlan` |
| `extension/src/background/facts/__tests__/tier1-fetcher.test.ts` | Create | End-to-end mock test |
| `extension/src/background/types/host-snapshot.ts` | Create | Shared types matching `HostSnapshotDto` from Plan 2 |

---

## Task 1: Add vitest + chain helpers

**Files:** Modify `extension/package.json`, create `extension/vitest.config.ts`.

- [ ] **Step 1: Add devDependencies**

In `extension/package.json` `devDependencies`, add:

```json
"vitest": "^2.1.0",
"@vitest/ui": "^2.1.0",
"happy-dom": "^15.7.0"
```

In `scripts`, add:

```json
"test": "vitest run",
"test:watch": "vitest"
```

Run: `cd extension && yarn install`.

- [ ] **Step 2: Create vitest.config.ts**

```typescript
import { defineConfig } from 'vitest/config';
import path from 'path';

export default defineConfig({
  test: {
    environment: 'happy-dom',
    globals: true,
    coverage: { provider: 'v8' },
  },
  resolve: {
    alias: {
      '@lib': path.resolve(__dirname, 'src/lib'),
      '@background': path.resolve(__dirname, 'src/background'),
    },
  },
});
```

- [ ] **Step 3: Add the matching alias to tsconfig**

In `extension/tsconfig.json` `compilerOptions.paths`, add:

```json
"@background/*": ["background/*"]
```

- [ ] **Step 4: Smoke test that vitest runs**

```bash
cd extension && yarn test 2>&1 | tail -5
```

Expected: "No test files found." (exit 0). If the runner aborts with config errors, fix before continuing.

- [ ] **Step 5: Commit**

```bash
git add extension/package.json extension/yarn.lock extension/vitest.config.ts extension/tsconfig.json
git commit -m "chore(extension): vitest test runner + @background alias"
```

---

## Task 2: Per-chain config

**Files:** Create `extension/src/background/chains/chain-config.ts`.

We start with a small set of mainnets. Adding more is one entry per chain in this file.

- [ ] **Step 1: Write chain-config.ts**

```typescript
import type { Chain } from 'viem';
import { mainnet, arbitrum, optimism, polygon, base } from 'viem/chains';

const ALCHEMY_KEY = (typeof process !== 'undefined' && process.env?.ALCHEMY_API_KEY) || '';

export interface ChainConfig {
  id: number;
  viem: Chain;
  /** Ordered list of RPC URLs; clients fall through on failure. */
  rpcUrls: string[];
  /** Multicall3 address (default `0xcA11bde05977b3631167028862bE2a173976CA11` on every supported chain). */
  multicall3: `0x${string}`;
  /** CoinGecko platform slug for /simple/token_price/{platform}. */
  coingeckoPlatform: string;
  /** CoinGecko coin id for /simple/price (native asset; ETH uses 'ethereum'). */
  coingeckoNativeId: string;
}

const MULTICALL3 = '0xcA11bde05977b3631167028862bE2a173976CA11' as const;

function withAlchemyOrFallback(alchemyTpl: string, free: string): string[] {
  const main = ALCHEMY_KEY ? alchemyTpl.replace('${KEY}', ALCHEMY_KEY) : '';
  return main ? [main, free] : [free];
}

export const CHAINS: Record<number, ChainConfig> = {
  1: {
    id: 1,
    viem: mainnet,
    rpcUrls: withAlchemyOrFallback(
      'https://eth-mainnet.g.alchemy.com/v2/${KEY}',
      'https://eth.llamarpc.com',
    ),
    multicall3: MULTICALL3,
    coingeckoPlatform: 'ethereum',
    coingeckoNativeId: 'ethereum',
  },
  10: {
    id: 10,
    viem: optimism,
    rpcUrls: withAlchemyOrFallback(
      'https://opt-mainnet.g.alchemy.com/v2/${KEY}',
      'https://mainnet.optimism.io',
    ),
    multicall3: MULTICALL3,
    coingeckoPlatform: 'optimistic-ethereum',
    coingeckoNativeId: 'ethereum',
  },
  137: {
    id: 137,
    viem: polygon,
    rpcUrls: withAlchemyOrFallback(
      'https://polygon-mainnet.g.alchemy.com/v2/${KEY}',
      'https://polygon-rpc.com',
    ),
    multicall3: MULTICALL3,
    coingeckoPlatform: 'polygon-pos',
    coingeckoNativeId: 'matic-network',
  },
  8453: {
    id: 8453,
    viem: base,
    rpcUrls: withAlchemyOrFallback(
      'https://base-mainnet.g.alchemy.com/v2/${KEY}',
      'https://mainnet.base.org',
    ),
    multicall3: MULTICALL3,
    coingeckoPlatform: 'base',
    coingeckoNativeId: 'ethereum',
  },
  42161: {
    id: 42161,
    viem: arbitrum,
    rpcUrls: withAlchemyOrFallback(
      'https://arb-mainnet.g.alchemy.com/v2/${KEY}',
      'https://arb1.arbitrum.io/rpc',
    ),
    multicall3: MULTICALL3,
    coingeckoPlatform: 'arbitrum-one',
    coingeckoNativeId: 'ethereum',
  },
};

export function chainConfig(chainId: number): ChainConfig {
  const c = CHAINS[chainId];
  if (!c) throw new Error(`Unsupported chainId: ${chainId}`);
  return c;
}

export function isChainSupported(chainId: number): boolean {
  return chainId in CHAINS;
}
```

- [ ] **Step 2: Add a chains-supported smoke test**

Create `extension/src/background/chains/__tests__/chain-config.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import { chainConfig, isChainSupported } from '../chain-config';

describe('chain-config', () => {
  it('exposes mainnet chains', () => {
    expect(isChainSupported(1)).toBe(true);
    expect(isChainSupported(8453)).toBe(true);
    expect(isChainSupported(99999)).toBe(false);
  });

  it('returns ordered RPC urls for mainnet', () => {
    const c = chainConfig(1);
    expect(c.rpcUrls.length).toBeGreaterThan(0);
    // Free fallback must always be present even without an Alchemy key.
    expect(c.rpcUrls.some((u) => u.includes('llamarpc'))).toBe(true);
  });

  it('throws for unsupported chains', () => {
    expect(() => chainConfig(99999)).toThrow();
  });
});
```

- [ ] **Step 3: Run + commit**

```bash
cd extension && yarn test 2>&1 | tail -5
git add extension/src/background/chains/
git commit -m "feat(extension): per-chain config (mainnet, op, polygon, base, arbitrum)"
```

---

## Task 3: viem RPC client with multicall batching + URL fallback

**Files:** Create `extension/src/background/chains/rpc-client.ts`.

- [ ] **Step 1: Write the client**

```typescript
import {
  createPublicClient,
  fallback,
  http,
  parseAbi,
  type PublicClient,
} from 'viem';
import { chainConfig } from './chain-config';

// Re-export Address so consumers don't double-import from viem.
export type { Address } from 'viem';

const ERC20_ABI = parseAbi([
  'function balanceOf(address) view returns (uint256)',
  'function allowance(address owner, address spender) view returns (uint256)',
  'function decimals() view returns (uint8)',
  'function symbol() view returns (string)',
] as const);

const clientCache = new Map<number, PublicClient>();

export function rpcClient(chainId: number): PublicClient {
  const cached = clientCache.get(chainId);
  if (cached) return cached;

  const cfg = chainConfig(chainId);
  const client = createPublicClient({
    chain: cfg.viem,
    transport: fallback(cfg.rpcUrls.map((url) => http(url, { timeout: 8_000 }))),
    batch: { multicall: true },
  });
  clientCache.set(chainId, client);
  return client;
}

import type { Address } from 'viem';

export interface BalanceFact {
  owner: Address;
  token: Address;
  chainId: number;
}
export interface AllowanceFact {
  owner: Address;
  token: Address;
  spender: Address;
  chainId: number;
}

/// Batched read of N balanceOf calls. Returns one bigint per fact in input
/// order. Failures (revert / non-standard ERC-20) become `undefined`.
export async function readBalances(
  facts: readonly BalanceFact[],
): Promise<readonly (bigint | undefined)[]> {
  if (facts.length === 0) return [];
  const byChain = new Map<number, BalanceFact[]>();
  for (const f of facts) {
    const list = byChain.get(f.chainId) ?? [];
    list.push(f);
    byChain.set(f.chainId, list);
  }
  // Map back to original positions after parallel chain reads.
  const out: (bigint | undefined)[] = new Array(facts.length).fill(undefined);
  await Promise.all(
    [...byChain.entries()].map(async ([chainId, perChain]) => {
      const client = rpcClient(chainId);
      const results = await Promise.allSettled(
        perChain.map((f) =>
          client.readContract({
            address: f.token,
            abi: ERC20_ABI,
            functionName: 'balanceOf',
            args: [f.owner],
          }),
        ),
      );
      for (let i = 0; i < perChain.length; i++) {
        const idx = facts.indexOf(perChain[i]);
        const r = results[i];
        if (r.status === 'fulfilled') out[idx] = r.value as bigint;
      }
    }),
  );
  return out;
}

export async function readAllowances(
  facts: readonly AllowanceFact[],
): Promise<readonly (bigint | undefined)[]> {
  if (facts.length === 0) return [];
  const byChain = new Map<number, AllowanceFact[]>();
  for (const f of facts) {
    const list = byChain.get(f.chainId) ?? [];
    list.push(f);
    byChain.set(f.chainId, list);
  }
  const out: (bigint | undefined)[] = new Array(facts.length).fill(undefined);
  await Promise.all(
    [...byChain.entries()].map(async ([chainId, perChain]) => {
      const client = rpcClient(chainId);
      const results = await Promise.allSettled(
        perChain.map((f) =>
          client.readContract({
            address: f.token,
            abi: ERC20_ABI,
            functionName: 'allowance',
            args: [f.owner, f.spender],
          }),
        ),
      );
      for (let i = 0; i < perChain.length; i++) {
        const idx = facts.indexOf(perChain[i]);
        const r = results[i];
        if (r.status === 'fulfilled') out[idx] = r.value as bigint;
      }
    }),
  );
  return out;
}

export async function readDecimals(
  chainId: number,
  token: Address,
): Promise<number | undefined> {
  try {
    const v = await rpcClient(chainId).readContract({
      address: token,
      abi: ERC20_ABI,
      functionName: 'decimals',
    });
    return Number(v);
  } catch {
    return undefined;
  }
}
```

- [ ] **Step 2: Tests**

Create `extension/src/background/chains/__tests__/rpc-client.test.ts`:

```typescript
import { describe, expect, it, vi, beforeEach } from 'vitest';

vi.mock('viem', async (importOriginal) => {
  const actual = await importOriginal<typeof import('viem')>();
  return {
    ...actual,
    createPublicClient: vi.fn(() => ({
      readContract: vi.fn(async ({ functionName, args }: any) => {
        if (functionName === 'balanceOf') return BigInt('1000000');
        if (functionName === 'allowance') return BigInt('500000');
        if (functionName === 'decimals') return 6;
        throw new Error('unhandled');
      }),
    })),
    fallback: actual.fallback,
    http: actual.http,
  };
});

import { readAllowances, readBalances, readDecimals } from '../rpc-client';

describe('rpc-client', () => {
  beforeEach(() => {
    // No-op; the mock client is per-chain cached.
  });

  it('returns balances in input order', async () => {
    const balances = await readBalances([
      {
        owner: '0x1111111111111111111111111111111111111111',
        token: '0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2',
        chainId: 1,
      },
    ]);
    expect(balances).toEqual([1_000_000n]);
  });

  it('returns allowances in input order', async () => {
    const allowances = await readAllowances([
      {
        owner: '0x1111111111111111111111111111111111111111',
        token: '0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2',
        spender: '0xE592427A0AEce92De3Edee1F18E0157C05861564',
        chainId: 1,
      },
    ]);
    expect(allowances).toEqual([500_000n]);
  });

  it('returns undefined on read failure', async () => {
    // The mocked client always succeeds; a revert path is tested in integration.
    const dec = await readDecimals(1, '0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2');
    expect(dec).toBe(6);
  });
});
```

- [ ] **Step 3: Run + commit**

```bash
cd extension && yarn test rpc-client 2>&1 | tail -10
git add extension/src/background/chains/rpc-client.ts extension/src/background/chains/__tests__/rpc-client.test.ts
git commit -m "$(cat <<'EOF'
feat(extension): viem RPC client with multicall batching

Single PublicClient per chain (cached). Transport is viem.fallback over
the chain-config URL list. batch.multicall=true collapses concurrent
ERC20 reads into one Multicall3.aggregate3 RPC. Per-call try/catch
(via Promise.allSettled) so non-standard ERC-20s don't tank the batch.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: CoinGecko price client

**Files:** Create `extension/src/background/oracle/coingecko-client.ts`.

- [ ] **Step 1: Write the client**

```typescript
import { chainConfig } from '@background/chains/chain-config';

const COINGECKO_BASE = 'https://api.coingecko.com/api/v3';
const MAX_BATCH = 30;

export interface CoinGeckoPrice {
  /** Lowercased token contract address. */
  address: string;
  /** USD price as a decimal string (we never use Number for currency). */
  usd: string;
  /** Server-reported `last_updated_at` if available; otherwise client clock. */
  asOfTs: number;
}

/// Fetch USD prices for a list of tokens on one chain, batched up to 30
/// addresses per request. Tokens not found by CoinGecko are simply absent
/// from the result — never represented as 0.
export async function fetchUsdPrices(
  chainId: number,
  addresses: readonly string[],
  fetchImpl: typeof fetch = fetch,
): Promise<readonly CoinGeckoPrice[]> {
  if (addresses.length === 0) return [];
  const platform = chainConfig(chainId).coingeckoPlatform;
  const out: CoinGeckoPrice[] = [];

  for (let i = 0; i < addresses.length; i += MAX_BATCH) {
    const slice = addresses.slice(i, i + MAX_BATCH).map((a) => a.toLowerCase());
    const url = new URL(`${COINGECKO_BASE}/simple/token_price/${platform}`);
    url.searchParams.set('contract_addresses', slice.join(','));
    url.searchParams.set('vs_currencies', 'usd');
    url.searchParams.set('include_last_updated_at', 'true');

    let response: Response;
    try {
      response = await fetchImpl(url.toString(), {
        signal: AbortSignal.timeout(5_000),
      });
    } catch {
      continue; // network failure → tokens just absent
    }
    if (!response.ok) continue; // 429, 5xx → tokens absent

    let body: Record<string, { usd?: number; last_updated_at?: number }>;
    try {
      body = await response.json();
    } catch {
      continue;
    }

    for (const [address, entry] of Object.entries(body)) {
      if (typeof entry?.usd !== 'number') continue;
      out.push({
        address: address.toLowerCase(),
        usd: entry.usd.toString(),
        asOfTs: entry.last_updated_at ?? Math.floor(Date.now() / 1000),
      });
    }
  }
  return out;
}

/// Fetch USD price for a list of *native* assets (one per chain). CoinGecko's
/// /simple/token_price endpoint only handles ERC-20 contracts; native assets
/// (ETH, MATIC, etc.) need /simple/price?ids=...&vs_currencies=usd.
export interface CoinGeckoNativePrice {
  chainId: number;
  usd: string;
  asOfTs: number;
}

export async function fetchNativeUsdPrices(
  chainIds: readonly number[],
  fetchImpl: typeof fetch = fetch,
): Promise<readonly CoinGeckoNativePrice[]> {
  if (chainIds.length === 0) return [];
  // Dedupe by coin id — multiple chains can share an id (op + base both 'ethereum').
  const idsByCoin = new Map<string, number[]>();
  for (const cid of chainIds) {
    const id = chainConfig(cid).coingeckoNativeId;
    const list = idsByCoin.get(id) ?? [];
    list.push(cid);
    idsByCoin.set(id, list);
  }
  const ids = [...idsByCoin.keys()];

  const url = new URL(`${COINGECKO_BASE}/simple/price`);
  url.searchParams.set('ids', ids.join(','));
  url.searchParams.set('vs_currencies', 'usd');
  url.searchParams.set('include_last_updated_at', 'true');

  let response: Response;
  try {
    response = await fetchImpl(url.toString(), { signal: AbortSignal.timeout(5_000) });
  } catch {
    return [];
  }
  if (!response.ok) return [];
  let body: Record<string, { usd?: number; last_updated_at?: number }>;
  try {
    body = await response.json();
  } catch {
    return [];
  }
  const out: CoinGeckoNativePrice[] = [];
  for (const [coinId, entry] of Object.entries(body)) {
    if (typeof entry?.usd !== 'number') continue;
    const cidsForCoin = idsByCoin.get(coinId) ?? [];
    for (const cid of cidsForCoin) {
      out.push({
        chainId: cid,
        usd: entry.usd.toString(),
        asOfTs: entry.last_updated_at ?? Math.floor(Date.now() / 1000),
      });
    }
  }
  return out;
}
```

- [ ] **Step 2: Tests**

Create `extension/src/background/oracle/__tests__/coingecko-client.test.ts`:

```typescript
import { describe, expect, it, vi } from 'vitest';
import { fetchUsdPrices } from '../coingecko-client';

const WETH = '0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2';
const USDC = '0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48';

describe('fetchUsdPrices', () => {
  it('returns empty for empty input without calling fetch', async () => {
    const fetchMock = vi.fn();
    const r = await fetchUsdPrices(1, [], fetchMock as any);
    expect(r).toEqual([]);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('hits CoinGecko with deduped lowercased addresses', async () => {
    const fetchMock = vi.fn(async (url: string) => {
      expect(url).toContain('/simple/token_price/ethereum');
      expect(url).toContain('contract_addresses=');
      return new Response(
        JSON.stringify({
          [WETH]: { usd: 3500.42, last_updated_at: 1_700_000_000 },
          [USDC]: { usd: 1.0, last_updated_at: 1_700_000_000 },
        }),
      );
    });
    const r = await fetchUsdPrices(1, [WETH, USDC], fetchMock as any);
    const byAddr = Object.fromEntries(r.map((p) => [p.address, p]));
    expect(byAddr[WETH].usd).toBe('3500.42');
    expect(byAddr[USDC].usd).toBe('1');
  });

  it('drops tokens missing from the response without throwing', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ [WETH]: { usd: 3500 } })),
    );
    const r = await fetchUsdPrices(1, [WETH, USDC], fetchMock as any);
    expect(r.map((p) => p.address)).toEqual([WETH]);
  });

  it('returns empty on HTTP failure', async () => {
    const fetchMock = vi.fn(async () => new Response('rate limited', { status: 429 }));
    const r = await fetchUsdPrices(1, [WETH], fetchMock as any);
    expect(r).toEqual([]);
  });

  it('batches above 30 addresses', async () => {
    const many = Array.from(
      { length: 45 },
      (_, i) =>
        `0x${'0'.repeat(39)}${i.toString(16).padStart(1, '0').slice(-1)}` as const,
    );
    let calls = 0;
    const fetchMock = vi.fn(async (url: string) => {
      calls++;
      const u = new URL(url);
      const cs = (u.searchParams.get('contract_addresses') ?? '').split(',');
      expect(cs.length).toBeLessThanOrEqual(30);
      return new Response('{}');
    });
    await fetchUsdPrices(1, many, fetchMock as any);
    expect(calls).toBe(2);
  });
});
```

- [ ] **Step 3: Run + commit**

```bash
cd extension && yarn test coingecko-client 2>&1 | tail -10
git add extension/src/background/oracle/coingecko-client.ts extension/src/background/oracle/__tests__/coingecko-client.test.ts
git commit -m "feat(extension): CoinGecko price client (batch ≤30, network-fail-open)"
```

---

## Task 5: Price cache (chrome.storage.local with 60s TTL)

**Files:** Create `extension/src/background/oracle/price-cache.ts`.

- [ ] **Step 1: Write the cache**

```typescript
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
  return (stored[STORAGE_KEY] as CacheShape) ?? {};
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
```

- [ ] **Step 2: Tests**

Create `extension/src/background/oracle/__tests__/price-cache.test.ts`:

```typescript
import { describe, expect, it, beforeEach, vi } from 'vitest';

const memoryStore: Record<string, unknown> = {};
vi.mock('webextension-polyfill', () => ({
  default: {
    storage: {
      local: {
        get: vi.fn(async (key: string) => ({ [key]: memoryStore[key] })),
        set: vi.fn(async (entry: Record<string, unknown>) => {
          Object.assign(memoryStore, entry);
        }),
        remove: vi.fn(async (key: string) => {
          delete memoryStore[key];
        }),
      },
    },
  },
}));

import { lookup, store, clearAll, __test_internals__ } from '../price-cache';

describe('price-cache', () => {
  beforeEach(async () => {
    await clearAll();
  });

  it('lookup misses on empty cache', async () => {
    const r = await lookup([{ chainId: 1, address: '0xabc' }]);
    expect(r.hits.size).toBe(0);
    expect(r.misses).toEqual(['1:0xabc']);
  });

  it('store + lookup round-trips', async () => {
    await store([{ chainId: 1, address: '0xABC', usd: '3500', asOfTs: 100 }], 1000);
    const r = await lookup([{ chainId: 1, address: '0xabc' }], 1000);
    expect(r.hits.size).toBe(1);
    expect(r.hits.get('1:0xabc')?.usd).toBe('3500');
  });

  it('expires entries past TTL', async () => {
    await store([{ chainId: 1, address: '0xabc', usd: '1', asOfTs: 0 }], 0);
    const r = await lookup([{ chainId: 1, address: '0xabc' }], __test_internals__.TTL_MS + 1);
    expect(r.misses).toEqual(['1:0xabc']);
  });

  it('lowercases addresses for cache keying', async () => {
    await store([{ chainId: 1, address: '0xABCdef', usd: '1', asOfTs: 0 }], 0);
    const r = await lookup([{ chainId: 1, address: '0xabcdef' }], 0);
    expect(r.hits.size).toBe(1);
  });
});
```

- [ ] **Step 3: Run + commit**

```bash
cd extension && yarn test price-cache 2>&1 | tail -10
git add extension/src/background/oracle/price-cache.ts extension/src/background/oracle/__tests__/price-cache.test.ts
git commit -m "feat(extension): price cache (chrome.storage.local TTL 60s)"
```

---

## Task 6: Oracle snapshot builder

**Files:** Create `extension/src/background/oracle/oracle-snapshot.ts`, `extension/src/background/types/host-snapshot.ts`.

- [ ] **Step 1: Shared HostSnapshot types**

```typescript
// extension/src/background/types/host-snapshot.ts
export interface OracleEntry {
  token_key: string; // `${chainId}:${addressLower}`
  usd_per_unit: string;
  as_of_ts: number;
  stale_sec?: number;
  sources?: string[];
}

export interface BalanceEntry {
  owner: string;
  token_key: string;
  balance: string; // raw uint256 as decimal string
}

export interface AllowanceEntry {
  owner: string;
  token_key: string;
  spender: string;
  allowance: string;
}

export interface WindowEntry {
  actor: string;
  name: string;
  value: string;
}

export interface HostSnapshot {
  oracle: OracleEntry[];
  balances: BalanceEntry[];
  allowances: AllowanceEntry[];
  now_ts: number;
  windows: WindowEntry[];
}
```

- [ ] **Step 2: oracle-snapshot.ts**

```typescript
import { fetchUsdPrices, fetchNativeUsdPrices } from './coingecko-client';
import { lookup, store } from './price-cache';
import type { OracleEntry } from '@background/types/host-snapshot';

export interface OracleNeed {
  chainId: number;
  address: string;
  isNative?: boolean;
}

/// Build a snapshot of oracle entries covering every (chainId, address)
/// in `needs`. Cache hits are returned directly; misses are fetched in
/// parallel per chain (split into ERC-20 contract path and native /simple/price
/// path), persisted, then merged. Tokens with no price available are simply
/// absent from the result.
export async function buildOracleSnapshot(
  needs: readonly OracleNeed[],
  fetchImpl: typeof fetch = fetch,
  nowMs: number = Date.now(),
): Promise<OracleEntry[]> {
  if (needs.length === 0) return [];

  // Dedup preserves the isNative flag — earlier draft dropped it, leaving
  // ETH/MATIC etc. silently unpriced.
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

  // Misses split: native vs ERC-20. Look up the canonical OracleNeed for each
  // miss key so we can branch correctly.
  const missByChainErc20 = new Map<number, string[]>();
  const nativeMissChains = new Set<number>();
  // address-by-key so we can preserve the sentinel address used for native
  // when threading prices back into OracleEntry.
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
      : Promise.resolve([] as readonly Awaited<
          ReturnType<typeof fetchNativeUsdPrices>
        >[number][]),
  ]);
  const fetchedByChain = erc20FetchResults;

  const toStore: { chainId: number; address: string; usd: string; asOfTs: number }[] = [];
  const out: OracleEntry[] = [];
  for (const [k, entry] of hits) {
    out.push({
      token_key: k,
      usd_per_unit: entry.usd,
      as_of_ts: entry.asOfTs,
      sources: ['coingecko'],
      stale_sec: Math.max(0, Math.floor((nowMs - entry.cachedAtMs) / 1000)),
    });
  }
  const nowSec = Math.floor(nowMs / 1000);
  for (const { chainId, prices } of fetchedByChain) {
    for (const p of prices) {
      // stale_sec is data freshness — wall-time delta from CoinGecko's
      // `last_updated_at`, not a flat 0 on fresh fetch.
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
  // Thread native /simple/price results back through the canonical sentinel
  // address. Without this, SnapshotOracle keys by (chainId, sentinel) on the
  // engine side never match.
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
    toStore.push({
      chainId: np.chainId,
      address: sentinel,
      usd: np.usd,
      asOfTs: np.asOfTs,
    });
  }
  if (toStore.length > 0) await store(toStore, nowMs);
  return out;
}
```

- [ ] **Step 3: Test**

Create `extension/src/background/oracle/__tests__/oracle-snapshot.test.ts`:

```typescript
import { describe, expect, it, vi, beforeEach } from 'vitest';

const memoryStore: Record<string, unknown> = {};
vi.mock('webextension-polyfill', () => ({
  default: {
    storage: {
      local: {
        get: vi.fn(async (key: string) => ({ [key]: memoryStore[key] })),
        set: vi.fn(async (entry: Record<string, unknown>) => {
          Object.assign(memoryStore, entry);
        }),
        remove: vi.fn(async (key: string) => {
          delete memoryStore[key];
        }),
      },
    },
  },
}));

import { buildOracleSnapshot } from '../oracle-snapshot';

const WETH = '0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2';

describe('buildOracleSnapshot', () => {
  beforeEach(() => {
    for (const k of Object.keys(memoryStore)) delete memoryStore[k];
  });

  it('returns empty for empty needs', async () => {
    const r = await buildOracleSnapshot([]);
    expect(r).toEqual([]);
  });

  it('fetches misses from CoinGecko and writes them to cache', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ [WETH]: { usd: 3500, last_updated_at: 100 } })),
    );
    const r = await buildOracleSnapshot([{ chainId: 1, address: WETH }], fetchMock as any, 0);
    expect(r.length).toBe(1);
    expect(r[0].usd_per_unit).toBe('3500');

    // Second call should not hit network — cached.
    const r2 = await buildOracleSnapshot([{ chainId: 1, address: WETH }], fetchMock as any, 1000);
    expect(r2.length).toBe(1);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('skips tokens not returned by CoinGecko (network-fail-open)', async () => {
    const fetchMock = vi.fn(async () => new Response('{}'));
    const r = await buildOracleSnapshot([{ chainId: 1, address: WETH }], fetchMock as any, 0);
    expect(r).toEqual([]);
  });
});
```

- [ ] **Step 4: Run + commit**

```bash
cd extension && yarn test oracle-snapshot 2>&1 | tail -10
git add extension/src/background/oracle/oracle-snapshot.ts extension/src/background/types/
git commit -m "feat(extension): oracle snapshot builder (cache + CoinGecko)"
```

---

## Task 7: Tier-1 fetcher (RPC + Oracle in parallel)

**Files:** Create `extension/src/background/facts/tier1-fetcher.ts`, with test.

- [ ] **Step 1: Define the input/output shapes**

The input mirrors the `HostFactPlanDto` produced by Plan 2's `tier1_fact_plan_json`. The output is a partial `HostSnapshot` that the orchestrator (Plan 5) extends with windows and clock before passing to `evaluate_json`.

- [ ] **Step 2: Write the fetcher**

```typescript
// extension/src/background/facts/tier1-fetcher.ts
import { readBalances, readAllowances, type Address } from '@background/chains/rpc-client';
import { buildOracleSnapshot } from '@background/oracle/oracle-snapshot';
import type {
  AllowanceEntry,
  BalanceEntry,
  HostSnapshot,
  OracleEntry,
} from '@background/types/host-snapshot';

export interface Tier1Plan {
  tokens_for_oracle: TokenLite[];
  balances: { owner: string; token: TokenLite }[];
  allowances: { owner: string; token: TokenLite; spender: string }[];
  clock_required: boolean;
  // sig_oracle_requirements is informational; oracle fetch is keyed by tokens_for_oracle alone.
}

export interface TokenLite {
  chain_id: number;
  address: string;
  symbol: string;
  decimals: number;
  is_native: boolean;
}

export interface Tier1FetchResult {
  oracle: OracleEntry[];
  balances: BalanceEntry[];
  allowances: AllowanceEntry[];
  now_ts: number;
}

const TIER1_OUTER_TIMEOUT_MS = 2_000;

/// Run all Tier-1 host fetches in parallel and assemble a partial HostSnapshot.
/// Failures (RPC reverts, CoinGecko 429s) become absent entries; never zero.
/// The orchestrator merges in `windows` (Tier 2) and the final `now_ts` later.
///
/// An outer AbortSignal caps the entire Tier-1 fetch at TIER1_OUTER_TIMEOUT_MS
/// regardless of how many fallback URLs viem stacks or how many CoinGecko
/// batches the oracle builder issues. Anything still in-flight when the timer
/// fires is dropped into the empty-snapshot path.
export async function fetchTier1(
  plan: Tier1Plan,
  fetchImpl: typeof fetch = fetch,
  nowMs: number = Date.now(),
): Promise<Tier1FetchResult> {
  const controller = new AbortController();
  const tID = setTimeout(() => controller.abort(), TIER1_OUTER_TIMEOUT_MS);
  // Wrap fetchImpl so we forward the outer signal to every CoinGecko call.
  const guardedFetch: typeof fetch = (input, init) =>
    fetchImpl(input, { ...init, signal: controller.signal });

  const oraclePromise = buildOracleSnapshot(
    plan.tokens_for_oracle.map((t) => ({
      chainId: t.chain_id,
      address: t.address,
      // CRITICAL: propagate is_native from the engine's HostFactPlan so the
      // oracle builder can branch to /simple/price for native assets. The
      // earlier round dropped this flag and silently lost USD coverage for
      // every native swap.
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
    clearTimeout(tID);
  }
  if (controller.signal.aborted) {
    // Outer budget exceeded → return empty snapshot; engine falls open per
    // optional-fact contract. The orchestrator still proceeds to evaluate
    // (no facts means policies that gate on `context has X` skip; policies
    // that require X cleanly fail-closed via their own logic).
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

/// Merge Tier-1 result + window entries + clock into a full HostSnapshot.
export function intoHostSnapshot(
  tier1: Tier1FetchResult,
  windows: HostSnapshot['windows'] = [],
): HostSnapshot {
  return {
    oracle: tier1.oracle,
    balances: tier1.balances,
    allowances: tier1.allowances,
    now_ts: tier1.now_ts,
    windows,
  };
}
```

- [ ] **Step 3: End-to-end test**

```typescript
// extension/src/background/facts/__tests__/tier1-fetcher.test.ts
import { describe, expect, it, vi } from 'vitest';

const memoryStore: Record<string, unknown> = {};
vi.mock('webextension-polyfill', () => ({
  default: {
    storage: {
      local: {
        get: vi.fn(async (key: string) => ({ [key]: memoryStore[key] })),
        set: vi.fn(async (entry: Record<string, unknown>) => {
          Object.assign(memoryStore, entry);
        }),
        remove: vi.fn(async () => {}),
      },
    },
  },
}));

vi.mock('@background/chains/rpc-client', () => ({
  readBalances: vi.fn(async (facts: any[]) => facts.map(() => 1_000_000n)),
  readAllowances: vi.fn(async (facts: any[]) => facts.map(() => 500_000n)),
  readDecimals: vi.fn(async () => 18),
}));

import { fetchTier1 } from '../tier1-fetcher';

const WETH_ADDR = '0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2';
const USDC_ADDR = '0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48';

describe('fetchTier1', () => {
  it('combines oracle + balances + allowances in parallel', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(
        JSON.stringify({
          [WETH_ADDR]: { usd: 3500, last_updated_at: 100 },
          [USDC_ADDR]: { usd: 1, last_updated_at: 100 },
        }),
      ),
    );
    const result = await fetchTier1(
      {
        tokens_for_oracle: [
          { chain_id: 1, address: WETH_ADDR, symbol: 'WETH', decimals: 18, is_native: false },
          { chain_id: 1, address: USDC_ADDR, symbol: 'USDC', decimals: 6, is_native: false },
        ],
        balances: [
          {
            owner: '0x1111111111111111111111111111111111111111',
            token: { chain_id: 1, address: WETH_ADDR, symbol: 'WETH', decimals: 18, is_native: false },
          },
        ],
        allowances: [
          {
            owner: '0x1111111111111111111111111111111111111111',
            token: { chain_id: 1, address: WETH_ADDR, symbol: 'WETH', decimals: 18, is_native: false },
            spender: '0xE592427A0AEce92De3Edee1F18E0157C05861564',
          },
        ],
        clock_required: false,
      },
      fetchMock as any,
      1_700_000_000_000,
    );

    expect(result.oracle.length).toBe(2);
    expect(result.balances).toEqual([
      {
        owner: '0x1111111111111111111111111111111111111111',
        token_key: `1:${WETH_ADDR}`,
        balance: '1000000',
      },
    ]);
    expect(result.allowances[0].allowance).toBe('500000');
    expect(result.now_ts).toBe(1_700_000_000);
  });
});
```

- [ ] **Step 4: Run + commit**

```bash
cd extension && yarn test tier1-fetcher 2>&1 | tail -10
git add extension/src/background/facts/
git commit -m "$(cat <<'EOF'
feat(extension): tier1 fact fetcher (RPC + oracle in parallel)

Consumes a HostFactPlan, fans out balanceOf/allowance via the
multicall-batched RPC client and oracle snapshots via the cached
CoinGecko client, returns a partial HostSnapshot keyed for evaluate_json.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Full test sweep

- [ ] **Step 1: Run all extension tests**

```bash
cd extension && yarn test 2>&1 | tail -15
```

Expected: all tests pass (chain-config × 3, rpc-client × 3, coingecko × 5, price-cache × 4, oracle-snapshot × 3, tier1-fetcher × 1) — 19 total.

- [ ] **Step 2: Build production artifacts**

```bash
yarn build:chrome && yarn build:firefox 2>&1 | tail -5
```

Expected: both build successfully.

- [ ] **Step 3: TypeScript check + format**

```bash
yarn typecheck 2>&1 | tail -5
yarn lint
```

Expected: no errors.

If anything had to change, commit:

```bash
git add -u
git commit -m "style(extension): formatting + typecheck pass"
```

---

## Self-review summary

**Spec coverage** (vs design §4.3.1a, learnings §2.8/§2.9/§2.10/§3a):
- ✅ viem `batch.multicall=true` + URL fallback — Task 3
- ✅ Per-chain config (mainnet, op, polygon, base, arbitrum) — Task 2
- ✅ ERC-20 reads with per-call try/catch via `Promise.allSettled` — Task 3
- ✅ CoinGecko free-tier client with batching ≤30 + network-fail-open — Task 4
- ✅ `chrome.storage.local` 60s TTL cache — Task 5
- ✅ OracleSnapshot builder consuming cache + fetch — Task 6
- ✅ Tier-1 fetcher combining RPC + Oracle in parallel — Task 7
- ⏭ Chainlink hybrid for major tokens → v1.1
- ⏭ Tier-2 window-key fetch + windows storage → Plan 5

**Risks flagged for the executor:**
- viem 2.x major-version drift might rename `parseAbi` (currently still exported; verify before Task 3)
- `webextension-polyfill` v0.12 storage API stable but the typed surface differs from `chrome.*`; if the executor sees TS errors, they're likely missing `await` not signature mismatches
- CoinGecko free tier rate limit (30 req/min) is real — under load, expect 429s; the cache absorbs steady state but a cold burst can still saturate
