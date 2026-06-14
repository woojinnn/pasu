/**
 * declarative-adapter-loader v3 cases.
 *
 * The WASM bridge is mocked the same way as `wasm-bridge.test.ts`: we stub
 * `declarativeInstallV3` to capture the raw JSON the loader forwards. This
 * isolates the loader from real WASM init while still exercising the
 * fetch → parse → install → result-mapping path end-to-end.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();
  return {
    declarativeInstallV3: vi.fn(),
    // Default: signature gate is a no-op pass, so every existing test exercises
    // the unsigned path unchanged. Fail-shape tests override per-call.
    verifyBundleSignature: vi.fn(
      async (): Promise<{ ok: boolean; reason?: string }> => ({ ok: true }),
    ),
    getURL: vi.fn((p: string) => `chrome-extension://dambi/${p}`),
    localStore,
    storageLocal: {
      get: vi.fn(async (key: string) => ({ [key]: localStore.get(key) })),
      set: vi.fn(async (entries: Record<string, unknown>) => {
        for (const [k, v] of Object.entries(entries)) localStore.set(k, v);
      }),
    },
  };
});

vi.mock("webextension-polyfill", () => ({
  default: {
    runtime: { getURL: mocks.getURL },
    storage: { local: mocks.storageLocal },
  },
}));

vi.mock("../wasm-bridge", () => ({
  declarativeInstallV3: mocks.declarativeInstallV3,
}));

vi.mock("../adapter-loader/bundle-verify", () => ({
  readBundleSigDefines: () => ({ require: false, pinnedKeySpkiB64: "" }),
  verifyBundleSignature: mocks.verifyBundleSignature,
}));

import {
  __resetDeclarativeV3CacheForTest,
  installDeclarativeBundleV3,
  installDeclarativeBundleV3ByTypedData,
  installDeclarativeBundleV3BySelector,
  InstallDeclarativeV3Error,
} from "../adapter-loader/declarative-adapter-loader";

describe("installDeclarativeBundleV3", () => {
  const fetchMock = vi.fn();
  const v3Bundle = {
    type: "adapter_action",
    id: "uniswap/v2-router-02/swapExactTokensForETH@1.0.0",
    publisher: "uniswap.eth",
    schema_version: "3",
    match: {
      selector: "0x18cbafe5",
      chain_to_addresses: {
        "1": ["0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"],
        "8453": ["0x4752ba5DBc23f44D87826276BF6Fd6b1C372aD24"],
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

  beforeEach(() => {
    vi.clearAllMocks();
    mocks.localStore.clear();
    __resetDeclarativeV3CacheForTest();
    fetchMock.mockReset();
  });

  it("fetches the callkey, parses the v3 bundle, and installs via WASM", async () => {
    fetchMock.mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          matched: true,
          bundle_id: v3Bundle.id,
          manifest_path: "manifests/x",
          bundle_sha256: "0x" + "a".repeat(64),
          bundle: v3Bundle,
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );
    mocks.declarativeInstallV3.mockResolvedValueOnce({
      decoder_id: v3Bundle.id,
      bundle_id: v3Bundle.id,
    });

    const result = await installDeclarativeBundleV3({
      chainId: 1,
      to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D",
      selector: "0x18cbafe5",
      baseUrl: "https://example.invalid",
      fetchImpl: fetchMock as unknown as typeof fetch,
    });

    expect(result).not.toBeNull();
    expect(result!.decoderId).toBe(v3Bundle.id);
    expect(result!.bundleId).toBe(v3Bundle.id);
    expect(result!.bundle.schema_version).toBe("3");
    expect(fetchMock).toHaveBeenCalledWith(
      "https://example.invalid/index/by-callkey/1__0x7a250d5630b4cf539739df2c5dacb4c659f2488d__0x18cbafe5.json",
    );
    // The bundle text handed to WASM mirrors what the registry sent
    // (pass-through invariant for byte-stable hashing downstream).
    expect(mocks.declarativeInstallV3).toHaveBeenCalledTimes(1);
    expect(mocks.declarativeInstallV3).toHaveBeenCalledWith(
      JSON.stringify(v3Bundle),
    );
  });

  it("returns the cached install on the same callkey without re-fetching", async () => {
    fetchMock.mockResolvedValue(
      new Response(
        JSON.stringify({
          matched: true,
          bundle_id: v3Bundle.id,
          manifest_path: "manifests/x",
          bundle_sha256: "0x" + "a".repeat(64),
          bundle: v3Bundle,
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );
    mocks.declarativeInstallV3.mockResolvedValue({
      decoder_id: v3Bundle.id,
      bundle_id: v3Bundle.id,
    });

    const first = await installDeclarativeBundleV3({
      chainId: 1,
      to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D",
      selector: "0x18cbafe5",
      baseUrl: "https://example.invalid",
      fetchImpl: fetchMock as unknown as typeof fetch,
    });
    const second = await installDeclarativeBundleV3({
      chainId: 1,
      to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D",
      selector: "0x18cbafe5",
      baseUrl: "https://example.invalid",
      fetchImpl: fetchMock as unknown as typeof fetch,
    });

    expect(first!.bundleId).toBe(second!.bundleId);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(mocks.declarativeInstallV3).toHaveBeenCalledTimes(1);
  });

  it("returns null for a 404 miss without throwing or installing", async () => {
    fetchMock.mockResolvedValueOnce(
      new Response("not found", { status: 404 }),
    );

    const result = await installDeclarativeBundleV3({
      chainId: 1,
      to: "0x0000000000000000000000000000000000000001",
      selector: "0xdeadbeef",
      baseUrl: "https://example.invalid",
      fetchImpl: fetchMock as unknown as typeof fetch,
    });

    expect(result).toBeNull();
    expect(mocks.declarativeInstallV3).not.toHaveBeenCalled();
  });

  it("returns null when the response matched=false", async () => {
    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify({ matched: false }), { status: 200 }),
    );

    const result = await installDeclarativeBundleV3({
      chainId: 1,
      to: "0x0000000000000000000000000000000000000001",
      selector: "0xdeadbeef",
      baseUrl: "https://example.invalid",
      fetchImpl: fetchMock as unknown as typeof fetch,
    });
    expect(result).toBeNull();
    expect(mocks.declarativeInstallV3).not.toHaveBeenCalled();
  });

  it("returns null when the registry serves a v2 manifest (silent v3 miss)", async () => {
    fetchMock.mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          matched: true,
          bundle_id: "uniswap/swap-router-02/wrapETH@1.0.0",
          manifest_path: "manifests/x",
          bundle_sha256: "0x" + "a".repeat(64),
          bundle: {
            type: "adapter_function",
            id: "uniswap/swap-router-02/wrapETH@1.0.0",
            publisher: "uniswap.eth",
            schema_version: "2",
            match: {
              selector: "0x1c58db4f",
              chain_to_addresses: {
                "1": ["0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45"],
              },
            },
            abi_fragment: { function_name: "wrapETH", abi: {} },
            emit: {
              strategy: "single_emit",
              category: "misc",
              action: "wrap",
              fields: {},
            },
          },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );

    const result = await installDeclarativeBundleV3({
      chainId: 1,
      to: "0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45",
      selector: "0x1c58db4f",
      baseUrl: "https://example.invalid",
      fetchImpl: fetchMock as unknown as typeof fetch,
    });
    expect(result).toBeNull();
    expect(mocks.declarativeInstallV3).not.toHaveBeenCalled();
  });

  it("throws InstallDeclarativeV3Error when the parsed v3 bundle is malformed", async () => {
    fetchMock.mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          matched: true,
          bundle_id: "x",
          manifest_path: "x",
          bundle_sha256: "0x" + "a".repeat(64),
          bundle: { ...v3Bundle, type: "adapter_function" },
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );

    await expect(
      installDeclarativeBundleV3({
        chainId: 1,
        to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D",
        selector: "0x18cbafe5",
        baseUrl: "https://example.invalid",
        fetchImpl: fetchMock as unknown as typeof fetch,
      }),
    ).rejects.toBeInstanceOf(InstallDeclarativeV3Error);
    expect(mocks.declarativeInstallV3).not.toHaveBeenCalled();
  });

  it("wraps WASM install rejections in InstallDeclarativeV3Error", async () => {
    fetchMock.mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          matched: true,
          bundle_id: v3Bundle.id,
          manifest_path: "x",
          bundle_sha256: "0x" + "a".repeat(64),
          bundle: v3Bundle,
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );
    mocks.declarativeInstallV3.mockRejectedValueOnce(new Error("engine boom"));

    await expect(
      installDeclarativeBundleV3({
        chainId: 1,
        to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D",
        selector: "0x18cbafe5",
        baseUrl: "https://example.invalid",
        fetchImpl: fetchMock as unknown as typeof fetch,
      }),
    ).rejects.toBeInstanceOf(InstallDeclarativeV3Error);
  });

  // ---------------------------------------------------------------------------
  // Layer 2 — chrome.storage.local mirror (plan §M3 SW restart 영속화)
  // ---------------------------------------------------------------------------

  it("persists ONLY the requested callkey — no chain_to_addresses fan-out (8s install fix)", async () => {
    fetchMock.mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          matched: true,
          bundle_id: v3Bundle.id,
          manifest_path: "manifests/x",
          bundle_sha256: "0x" + "a".repeat(64),
          bundle: v3Bundle,
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );
    mocks.declarativeInstallV3.mockResolvedValueOnce({
      decoder_id: v3Bundle.id,
      bundle_id: v3Bundle.id,
    });

    await installDeclarativeBundleV3({
      chainId: 1,
      to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D",
      selector: "0x18cbafe5",
      baseUrl: "https://example.invalid",
      fetchImpl: fetchMock as unknown as typeof fetch,
    });

    const stored = mocks.localStore.get("registry:adapter-bundles-v3") as
      | {
          schemaVersion: number;
          bundles: Record<string, unknown>;
          callkeys: Record<
            string,
            { bundleId: string; decoderId: string; bundleSha256: string; fetchedAtMs: number }
          >;
        }
      | undefined;
    expect(stored).toBeTruthy();
    expect(stored!.schemaVersion).toBe(2);
    expect(Object.keys(stored!.bundles)).toEqual([v3Bundle.id]);
    const keys = Object.keys(stored!.callkeys);
    // Regression guard for the `__engine::timeout` 8s overrun: installing a
    // large-match bundle used to persist ONE chrome.storage.local record per
    // chain_to_addresses pair, and every `put` re-serializes the whole record
    // (O(N²) writes — ~7.7s for the ~3891-address standard/erc20/approve
    // bundle, blowing HARD_TIMEOUT_MS=8000). We now persist ONLY the requested
    // callkey; the sibling (chain 8453) is NOT mirrored to disk — it re-fetches
    // its own tiny by-callkey file on demand after a cold SW restart. In-session
    // sibling routing is unaffected (one WASM install bridges every address in
    // the served match + the in-memory mirror covers it). Do NOT reintroduce
    // the per-address persist fan-out.
    expect(keys).toEqual([
      "v3:1__0x7a250d5630b4cf539739df2c5dacb4c659f2488d__0x18cbafe5",
    ]);
    expect(keys).not.toContain(
      "v3:8453__0x4752ba5dbc23f44d87826276bf6fd6b1c372ad24__0x18cbafe5",
    );
    expect(stored!.callkeys[keys[0]].bundleId).toBe(v3Bundle.id);
    expect(stored!.callkeys[keys[0]].bundleSha256).toBe("0x" + "a".repeat(64));
  });

  it("rehydrates from chrome.storage.local on a cold SW (storage-hit path)", async () => {
    // 직전 lifetime 의 storage 를 흉내냄 — schema-v2 seed (dedup된 bundle store + callkey index).
    const seedKey =
      "v3:1__0x7a250d5630b4cf539739df2c5dacb4c659f2488d__0x18cbafe5";
    const crossKey =
      "v3:8453__0x4752ba5dbc23f44d87826276bf6fd6b1c372ad24__0x18cbafe5";
    const seedMeta = {
      bundleId: v3Bundle.id,
      decoderId: v3Bundle.id,
      bundleSha256: "0x" + "b".repeat(64),
      fetchedAtMs: Date.now() - 60 * 1000,
    };
    mocks.localStore.set("registry:adapter-bundles-v3", {
      schemaVersion: 2,
      bundles: { [v3Bundle.id]: v3Bundle },
      callkeys: {
        [seedKey]: seedMeta,
        [crossKey]: seedMeta,
      },
    });

    mocks.declarativeInstallV3.mockResolvedValueOnce({
      decoder_id: v3Bundle.id,
      bundle_id: v3Bundle.id,
    });

    const result = await installDeclarativeBundleV3({
      chainId: 1,
      to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D",
      selector: "0x18cbafe5",
      baseUrl: "https://example.invalid",
      fetchImpl: fetchMock as unknown as typeof fetch,
    });

    expect(result).not.toBeNull();
    expect(result!.bundleId).toBe(v3Bundle.id);
    expect(result!.bundle.schema_version).toBe("3");
    // storage-hit path = registry-api-v3 fetch 없이 WASM 재install 만.
    expect(fetchMock).not.toHaveBeenCalled();
    expect(mocks.declarativeInstallV3).toHaveBeenCalledTimes(1);
    expect(mocks.declarativeInstallV3).toHaveBeenCalledWith(
      JSON.stringify(v3Bundle),
    );
  });
});

describe("installDeclarativeBundleV3BySelector", () => {
  const fetchMock = vi.fn();
  // Address-agnostic bundle: match keyed by (chain, selector) ONLY, no `to`.
  // This is exactly the shape parseMatch must accept (chain_ids present, no
  // chain_to_addresses / to) — a v1-legacy parse would reject it on `to`.
  const agnosticBundle = {
    type: "adapter_action",
    id: "standard/nft/set-approval-for-all@1.0.0",
    schema_version: "3",
    match: {
      selector: "0xa22cb465",
      address_agnostic: true,
      chain_ids: [1, 10, 8453, 42161],
    },
    abi_fragment: {
      function_name: "setApprovalForAll",
      abi: { name: "setApprovalForAll", type: "function", inputs: [] },
    },
    emit: {
      strategy: "single_emit",
      body: {
        domain: "token",
        token: { action: "nft_set_approval_for_all" },
      },
    },
  };

  beforeEach(() => {
    vi.clearAllMocks();
    mocks.localStore.clear();
    __resetDeclarativeV3CacheForTest();
    fetchMock.mockReset();
  });

  it("fetches by-selector, parses the address-agnostic bundle, installs via WASM", async () => {
    fetchMock.mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          matched: true,
          bundle_id: agnosticBundle.id,
          bundle: agnosticBundle,
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );
    mocks.declarativeInstallV3.mockResolvedValueOnce({
      decoder_id: agnosticBundle.id,
      bundle_id: agnosticBundle.id,
    });

    const result = await installDeclarativeBundleV3BySelector({
      chainId: 1,
      selector: "0xa22cb465",
      baseUrl: "https://example.invalid",
      fetchImpl: fetchMock as unknown as typeof fetch,
    });

    expect(result).not.toBeNull();
    expect(result!.bundleId).toBe(agnosticBundle.id);
    expect(fetchMock).toHaveBeenCalledWith(
      "https://example.invalid/index/by-selector/1__0xa22cb465.json",
    );
    expect(mocks.declarativeInstallV3).toHaveBeenCalledWith(
      JSON.stringify(agnosticBundle),
    );
  });

  it("returns null on a 404 by-selector miss without installing", async () => {
    fetchMock.mockResolvedValueOnce(new Response("", { status: 404 }));
    const result = await installDeclarativeBundleV3BySelector({
      chainId: 1,
      selector: "0xa22cb465",
      baseUrl: "https://example.invalid",
      fetchImpl: fetchMock as unknown as typeof fetch,
    });
    expect(result).toBeNull();
    expect(mocks.declarativeInstallV3).not.toHaveBeenCalled();
  });
});

describe("bundle signature gate — per-site fail shape", () => {
  const fetchMock = vi.fn();
  const okBundle = {
    type: "adapter_action",
    id: "sig-gate/test@1.0.0",
    schema_version: "3",
    match: {
      selector: "0x18cbafe5",
      chain_to_addresses: {
        "1": ["0x7a250d5630b4cf539739df2c5dacb4c659f2488d"],
      },
    },
    abi_fragment: {
      function_name: "f",
      abi: { name: "f", type: "function", inputs: [] },
    },
    emit: {
      strategy: "single_emit",
      body: {
        domain: "amm",
        amm: { action: "swap", swap: { venue: { name: "uniswap_v2" } } },
      },
    },
  };
  const resp = (bundle: unknown) =>
    new Response(
      JSON.stringify({
        matched: true,
        bundle_id: okBundle.id,
        manifest_path: "m",
        bundle_sha256: "0x" + "a".repeat(64),
        bundle,
      }),
      { status: 200, headers: { "content-type": "application/json" } },
    );
  const ROUTER = "0x7a250d5630b4cf539739df2c5dacb4c659f2488d";

  beforeEach(() => {
    vi.clearAllMocks();
    mocks.localStore.clear();
    __resetDeclarativeV3CacheForTest();
    fetchMock.mockReset();
    mocks.verifyBundleSignature.mockResolvedValue({ ok: true });
    mocks.declarativeInstallV3.mockResolvedValue({
      decoder_id: okBundle.id,
      bundle_id: okBundle.id,
    });
  });

  it("callkey: verify fail → throws InstallDeclarativeV3Error(stage=verify), no install", async () => {
    fetchMock.mockResolvedValueOnce(resp(okBundle));
    mocks.verifyBundleSignature.mockResolvedValueOnce({
      ok: false,
      reason: "sig_invalid",
    });
    await expect(
      installDeclarativeBundleV3({
        chainId: 1,
        to: ROUTER,
        selector: "0x18cbafe5",
        baseUrl: "https://example.invalid",
        fetchImpl: fetchMock as unknown as typeof fetch,
      }),
    ).rejects.toMatchObject({ stage: "verify" });
    expect(mocks.declarativeInstallV3).not.toHaveBeenCalled();
  });

  it("typed-data: verify fail → { ok:false, reason:'verify_failed' }, no install", async () => {
    fetchMock.mockResolvedValueOnce(resp(okBundle));
    mocks.verifyBundleSignature.mockResolvedValueOnce({
      ok: false,
      reason: "sig_invalid",
    });
    const r = await installDeclarativeBundleV3ByTypedData(
      {
        chainId: 1,
        verifyingContract: "0x000000000022d473030f116ddee9f6b43ac78ba3",
        primaryType: "PermitSingle",
      },
      {
        baseUrl: "https://example.invalid",
        fetchImpl: fetchMock as unknown as typeof fetch,
      },
    );
    expect(r).toEqual({ ok: false, reason: "verify_failed" });
    expect(mocks.declarativeInstallV3).not.toHaveBeenCalled();
  });

  it("by-selector: verify fail → null, no install", async () => {
    fetchMock.mockResolvedValueOnce(resp(okBundle));
    mocks.verifyBundleSignature.mockResolvedValueOnce({
      ok: false,
      reason: "sig_invalid",
    });
    const r = await installDeclarativeBundleV3BySelector({
      chainId: 1,
      selector: "0x18cbafe5",
      baseUrl: "https://example.invalid",
      fetchImpl: fetchMock as unknown as typeof fetch,
    });
    expect(r).toBeNull();
    expect(mocks.declarativeInstallV3).not.toHaveBeenCalled();
  });

  it("verify pass (default) → install proceeds (control)", async () => {
    fetchMock.mockResolvedValueOnce(resp(okBundle));
    const r = await installDeclarativeBundleV3({
      chainId: 1,
      to: ROUTER,
      selector: "0x18cbafe5",
      baseUrl: "https://example.invalid",
      fetchImpl: fetchMock as unknown as typeof fetch,
    });
    expect(r).not.toBeNull();
    expect(mocks.declarativeInstallV3).toHaveBeenCalledTimes(1);
  });
});
