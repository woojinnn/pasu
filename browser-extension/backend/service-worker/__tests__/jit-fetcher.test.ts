/**
 * Phase 2B — JIT fetcher cases.
 *
 * End-to-end of `resolveAdapter`:
 *   - Layer 1 hit (already mounted)             → kind: "adapter", source: "layer1"
 *   - Registry 200 OK + sha match               → kind: "adapter", source: "jit"
 *   - Registry 404                              → verdict no_adapter no_publisher (5 min)
 *   - Hash mismatch                             → verdict no_adapter integrity_failed (5 min)
 *   - Registry timeout / network                → verdict no_adapter timeout (30 s)
 *   - inflight dedupe — concurrent same-key calls share one Promise
 *
 * The WASM bridge is mocked; the canonical hash + RFC 8785 path uses the
 * real `canonicalize` package and real SubtleCrypto via happy-dom.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";

type AdapterCacheEntryLike = {
  bundle: unknown;
  bundle_sha256: string;
  fetchedAtMs: number;
};

const mocks = vi.hoisted(() => ({
  installDeclarativeBundle: vi.fn(),
  getURL: vi.fn((p: string) => `chrome-extension://scopeball/${p}`),
  adapterCacheGet: vi
    .fn<() => Promise<AdapterCacheEntryLike | null>>()
    .mockResolvedValue(null),
  adapterCachePut: vi.fn<() => Promise<void>>().mockResolvedValue(undefined),
  adapterCacheDelete: vi.fn<() => Promise<void>>().mockResolvedValue(undefined),
}));

vi.mock("webextension-polyfill", () => ({
  default: { runtime: { getURL: mocks.getURL } },
}));

vi.mock("../wasm-bridge", () => ({
  installDeclarativeBundle: mocks.installDeclarativeBundle,
}));

vi.mock("../marketplace/adapter-cache", () => ({
  adapterCache: {
    get: mocks.adapterCacheGet,
    put: mocks.adapterCachePut,
    delete: mocks.adapterCacheDelete,
  },
}));

import {
  __resetJitFetcherForTest,
  prefetchChildAdapters,
  resolveAdapter,
} from "../marketplace/jit-fetcher";
import {
  __resetNegativeCacheForTest,
  negativeCache,
} from "../marketplace/negative-cache";
import {
  __resetSeedBundlesForTest,
  mountDeclarativeBundle,
} from "../marketplace/declarative-adapter-loader";
import type { CallMatchKey } from "../registry/client";

const FIXTURE_PATH = path.resolve(
  __dirname,
  "../../../../crates/adapters/mappers/tests/fixtures/uniswap-v2-swap-exact-tokens.json",
);

const EXPECTED_SHA256 =
  "0xbb7d55d04f0dd7eda5f122a096d96a3f3c54e564b43c8fba3a359f303375bf93";

// match.chain_ids[0] × match.to[0] of the fixture, lowercased on selector
// to mirror the registry callkey convention.
const FIXTURE_KEY: CallMatchKey = {
  chain_id: 1,
  to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D",
  selector: "0x38ed1739",
};

// Distinct key — used for "no Layer-1 hit" cases.
const UNRELATED_KEY: CallMatchKey = {
  chain_id: 1,
  to: "0x0000000000000000000000000000000000000099",
  selector: "0xdeadbeef",
};

function loadFixtureBundle() {
  return JSON.parse(readFileSync(FIXTURE_PATH, "utf8"));
}

function indexResponse() {
  return {
    matched: true as const,
    bundle_id: "uniswap/v2/swapExactTokensForTokens@1.0.0",
    manifest_path: "manifests/uniswap/v2/swapExactTokensForTokens@1.0.0.json",
    bundle_sha256: EXPECTED_SHA256,
    bundle: loadFixtureBundle(),
  };
}

describe("resolveAdapter", () => {
  // Typed mock so it satisfies `fetchImpl?: typeof fetch` without casts.
  let fetchMock: ReturnType<typeof vi.fn<typeof fetch>>;

  beforeEach(() => {
    vi.clearAllMocks();
    __resetJitFetcherForTest();
    __resetNegativeCacheForTest();
    __resetSeedBundlesForTest();
    fetchMock = vi.fn<typeof fetch>();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("Layer 1 hit: returns a pre-mounted adapter without touching the registry", async () => {
    mocks.installDeclarativeBundle.mockResolvedValueOnce({
      decoder_id: "declarative.uniswap/v2/swapExactTokensForTokens",
      bundle_id: "uniswap/v2/swapExactTokensForTokens@1.0.0",
    });
    // Mount the bundle via the loader so the callkey lookup is populated.
    await mountDeclarativeBundle(readFileSync(FIXTURE_PATH, "utf8"));

    const result = await resolveAdapter(FIXTURE_KEY, {
      registry: { fetchImpl: fetchMock },
    });

    expect(result.kind).toBe("adapter");
    if (result.kind === "adapter") {
      expect(result.source).toBe("layer1");
      expect(result.adapter.bundleId).toBe(
        "uniswap/v2/swapExactTokensForTokens@1.0.0",
      );
    }
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("JIT happy path: registry 200 OK + sha match → mount + adapter", async () => {
    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify(indexResponse()), { status: 200 }),
    );
    mocks.installDeclarativeBundle.mockResolvedValueOnce({
      decoder_id: "declarative.uniswap/v2/swapExactTokensForTokens",
      bundle_id: "uniswap/v2/swapExactTokensForTokens@1.0.0",
    });

    const result = await resolveAdapter(FIXTURE_KEY, {
      registry: { fetchImpl: fetchMock },
    });

    expect(result.kind).toBe("adapter");
    if (result.kind === "adapter") {
      expect(result.source).toBe("jit");
    }
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(mocks.installDeclarativeBundle).toHaveBeenCalledTimes(1);
  });

  it("404 → verdict no_adapter / no_publisher + negative-cache 5min", async () => {
    fetchMock.mockResolvedValueOnce(new Response("nope", { status: 404 }));

    const result = await resolveAdapter(UNRELATED_KEY, {
      registry: { fetchImpl: fetchMock },
    });

    expect(result).toEqual({
      kind: "verdict",
      verdict: "no_adapter",
      reason: "no_publisher",
    });
    const cached = negativeCache.get(UNRELATED_KEY);
    expect(cached?.reason).toBe("no_publisher");
  });

  it("hash mismatch → verdict no_adapter / integrity_failed + 5min cache", async () => {
    const payload = indexResponse();
    payload.bundle_sha256 = "0x" + "00".repeat(32); // poison the hash
    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify(payload), { status: 200 }),
    );

    const result = await resolveAdapter(FIXTURE_KEY, {
      registry: { fetchImpl: fetchMock },
    });

    expect(result).toEqual({
      kind: "verdict",
      verdict: "no_adapter",
      reason: "integrity_failed",
    });
    const cached = negativeCache.get(FIXTURE_KEY);
    expect(cached?.reason).toBe("integrity_failed");
    // WASM must NEVER see a hash-mismatched bundle.
    expect(mocks.installDeclarativeBundle).not.toHaveBeenCalled();
  });

  it("registry timeout → verdict no_adapter / timeout + 30s cache", async () => {
    // Mock fetch that respects the AbortSignal so the client's
    // timeout machinery actually fires.
    fetchMock.mockImplementationOnce((_input, init) => {
      return new Promise<Response>((_resolve, reject) => {
        init?.signal?.addEventListener("abort", () => {
          const err = new Error("aborted");
          err.name = "AbortError";
          reject(err);
        });
      });
    });

    const result = await resolveAdapter(FIXTURE_KEY, {
      registry: { fetchImpl: fetchMock, timeoutMs: 5 },
    });

    expect(result).toEqual({
      kind: "verdict",
      verdict: "no_adapter",
      reason: "timeout",
    });
    const cached = negativeCache.get(FIXTURE_KEY);
    expect(cached?.reason).toBe("timeout");
  });

  it("registry network error → verdict no_adapter / timeout (30s)", async () => {
    fetchMock.mockRejectedValueOnce(new TypeError("Failed to fetch"));

    const result = await resolveAdapter(FIXTURE_KEY, {
      registry: { fetchImpl: fetchMock },
    });

    expect(result).toEqual({
      kind: "verdict",
      verdict: "no_adapter",
      reason: "timeout",
    });
  });

  it("negative cache short-circuits before the registry on the next call", async () => {
    fetchMock.mockResolvedValueOnce(new Response("nope", { status: 404 }));

    const first = await resolveAdapter(UNRELATED_KEY, {
      registry: { fetchImpl: fetchMock },
    });
    expect(first.kind).toBe("verdict");

    // Second call must not hit fetch — pure cache return.
    const second = await resolveAdapter(UNRELATED_KEY, {
      registry: { fetchImpl: fetchMock },
    });
    expect(second).toEqual({
      kind: "verdict",
      verdict: "no_adapter",
      reason: "no_publisher",
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("inflight dedupe: concurrent resolveAdapter calls share one fetch/install", async () => {
    let resolveFetch!: (r: Response) => void;
    fetchMock.mockImplementationOnce(
      () =>
        new Promise<Response>((res) => {
          resolveFetch = res;
        }),
    );
    mocks.installDeclarativeBundle.mockResolvedValueOnce({
      decoder_id: "declarative.uniswap/v2/swapExactTokensForTokens",
      bundle_id: "uniswap/v2/swapExactTokensForTokens@1.0.0",
    });

    const p1 = resolveAdapter(FIXTURE_KEY, {
      registry: { fetchImpl: fetchMock },
    });
    const p2 = resolveAdapter(FIXTURE_KEY, {
      registry: { fetchImpl: fetchMock },
    });

    // Layer 2 cache.get() is async — flush the resolved Promise microtasks
    // so both calls advance past the cache check and p1 registers the
    // inflight entry + calls fetchMock before we invoke resolveFetch.
    await Promise.resolve();
    await Promise.resolve();

    resolveFetch(
      new Response(JSON.stringify(indexResponse()), { status: 200 }),
    );

    const [r1, r2] = await Promise.all([p1, p2]);
    // Dedupe contract: both callers receive *equivalent* adapter results
    // and the registry+WASM pipeline ran exactly once. (The outer
    // resolveAdapter Promises are distinct — `async` wraps each call —
    // but the underlying doJitFetch Promise is shared, so the resolved
    // values are reference-equal.)
    expect(r1).toBe(r2);
    if (r1.kind === "adapter") {
      expect(r1.source).toBe("jit");
    }
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(mocks.installDeclarativeBundle).toHaveBeenCalledTimes(1);
  });

  it("inflight slot is released after settlement (next call re-fetches)", async () => {
    fetchMock.mockResolvedValue(new Response("nope", { status: 404 }));

    const first = await resolveAdapter(UNRELATED_KEY, {
      registry: { fetchImpl: fetchMock },
    });
    expect(first.kind).toBe("verdict");

    // After the prior call settled, the inflight slot must be empty so a
    // *new* (post negative-cache-clear) call would re-fetch.
    __resetNegativeCacheForTest();
    await resolveAdapter(UNRELATED_KEY, {
      registry: { fetchImpl: fetchMock },
    });
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  // --- Layer 2 (adapter-cache) integration tests ---
  // Note: TTL expiry is internal to adapter-cache.ts and already covered in
  // adapter-cache.test.ts. At the resolveAdapter level an expired entry means
  // adapterCacheGet → null → Layer 3, which the existing cache-miss tests cover.

  it("Layer 2: JIT fetch writes to adapter-cache on success", async () => {
    const { bundle, bundle_sha256: bundleSha256 } = indexResponse();
    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify(indexResponse()), { status: 200 }),
    );
    mocks.installDeclarativeBundle.mockResolvedValueOnce({
      decoder_id: "declarative.uniswap/v2/swapExactTokensForTokens",
      bundle_id: "uniswap/v2/swapExactTokensForTokens@1.0.0",
    });

    const result = await resolveAdapter(FIXTURE_KEY, {
      registry: { fetchImpl: fetchMock },
    });

    expect(result.kind).toBe("adapter");
    if (result.kind === "adapter") {
      expect(result.source).toBe("jit");
    }
    // doJitFetch must have called adapterCache.put once with the fetched bundle + sha256.
    expect(mocks.adapterCachePut).toHaveBeenCalledTimes(1);
    expect(mocks.adapterCachePut).toHaveBeenCalledWith(
      FIXTURE_KEY,
      bundle,
      bundleSha256,
    );
  });

  it("Layer 2 hit: returns adapter from cache without touching registry", async () => {
    const { bundle, bundle_sha256: bundleSha256 } = indexResponse();
    // Seed the Layer 2 cache with a valid entry (matching bundle + sha256).
    mocks.adapterCacheGet.mockResolvedValueOnce({
      bundle,
      bundle_sha256: bundleSha256,
      fetchedAtMs: Date.now(),
    });
    mocks.installDeclarativeBundle.mockResolvedValueOnce({
      decoder_id: "declarative.uniswap/v2/swapExactTokensForTokens",
      bundle_id: "uniswap/v2/swapExactTokensForTokens@1.0.0",
    });

    const result = await resolveAdapter(FIXTURE_KEY, {
      registry: { fetchImpl: fetchMock },
    });

    expect(result.kind).toBe("adapter");
    if (result.kind === "adapter") {
      expect(result.source).toBe("layer2");
    }
    // Layer 3 fetch must NOT have been triggered.
    expect(fetchMock).not.toHaveBeenCalled();
    // A cache hit must NOT re-write the cache entry.
    expect(mocks.adapterCachePut).not.toHaveBeenCalled();
  });

  it("Layer 2 corrupted entry (sha mismatch): deletes entry then falls through to Layer 3", async () => {
    const { bundle } = indexResponse();
    // Corrupt the sha256 so installBundle throws bundle_hash_mismatch.
    const corruptSha256 = "0x" + "cc".repeat(32);
    mocks.adapterCacheGet.mockResolvedValueOnce({
      bundle,
      bundle_sha256: corruptSha256,
      fetchedAtMs: Date.now(),
    });
    // Layer 3 fetch returns a good response.
    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify(indexResponse()), { status: 200 }),
    );
    mocks.installDeclarativeBundle.mockResolvedValue({
      decoder_id: "declarative.uniswap/v2/swapExactTokensForTokens",
      bundle_id: "uniswap/v2/swapExactTokensForTokens@1.0.0",
    });

    const result = await resolveAdapter(FIXTURE_KEY, {
      registry: { fetchImpl: fetchMock },
    });

    // Corrupted entry must be evicted.
    expect(mocks.adapterCacheDelete).toHaveBeenCalledTimes(1);
    expect(mocks.adapterCacheDelete).toHaveBeenCalledWith(FIXTURE_KEY);
    // Fall-through to Layer 3: fetch must have been called.
    expect(fetchMock).toHaveBeenCalledTimes(1);
    // Final result comes from JIT path.
    expect(result.kind).toBe("adapter");
    if (result.kind === "adapter") {
      expect(result.source).toBe("jit");
    }
  });
});

describe("prefetchChildAdapters", () => {
  let fetchMock: ReturnType<typeof vi.fn<typeof fetch>>;

  beforeEach(() => {
    vi.clearAllMocks();
    __resetJitFetcherForTest();
    __resetNegativeCacheForTest();
    __resetSeedBundlesForTest();
    fetchMock = vi.fn<typeof fetch>();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("empty child list is a no-op (no registry fetch)", async () => {
    await expect(prefetchChildAdapters([])).resolves.toBeUndefined();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("de-dupes identical callkeys to one fetch", async () => {
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(indexResponse()), { status: 200 }),
    );
    mocks.installDeclarativeBundle.mockResolvedValue({
      decoder_id: "declarative.uniswap/v2/swapExactTokensForTokens",
      bundle_id: "uniswap/v2/swapExactTokensForTokens@1.0.0",
    });

    await prefetchChildAdapters([FIXTURE_KEY, { ...FIXTURE_KEY }], {
      registry: { fetchImpl: fetchMock },
    });

    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("fans out to every distinct child callkey", async () => {
    fetchMock.mockResolvedValue(
      new Response(JSON.stringify(indexResponse()), { status: 200 }),
    );
    mocks.installDeclarativeBundle.mockResolvedValue({
      decoder_id: "declarative.uniswap/v2/swapExactTokensForTokens",
      bundle_id: "uniswap/v2/swapExactTokensForTokens@1.0.0",
    });

    await prefetchChildAdapters([FIXTURE_KEY, UNRELATED_KEY], {
      registry: { fetchImpl: fetchMock },
    });

    // Two distinct child callkeys → two registry fetches. (Both resolve to
    // the same fixture bundle here, and `mountDeclarativeBundle` de-dups the
    // WASM install by bundle id, so the fetch count is the fan-out signal.)
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("a child that 404s is swallowed — never throws, others still fetch", async () => {
    // FIXTURE_KEY child → 200, UNRELATED_KEY child → 404. Keyed on the
    // callkey URL so the response is deterministic under concurrency.
    fetchMock.mockImplementation((input) => {
      const url = typeof input === "string" ? input : (input as Request).url;
      if (url.includes(FIXTURE_KEY.selector)) {
        return Promise.resolve(
          new Response(JSON.stringify(indexResponse()), { status: 200 }),
        );
      }
      return Promise.resolve(new Response("nope", { status: 404 }));
    });
    mocks.installDeclarativeBundle.mockResolvedValue({
      decoder_id: "declarative.uniswap/v2/swapExactTokensForTokens",
      bundle_id: "uniswap/v2/swapExactTokensForTokens@1.0.0",
    });

    await expect(
      prefetchChildAdapters([FIXTURE_KEY, UNRELATED_KEY], {
        registry: { fetchImpl: fetchMock },
      }),
    ).resolves.toBeUndefined();
    // Both callkeys were attempted — the 404 did not abort the other.
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });
});
