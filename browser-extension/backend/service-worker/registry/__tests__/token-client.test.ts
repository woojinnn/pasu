/**
 * Phase 7D — Token registry client cases.
 *
 * Coverage matrix:
 *   - HTTP 200 + valid JSON  → metadata (Layer 2 hit, persisted)
 *   - HTTP 404               → null + no_publisher (5 min)
 *   - Cache hit              → second call serves from in-process cache
 *   - Inflight dedupe        → concurrent same-key calls share one fetch
 *   - Schema invalid         → null + integrity_failed (5 min)
 *   - Address case folding   → mixed-case input → single cache slot
 *   - Network error          → null + timeout (30 s)
 *
 * `Browser.storage.local` is mocked via the standard policies-loader
 * pattern (`Map<string, unknown>` backing `get/set`).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();
  return {
    localStore,
    browser: {
      runtime: { getURL: (p: string) => `chrome-extension://x/${p}` },
      storage: {
        local: {
          get: vi.fn(async (key: string) => ({ [key]: localStore.get(key) })),
          set: vi.fn(async (entries: Record<string, unknown>) => {
            for (const [k, v] of Object.entries(entries)) localStore.set(k, v);
          }),
        },
      },
    },
  };
});

vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));

import {
  __peekTokenNegativeCacheForTest,
  __resetTokenRegistryClientForTest,
  createTokenRegistryClient,
  defaultTokenRegistryClient,
  type TokenMetadata,
} from "../token-client";

const WETH_MAINNET = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const WETH_PAYLOAD: TokenMetadata = {
  kind: "erc20",
  chainId: 1,
  address: WETH_MAINNET,
  symbol: "WETH",
  decimals: 18,
  name: "Wrapped Ether",
};
const USDC_MAINNET = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
const USDC_PAYLOAD: TokenMetadata = {
  kind: "erc20",
  chainId: 1,
  address: USDC_MAINNET,
  symbol: "USDC",
  decimals: 6,
  name: "USD Coin",
};
const UNKNOWN_ADDR = "0x0000000000000000000000000000000000000099";

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("TokenRegistryClient.lookup", () => {
  let fetchMock: ReturnType<typeof vi.fn<typeof fetch>>;

  beforeEach(() => {
    vi.clearAllMocks();
    mocks.localStore.clear();
    __resetTokenRegistryClientForTest();
    fetchMock = vi.fn<typeof fetch>();
  });

  it("hit (200 OK): returns parsed TokenMetadata and persists to storage", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse(WETH_PAYLOAD));

    const client = createTokenRegistryClient({ fetchImpl: fetchMock });
    const got = await client.lookup(1, WETH_MAINNET);

    expect(got).toEqual(WETH_PAYLOAD);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    // URL must follow `/tokens/<chainId>/<lower(address)>.json`
    const calledUrl = String(fetchMock.mock.calls[0][0]);
    expect(calledUrl).toContain(`/tokens/1/${WETH_MAINNET}.json`);
    // Storage persisted (Browser.storage.local.set was called)
    expect(mocks.browser.storage.local.set).toHaveBeenCalled();
    const stored = mocks.localStore.get("registry:tokens") as
      | Record<string, TokenMetadata>
      | undefined;
    expect(stored).toBeTruthy();
    expect(stored![`1__${WETH_MAINNET}`]).toEqual(WETH_PAYLOAD);
  });

  it("404 → null + negative cache no_publisher (subsequent call short-circuits)", async () => {
    fetchMock.mockResolvedValue(new Response("nope", { status: 404 }));

    // Use the singleton so the negative-cache peek helper sees the entry.
    const client = defaultTokenRegistryClient();
    // Inject the fetch impl by going through the singleton's first call —
    // because the singleton holds default options, we replace global fetch
    // for this case.
    const realFetch = globalThis.fetch;
    (globalThis as { fetch: typeof fetch }).fetch = fetchMock as unknown as typeof fetch;
    try {
      const first = await client.lookup(1, UNKNOWN_ADDR);
      expect(first).toBeNull();
      expect(fetchMock).toHaveBeenCalledTimes(1);

      // Second call: fetch must NOT be hit — negative cache short-circuit.
      const second = await client.lookup(1, UNKNOWN_ADDR);
      expect(second).toBeNull();
      expect(fetchMock).toHaveBeenCalledTimes(1);

      // Reason is no_publisher (5 min TTL per spec).
      const cached = __peekTokenNegativeCacheForTest(1, UNKNOWN_ADDR);
      expect(cached?.reason).toBe("no_publisher");
    } finally {
      (globalThis as { fetch: typeof fetch }).fetch = realFetch;
    }
  });

  it("cache hit: second lookup serves from in-process cache without fetch", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse(USDC_PAYLOAD));

    const client = createTokenRegistryClient({ fetchImpl: fetchMock });
    const first = await client.lookup(1, USDC_MAINNET);
    const second = await client.lookup(1, USDC_MAINNET);

    expect(first).toEqual(USDC_PAYLOAD);
    expect(second).toEqual(USDC_PAYLOAD);
    // Critical: only one network round-trip even after two lookups.
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("inflight dedupe: concurrent same-key lookups share one fetch", async () => {
    let resolveFetch!: (r: Response) => void;
    fetchMock.mockImplementationOnce(
      () =>
        new Promise<Response>((res) => {
          resolveFetch = res;
        }),
    );

    const client = createTokenRegistryClient({ fetchImpl: fetchMock });
    const p1 = client.lookup(1, WETH_MAINNET);
    const p2 = client.lookup(1, WETH_MAINNET);

    // Yield so both lookups pass the `await hydrate()` boundary and reach
    // doFetch — the mock impl runs synchronously inside the inflight slot,
    // populating `resolveFetch` before we resolve it.
    await Promise.resolve();
    await Promise.resolve();
    resolveFetch(jsonResponse(WETH_PAYLOAD));
    const [r1, r2] = await Promise.all([p1, p2]);

    // Both callers receive the same metadata object (reference equality is
    // a documented dedupe contract: one Promise → one resolved value).
    expect(r1).toBe(r2);
    expect(r1).toEqual(WETH_PAYLOAD);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("schema invalid → null + integrity_failed (5 min)", async () => {
    fetchMock.mockResolvedValueOnce(
      jsonResponse({
        // Missing `decimals` (required field), wrong `kind`.
        kind: "not-erc20",
        chainId: 1,
        address: WETH_MAINNET,
        symbol: "WETH",
        name: "Wrapped Ether",
      }),
    );

    const realFetch = globalThis.fetch;
    (globalThis as { fetch: typeof fetch }).fetch = fetchMock as unknown as typeof fetch;
    try {
      const client = defaultTokenRegistryClient();
      const got = await client.lookup(1, WETH_MAINNET);
      expect(got).toBeNull();

      const cached = __peekTokenNegativeCacheForTest(1, WETH_MAINNET);
      expect(cached?.reason).toBe("integrity_failed");
    } finally {
      (globalThis as { fetch: typeof fetch }).fetch = realFetch;
    }
  });

  it("address case-insensitive: mixed-case input hits the same cache slot", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse(WETH_PAYLOAD));

    const client = createTokenRegistryClient({ fetchImpl: fetchMock });
    // Mixed case input — EIP-55 checksum form. Must lowercase internally.
    const mixed = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
    const first = await client.lookup(1, mixed);
    const second = await client.lookup(1, WETH_MAINNET); // already lower

    expect(first).toEqual(WETH_PAYLOAD);
    expect(second).toEqual(WETH_PAYLOAD);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    // URL must always be the lowercased form so the static host serves
    // the right file (file system case sensitivity).
    const calledUrl = String(fetchMock.mock.calls[0][0]);
    expect(calledUrl).toContain(`/${WETH_MAINNET}.json`);
    expect(calledUrl).not.toContain("C02aaA");
  });

  it("network error → null + timeout (30 s self-healing)", async () => {
    fetchMock.mockRejectedValueOnce(new TypeError("Failed to fetch"));

    const realFetch = globalThis.fetch;
    (globalThis as { fetch: typeof fetch }).fetch = fetchMock as unknown as typeof fetch;
    try {
      const client = defaultTokenRegistryClient();
      const got = await client.lookup(1, WETH_MAINNET);
      expect(got).toBeNull();

      const cached = __peekTokenNegativeCacheForTest(1, WETH_MAINNET);
      expect(cached?.reason).toBe("timeout");
    } finally {
      (globalThis as { fetch: typeof fetch }).fetch = realFetch;
    }
  });
});
