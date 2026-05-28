/**
 * Phase 1B — declarative-adapter-loader cases.
 *
 * The WASM bridge is mocked the same way as `wasm-bridge.test.ts`: we
 * stub `installDeclarativeBundle` to capture the raw JSON the loader
 * forwards. This isolates the loader from real WASM init while still
 * exercising the parse → install → result-mapping path end-to-end.
 *
 * For `ensureSeedBundlesInstalled`, `fetch` is stubbed against
 * `Browser.runtime.getURL("seed-bundles/<filename>")` and we read the
 * actual fixture off disk so the seed JSON stays in lockstep with the
 * Rust side.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";

const mocks = vi.hoisted(() => ({
  installDeclarativeBundle: vi.fn(),
  declarativeInstallV3: vi.fn(),
  getURL: vi.fn((p: string) => `chrome-extension://scopeball/${p}`),
}));

vi.mock("webextension-polyfill", () => ({
  default: { runtime: { getURL: mocks.getURL } },
}));

vi.mock("../wasm-bridge", () => ({
  installDeclarativeBundle: mocks.installDeclarativeBundle,
  declarativeInstallV3: mocks.declarativeInstallV3,
}));

import {
  __resetDeclarativeV3CacheForTest,
  __resetSeedBundlesForTest,
  DeclarativeAdapterLoadError,
  ensureSeedBundlesInstalled,
  installDeclarativeBundleV3,
  InstallDeclarativeV3Error,
  mountDeclarativeBundle,
} from "../adapter-loader/declarative-adapter-loader";

const FIXTURE_PATH = path.resolve(
  __dirname,
  "../../../../crates/adapters/mappers/tests/fixtures/uniswap-v2-swap-exact-tokens.json",
);

function loadFixtureText(): string {
  return readFileSync(FIXTURE_PATH, "utf8");
}

describe("mountDeclarativeBundle", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("returns the decoder_id + bundle_id for a valid bundle", async () => {
    mocks.installDeclarativeBundle.mockResolvedValueOnce({
      decoder_id: "declarative.uniswap/v2/swapExactTokensForTokens",
      bundle_id: "uniswap/v2/swapExactTokensForTokens@1.0.0",
    });

    const result = await mountDeclarativeBundle(loadFixtureText());

    expect(result).toMatchObject({
      decoderId: "declarative.uniswap/v2/swapExactTokensForTokens",
      bundleId: "uniswap/v2/swapExactTokensForTokens@1.0.0",
    });
    // Phase 6 — the mount result now also exposes the parsed bundle so
    // downstream consumers (orchestrator declarative path) can decode
    // calldata against `bundle.abi_fragment`.
    expect(result.bundle.id).toBe("uniswap/v2/swapExactTokensForTokens@1.0.0");
    expect(result.bundle.match.selector).toBe("0x38ed1739");
    // Pass-through invariant: the loader must hand the engine the exact
    // bytes it received, not a re-stringified parsed copy.
    expect(mocks.installDeclarativeBundle).toHaveBeenCalledWith(
      loadFixtureText(),
    );
  });

  it("rejects non-JSON input at the parse stage", async () => {
    await expect(mountDeclarativeBundle("{not json")).rejects.toBeInstanceOf(
      DeclarativeAdapterLoadError,
    );
    expect(mocks.installDeclarativeBundle).not.toHaveBeenCalled();
  });

  it("rejects bundles that fail the BNF shape check before reaching WASM", async () => {
    // Invalid selector (not 4 bytes) — parseBundle must catch this so the
    // WASM engine never has to swallow a malformed bundle.
    const badBundle = JSON.stringify({
      type: "adapter_function",
      id: "bad/x@1.0.0",
      publisher: "x.eth",
      match: {
        chain_ids: [1],
        to: ["0x0000000000000000000000000000000000000001"],
        selector: "0xshort",
      },
      abi_fragment: { function_name: "x", abi: {} },
      emit: {
        strategy: "single_emit",
        category: "x",
        action: "y",
        fields: {},
      },
      requires: { imperative: [], host_capabilities: [], extension: ">=0.1.0" },
    });
    await expect(mountDeclarativeBundle(badBundle)).rejects.toBeInstanceOf(
      DeclarativeAdapterLoadError,
    );
    expect(mocks.installDeclarativeBundle).not.toHaveBeenCalled();
  });

  it("wraps WASM engine errors in DeclarativeAdapterLoadError", async () => {
    mocks.installDeclarativeBundle.mockRejectedValueOnce(
      new Error("engine boom"),
    );
    await expect(mountDeclarativeBundle(loadFixtureText())).rejects.toThrow(
      /declarative-adapter-loader\[install\]/,
    );
  });
});

describe("ensureSeedBundlesInstalled", () => {
  const fetchMock = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    __resetSeedBundlesForTest();
    fetchMock.mockReset();
    vi.stubGlobal("fetch", fetchMock);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("fetches every shipped seed bundle and mounts it", async () => {
    fetchMock.mockResolvedValue(
      new Response(loadFixtureText(), { status: 200 }),
    );
    mocks.installDeclarativeBundle.mockResolvedValue({
      decoder_id: "declarative.uniswap/v2/swapExactTokensForTokens",
      bundle_id: "uniswap/v2/swapExactTokensForTokens@1.0.0",
    });

    await ensureSeedBundlesInstalled();

    expect(fetchMock).toHaveBeenCalledWith(
      "chrome-extension://scopeball/seed-bundles/uniswap-v2-swapExactTokensForTokens@1.0.0.json",
    );
    expect(mocks.installDeclarativeBundle).toHaveBeenCalledTimes(1);
    expect(mocks.installDeclarativeBundle).toHaveBeenCalledWith(
      loadFixtureText(),
    );
  });

  it("returns the same in-flight promise on overlapping calls", async () => {
    fetchMock.mockResolvedValue(
      new Response(loadFixtureText(), { status: 200 }),
    );
    mocks.installDeclarativeBundle.mockResolvedValue({
      decoder_id: "declarative.uniswap/v2/swapExactTokensForTokens",
      bundle_id: "uniswap/v2/swapExactTokensForTokens@1.0.0",
    });

    const first = ensureSeedBundlesInstalled();
    const second = ensureSeedBundlesInstalled();
    expect(first).toBe(second);
    await first;

    // Idempotent: a second call after settle must not re-fetch within the
    // same SW lifetime.
    await ensureSeedBundlesInstalled();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(mocks.installDeclarativeBundle).toHaveBeenCalledTimes(1);
  });

  it("logs and swallows a failed seed bundle (does not throw)", async () => {
    fetchMock.mockResolvedValue(
      new Response("not-found", { status: 404 }),
    );
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

    await expect(ensureSeedBundlesInstalled()).resolves.toBeUndefined();
    expect(warnSpy).toHaveBeenCalled();
    expect(mocks.installDeclarativeBundle).not.toHaveBeenCalled();
    warnSpy.mockRestore();
  });
});

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
});
