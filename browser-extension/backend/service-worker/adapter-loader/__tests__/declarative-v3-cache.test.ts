/**
 * Layer 2 v3 persistent cache tests.
 *
 * Coverage matrix mirrors `adapter-cache.test.ts`:
 *   1. get on empty cache → null
 *   2. put then get → returns stored entry
 *   3. TTL expiry: stored 25h ago → null on next get
 *   4. Hydrate: pre-seeded storage → fresh cache instance loads
 *   5. LRU eviction: 257 distinct keys → first-inserted evicted
 *
 * `Browser.storage.local` is mocked with the same `Map`-backed pattern as
 * `adapter-cache.test.ts` so the two layers stay symmetric.
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
  declarativeV3Cache,
  __resetDeclarativeV3CacheStorageForTest,
  type DeclarativeV3CacheEntry,
} from "../declarative-v3-cache";
import type { V3Bundle } from "../bundle-schema";

const V3_BUNDLE: V3Bundle = {
  type: "adapter_action",
  id: "uniswap/v2-router-02/swapExactTokensForETH@1.0.0",
  publisher: "uniswap.eth",
  schema_version: "3",
  match: {
    selector: "0x18cbafe5",
    chain_to_addresses: {
      "1": ["0x7a250d5630b4cf539739df2c5dacb4c659f2488d"],
      "8453": ["0x4752ba5dbc23f44d87826276bf6fd6b1c372ad24"],
    },
  },
  abi_fragment: {
    function_name: "swapExactTokensForETH",
    abi: { name: "swapExactTokensForETH", type: "function", inputs: [] },
  },
  emit: {
    strategy: "single_emit",
    body: {
      domain: "amm",
      amm: { action: "swap", swap: { venue: { name: "uniswap_v2" } } },
    },
  },
};

const SHA256 = "0x" + "a".repeat(64);
const CALLKEY =
  "v3:1__0x7a250d5630b4cf539739df2c5dacb4c659f2488d__0x18cbafe5";

function freshEntry(now: number = Date.now()): DeclarativeV3CacheEntry {
  return {
    bundle: V3_BUNDLE,
    bundleId: V3_BUNDLE.id,
    decoderId: V3_BUNDLE.id,
    bundleSha256: SHA256,
    fetchedAtMs: now,
  };
}

function makeCallkey(i: number): string {
  // chain_id 부분만 1..N 으로 분기 — `to` + selector 는 고정.
  return `v3:${i + 1}__0x7a250d5630b4cf539739df2c5dacb4c659f2488d__0x18cbafe5`;
}

describe("declarativeV3Cache", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.localStore.clear();
    __resetDeclarativeV3CacheStorageForTest();
  });

  it("1. get on empty cache returns null", async () => {
    const got = await declarativeV3Cache.get(CALLKEY);
    expect(got).toBeNull();
    expect(mocks.browser.storage.local.get).toHaveBeenCalledWith(
      "registry:adapter-bundles-v3",
    );
  });

  it("2. put then get returns the stored entry", async () => {
    await declarativeV3Cache.put(CALLKEY, freshEntry());
    const got = await declarativeV3Cache.get(CALLKEY);

    expect(got).not.toBeNull();
    expect(got!.bundle).toEqual(V3_BUNDLE);
    expect(got!.bundleId).toBe(V3_BUNDLE.id);
    expect(got!.decoderId).toBe(V3_BUNDLE.id);
    expect(got!.bundleSha256).toBe(SHA256);
    expect(typeof got!.fetchedAtMs).toBe("number");
    expect(mocks.browser.storage.local.set).toHaveBeenCalled();
  });

  it("3. TTL expiry: entry older than 24h returns null", async () => {
    const staleMs = Date.now() - 25 * 60 * 60 * 1000; // 25h 이전
    mocks.localStore.set("registry:adapter-bundles-v3", {
      schemaVersion: 2,
      bundles: { [V3_BUNDLE.id]: V3_BUNDLE },
      callkeys: {
        [CALLKEY]: {
          bundleId: V3_BUNDLE.id,
          decoderId: V3_BUNDLE.id,
          bundleSha256: SHA256,
          fetchedAtMs: staleMs,
        },
      },
    });

    // hydrate 시점에 TTL filter 가 stale 을 drop.
    const got = await declarativeV3Cache.get(CALLKEY);
    expect(got).toBeNull();
    // 두 번째 호출도 null (hydrate 이미 끝남 + mem 비어있음).
    const got2 = await declarativeV3Cache.get(CALLKEY);
    expect(got2).toBeNull();
  });

  it("4. Hydrate: pre-seeded fresh entry loaded from storage", async () => {
    const freshMs = Date.now() - 60 * 1000; // 1 분 전
    mocks.localStore.set("registry:adapter-bundles-v3", {
      schemaVersion: 2,
      bundles: { [V3_BUNDLE.id]: V3_BUNDLE },
      callkeys: {
        [CALLKEY]: {
          bundleId: V3_BUNDLE.id,
          decoderId: V3_BUNDLE.id,
          bundleSha256: SHA256,
          fetchedAtMs: freshMs,
        },
      },
    });

    const got = await declarativeV3Cache.get(CALLKEY);
    expect(got).not.toBeNull();
    expect(got!.bundleId).toBe(V3_BUNDLE.id);
    expect(got!.bundleSha256).toBe(SHA256);
    expect(got!.fetchedAtMs).toBe(freshMs);
    expect(mocks.browser.storage.local.get).toHaveBeenCalledWith(
      "registry:adapter-bundles-v3",
    );
  });

  it("6. Legacy fat-format storage is dropped on hydrate (migration)", async () => {
    // Pre-v2 storage: bundle copied under every callkey. Must be discarded
    // so the quota-exhausted state self-heals on next SW boot.
    const legacy = {
      [CALLKEY]: {
        bundle: V3_BUNDLE,
        bundleId: V3_BUNDLE.id,
        decoderId: V3_BUNDLE.id,
        bundleSha256: SHA256,
        fetchedAtMs: Date.now(),
      },
    };
    mocks.localStore.set("registry:adapter-bundles-v3", legacy);

    const got = await declarativeV3Cache.get(CALLKEY);
    expect(got).toBeNull();
    // Storage should have been overwritten with the empty v2 record.
    const stored = mocks.localStore.get("registry:adapter-bundles-v3") as
      | { schemaVersion?: number }
      | undefined;
    expect(stored?.schemaVersion).toBe(2);
  });

  it("7. Same bundleId across many callkeys stores ONE bundle copy", async () => {
    // The original bug: 8 callkeys for one UR deployment held 8 full bundle
    // copies (~370 KB for a 46 KB bundle). With normalization, only one
    // copy lives in storage.
    for (let i = 0; i < 8; i++) {
      await declarativeV3Cache.put(makeCallkey(i), freshEntry());
    }
    const stored = mocks.localStore.get("registry:adapter-bundles-v3") as {
      bundles: Record<string, unknown>;
      callkeys: Record<string, unknown>;
    };
    expect(Object.keys(stored.bundles)).toHaveLength(1);
    expect(Object.keys(stored.callkeys)).toHaveLength(8);
  });

  it("5. LRU eviction: 257 puts evicts the first-inserted entry", async () => {
    for (let i = 0; i < 257; i++) {
      await declarativeV3Cache.put(makeCallkey(i), freshEntry());
    }

    // 첫 entry (i=0, chain_id=1) 는 evict.
    const evicted = await declarativeV3Cache.get(makeCallkey(0));
    expect(evicted).toBeNull();

    // 마지막 entry (i=256, chain_id=257) 는 남음.
    const latest = await declarativeV3Cache.get(makeCallkey(256));
    expect(latest).not.toBeNull();
    expect(latest!.bundleSha256).toBe(SHA256);
  });
});
