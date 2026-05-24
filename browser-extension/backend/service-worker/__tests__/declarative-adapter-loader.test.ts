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
  getURL: vi.fn((p: string) => `chrome-extension://scopeball/${p}`),
}));

vi.mock("webextension-polyfill", () => ({
  default: { runtime: { getURL: mocks.getURL } },
}));

vi.mock("../wasm-bridge", () => ({
  installDeclarativeBundle: mocks.installDeclarativeBundle,
}));

import {
  __resetSeedBundlesForTest,
  DeclarativeAdapterLoadError,
  ensureSeedBundlesInstalled,
  mountDeclarativeBundle,
} from "../marketplace/declarative-adapter-loader";

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
