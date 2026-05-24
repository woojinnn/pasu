/**
 * Phase 2B — installBundle cases.
 *
 * The hash check is exercised against the real Phase 2A bundle fixture
 * — if `canonicalSha256` ever diverges from `build-index.ts` (the
 * registry-side hash producer), this test catches it.
 *
 * Expected hash: 0xbb7d55d04f0dd7eda5f122a096d96a3f3c54e564b43c8fba3a359f303375bf93
 * (matches the value committed at
 * `/Users/jhy/Desktop/ScopeBall/scopeball/registry/index/by-callkey/1__….json`)
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
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
  canonicalSha256,
  InstallError,
  installBundle,
} from "../marketplace/installBundle";
import { __resetSeedBundlesForTest } from "../marketplace/declarative-adapter-loader";

const FIXTURE_PATH = path.resolve(
  __dirname,
  "../../../../crates/adapters/mappers/tests/fixtures/uniswap-v2-swap-exact-tokens.json",
);

const EXPECTED_SHA256 =
  "0xbb7d55d04f0dd7eda5f122a096d96a3f3c54e564b43c8fba3a359f303375bf93";

function loadFixtureBundle(): unknown {
  return JSON.parse(readFileSync(FIXTURE_PATH, "utf8"));
}

describe("canonicalSha256", () => {
  it("reproduces the Phase 2A registry bundle_sha256 exactly", async () => {
    const bundle = loadFixtureBundle();
    const computed = await canonicalSha256(bundle);
    expect(computed).toBe(EXPECTED_SHA256);
  });

  it("is whitespace-independent (canonical JSON)", async () => {
    const bundle = loadFixtureBundle();
    // Round-trip through stringify+parse to drop original formatting.
    const reformatted = JSON.parse(JSON.stringify(bundle));
    const a = await canonicalSha256(bundle);
    const b = await canonicalSha256(reformatted);
    expect(a).toBe(b);
  });
});

describe("installBundle", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    __resetSeedBundlesForTest();
  });

  it("happy path: validates shape, matches hash, mounts via WASM", async () => {
    mocks.installDeclarativeBundle.mockResolvedValueOnce({
      decoder_id: "declarative.uniswap/v2/swapExactTokensForTokens",
      bundle_id: "uniswap/v2/swapExactTokensForTokens@1.0.0",
    });

    const bundle = loadFixtureBundle();
    const result = await installBundle(bundle, EXPECTED_SHA256);

    expect(result).toMatchObject({
      decoderId: "declarative.uniswap/v2/swapExactTokensForTokens",
      bundleId: "uniswap/v2/swapExactTokensForTokens@1.0.0",
    });
    // Phase 6 — the mount result now also exposes the parsed bundle so
    // the orchestrator can decode calldata against the bundle's ABI.
    expect(result.bundle).toBeDefined();
    expect(result.bundle.id).toBe("uniswap/v2/swapExactTokensForTokens@1.0.0");
    expect(mocks.installDeclarativeBundle).toHaveBeenCalledTimes(1);
  });

  it("throws InstallError('bundle_hash_mismatch') when sha differs", async () => {
    const bundle = loadFixtureBundle();
    const wrong = "0x" + "00".repeat(32);

    try {
      await installBundle(bundle, wrong);
      expect.fail("expected throw");
    } catch (err) {
      expect(err).toBeInstanceOf(InstallError);
      expect((err as InstallError).code).toBe("bundle_hash_mismatch");
      expect((err as InstallError).details.expected).toBe(wrong);
      expect((err as InstallError).details.computed).toBe(EXPECTED_SHA256);
    }
    expect(mocks.installDeclarativeBundle).not.toHaveBeenCalled();
  });

  it("hash check is case-insensitive on the expected side", async () => {
    mocks.installDeclarativeBundle.mockResolvedValueOnce({
      decoder_id: "declarative.x",
      bundle_id: "x@1.0.0",
    });

    const bundle = loadFixtureBundle();
    const uppercase = "0x" + EXPECTED_SHA256.slice(2).toUpperCase();

    await expect(installBundle(bundle, uppercase)).resolves.toBeDefined();
  });

  it("throws InstallError('schema_invalid') on a malformed bundle", async () => {
    const bad = {
      type: "adapter_function",
      id: "bad/x@1.0.0",
      publisher: "x.eth",
      match: {
        chain_ids: [1],
        to: ["0x0000000000000000000000000000000000000001"],
        selector: "0xshort", // invalid — must be 0x + 8 hex
      },
      abi_fragment: { function_name: "x", abi: {} },
      emit: { strategy: "single_emit", category: "x", action: "y", fields: {} },
      requires: { imperative: [], host_capabilities: [], extension: ">=0.1.0" },
    };

    try {
      await installBundle(bad, EXPECTED_SHA256);
      expect.fail("expected throw");
    } catch (err) {
      expect(err).toBeInstanceOf(InstallError);
      expect((err as InstallError).code).toBe("schema_invalid");
    }
    expect(mocks.installDeclarativeBundle).not.toHaveBeenCalled();
  });

  it("wraps engine errors as InstallError('wasm_install_failed')", async () => {
    mocks.installDeclarativeBundle.mockRejectedValueOnce(
      new Error("engine boom"),
    );

    const bundle = loadFixtureBundle();

    try {
      await installBundle(bundle, EXPECTED_SHA256);
      expect.fail("expected throw");
    } catch (err) {
      expect(err).toBeInstanceOf(InstallError);
      expect((err as InstallError).code).toBe("wasm_install_failed");
    }
  });
});
