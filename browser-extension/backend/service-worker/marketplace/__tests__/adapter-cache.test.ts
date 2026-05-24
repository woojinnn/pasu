/**
 * Layer 2 persistent adapter cache tests.
 *
 * Coverage matrix:
 *   1. put → get: returns stored entry
 *   2. TTL expiry: entry older than 24h → get returns null
 *   3. LRU eviction: 257 distinct keys → first-inserted entry evicted
 *   4. Hydrate: pre-seeded storage → fresh cache instance loads from storage
 *
 * `Browser.storage.local` is mocked using the same `Map`-backed pattern
 * as `registry/__tests__/token-client.test.ts`.
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
  adapterCache,
  __resetAdapterCacheForTest,
  type AdapterCacheEntry,
} from "../adapter-cache";
import type { AdapterFunctionBundle } from "../bundle-schema";
import type { CallMatchKey } from "../../registry/client";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const BUNDLE_V2: AdapterFunctionBundle = {
  type: "adapter_function",
  id: "uniswap/v2/swapExactTokensForTokens@1.0.0",
  publisher: "uniswap.eth",
  match: {
    chain_ids: [1],
    to: ["0x7a250d5630b4cf539739df2c5dacb4c659f2488d"],
    selector: "0x38ed1739",
  },
  abi_fragment: {
    function_name: "swapExactTokensForTokens",
    abi: { name: "swapExactTokensForTokens", type: "function", inputs: [] },
  },
  emit: {
    strategy: "single_emit",
    category: "dex",
    action: "swap",
    fields: {
      "inputToken.asset.kind": { literal: "erc20" },
    },
  },
  requires: {
    imperative: [],
    adapter_capabilities: [],
    host_capabilities: [],
    extension: ">=0.1.0",
  },
};

const SHA256_V2 = "0x9d54198599e1ced436bfbb458bf36aae4b3a01ba5a8bd885ab20f07c5a3f02f0";

const KEY_V2: CallMatchKey = {
  chain_id: 1,
  to: "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
  selector: "0x38ed1739",
};

function makeKey(i: number): CallMatchKey {
  // Produce 257 syntactically distinct keys. chain_id varies from 1..257 so
  // the `to` and `selector` can stay constant (they are valid EVM format
  // regardless of chain_id).
  return {
    chain_id: i + 1,
    to: "0x7a250d5630b4cf539739df2c5dacb4c659f2488d",
    selector: "0x38ed1739",
  };
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("adapterCache", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.localStore.clear();
    __resetAdapterCacheForTest();
  });

  it("1. put then get returns the stored entry", async () => {
    await adapterCache.put(KEY_V2, BUNDLE_V2, SHA256_V2);
    const got = await adapterCache.get(KEY_V2);

    expect(got).not.toBeNull();
    expect(got!.bundle).toEqual(BUNDLE_V2);
    expect(got!.bundle_sha256).toBe(SHA256_V2);
    expect(typeof got!.fetchedAtMs).toBe("number");
    // Storage must have been persisted
    expect(mocks.browser.storage.local.set).toHaveBeenCalled();
  });

  it("2. TTL expiry: entry older than 24h returns null", async () => {
    const staleMs = Date.now() - 25 * 60 * 60 * 1000; // 25 hours ago
    const staleEntry: AdapterCacheEntry = {
      bundle: BUNDLE_V2,
      bundle_sha256: SHA256_V2,
      fetchedAtMs: staleMs,
    };
    // Pre-seed storage with a stale entry (simulates a previous SW lifetime).
    const serializedKey = `${KEY_V2.chain_id}__${KEY_V2.to.toLowerCase()}__${KEY_V2.selector.toLowerCase()}`;
    mocks.localStore.set("registry:adapter-bundles", { [serializedKey]: staleEntry });

    // get() should hydrate from storage, detect TTL expiry, and return null.
    const got = await adapterCache.get(KEY_V2);
    expect(got).toBeNull();
    // The stale entry must be evicted from in-memory state — a second get
    // (no hydrate re-run, already hydrated) also returns null.
    const got2 = await adapterCache.get(KEY_V2);
    expect(got2).toBeNull();
  });

  it("3. LRU eviction: 257 puts evicts the first-inserted entry", async () => {
    // Insert 257 entries (cap is 256).
    for (let i = 0; i < 257; i++) {
      await adapterCache.put(makeKey(i), BUNDLE_V2, SHA256_V2);
    }

    // The first-inserted key (i=0, chain_id=1) must have been evicted.
    const evicted = await adapterCache.get(makeKey(0));
    expect(evicted).toBeNull();

    // The most-recently inserted key (i=256, chain_id=257) must still be present.
    const latest = await adapterCache.get(makeKey(256));
    expect(latest).not.toBeNull();
    expect(latest!.bundle_sha256).toBe(SHA256_V2);
  });

  it("4. Hydrate: pre-seeded fresh entry is loaded from storage", async () => {
    const freshMs = Date.now() - 60 * 1000; // 1 minute ago (well within TTL)
    const freshEntry: AdapterCacheEntry = {
      bundle: BUNDLE_V2,
      bundle_sha256: SHA256_V2,
      fetchedAtMs: freshMs,
    };
    const serializedKey = `${KEY_V2.chain_id}__${KEY_V2.to.toLowerCase()}__${KEY_V2.selector.toLowerCase()}`;
    mocks.localStore.set("registry:adapter-bundles", { [serializedKey]: freshEntry });

    // A brand-new call (hydrated = false after reset in beforeEach) should
    // load the entry from chrome.storage.local and return it.
    const got = await adapterCache.get(KEY_V2);
    expect(got).not.toBeNull();
    expect(got!.bundle).toEqual(BUNDLE_V2);
    expect(got!.bundle_sha256).toBe(SHA256_V2);
    expect(got!.fetchedAtMs).toBe(freshMs);
    // Storage.local.get must have been called (hydrate happened)
    expect(mocks.browser.storage.local.get).toHaveBeenCalledWith("registry:adapter-bundles");
  });
});
