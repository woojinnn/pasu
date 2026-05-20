/**
 * Phase 6 — `tryDeclarativeRoute` cases.
 *
 * The route helper composes `resolveAdapter` + the bundle-aware calldata
 * decoder + the WASM `declarativeRouteRequest` entry. We isolate it from
 * the network and the real WASM by mocking those two boundaries; the
 * decoder runs for real (it's pure code over the bundle JSON).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";

const FIXTURE_PATH = path.resolve(
  __dirname,
  "../../../../../crates/adapters/mappers/tests/fixtures/uniswap-v2-swap-exact-tokens.json",
);
const V2_RAW_TX_PATH = path.resolve(
  __dirname,
  "../../../../../crates/integration-tests/data/golden/inputs/swap_uniswap_v2_exact_in.json",
);

const fixtureText = readFileSync(FIXTURE_PATH, "utf8");
const fixtureBundle = JSON.parse(fixtureText);
const calldata = JSON.parse(readFileSync(V2_RAW_TX_PATH, "utf8")).rpc.params[0]
  .data as string;

const mocks = vi.hoisted(() => {
  class MockEngineError extends Error {
    constructor(
      readonly kind: string,
      message: string,
    ) {
      super(message);
      this.name = "EngineError";
    }
  }
  return {
    MockEngineError,
    resolveAdapter: vi.fn(),
    declarativeRouteRequest: vi.fn(),
  };
});

vi.mock("webextension-polyfill", () => ({
  default: {
    runtime: {
      getURL: vi.fn((p: string) => `chrome-extension://scopeball/${p}`),
    },
  },
}));

vi.mock("../jit-fetcher", () => ({
  resolveAdapter: mocks.resolveAdapter,
}));

vi.mock("../../wasm-bridge", () => ({
  EngineError: mocks.MockEngineError,
  declarativeRouteRequest: mocks.declarativeRouteRequest,
}));

import { tryDeclarativeRoute } from "../declarative-route";

const V2_ROUTER = "0x7a250d5630b4cf539739df2c5dacb4c659f2488d";

function adapterHit(source: "layer1" | "jit" = "layer1") {
  return {
    kind: "adapter" as const,
    source,
    adapter: {
      decoderId: "declarative.uniswap/v2/swapExactTokensForTokens",
      bundleId: "uniswap/v2/swapExactTokensForTokens@1.0.0",
      bundle: fixtureBundle,
    },
  };
}

describe("tryDeclarativeRoute", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it("returns no_selector for empty calldata", async () => {
    const outcome = await tryDeclarativeRoute({
      chainId: 1,
      from: "0x" + "1".repeat(40),
      to: V2_ROUTER,
      calldataHex: "0x",
    });
    expect(outcome).toEqual({ kind: "miss", reason: "no_selector" });
    expect(mocks.resolveAdapter).not.toHaveBeenCalled();
  });

  it("forwards resolveAdapter negative-cache verdicts as miss outcomes", async () => {
    mocks.resolveAdapter.mockResolvedValueOnce({
      kind: "verdict",
      verdict: "no_adapter",
      reason: "no_publisher",
    });

    const outcome = await tryDeclarativeRoute({
      chainId: 1,
      from: "0x" + "1".repeat(40),
      to: V2_ROUTER,
      calldataHex: calldata,
    });
    expect(outcome).toEqual({ kind: "miss", reason: "no_publisher" });
    expect(mocks.declarativeRouteRequest).not.toHaveBeenCalled();
  });

  it("invokes the WASM route entry with bridge-friendly input on hit", async () => {
    mocks.resolveAdapter.mockResolvedValueOnce(adapterHit("layer1"));
    mocks.declarativeRouteRequest.mockResolvedValueOnce({
      envelopes: [
        { category: "dex", action: "swap", fields: { swapMode: "exact_in" } },
      ],
      decoder_id: "declarative.uniswap/v2/swapExactTokensForTokens",
    });

    const outcome = await tryDeclarativeRoute({
      chainId: 1,
      from: "0x" + "1".repeat(40),
      to: V2_ROUTER,
      calldataHex: calldata,
      options: { blockTimestamp: 1_700_000_000 },
    });

    expect(outcome.kind).toBe("hit");
    if (outcome.kind !== "hit") return;
    expect(outcome.value.decoderId).toBe(
      "declarative.uniswap/v2/swapExactTokensForTokens",
    );
    expect(outcome.value.bundleId).toBe(
      "uniswap/v2/swapExactTokensForTokens@1.0.0",
    );
    expect(outcome.value.source).toBe("layer1");
    expect(outcome.value.envelopes).toHaveLength(1);

    expect(mocks.resolveAdapter).toHaveBeenCalledOnce();
    expect(mocks.declarativeRouteRequest).toHaveBeenCalledOnce();
    const arg = mocks.declarativeRouteRequest.mock.calls[0][0];
    expect(arg.chain_id).toBe(1);
    expect(arg.to.toLowerCase()).toBe(V2_ROUTER);
    expect(arg.selector).toBe("0x38ed1739");
    expect(arg.ctx.block_timestamp).toBe(1_700_000_000);
    // Confirms the raw calldata is forwarded to the WASM entry — decode
    // correctness is now owned by Rust (abi-resolver decode_with_json_abi).
    expect(arg.calldata).toBe(calldata);
  });

  it("downgrades engine 'no_declarative_mapper' to a miss outcome", async () => {
    mocks.resolveAdapter.mockResolvedValueOnce(adapterHit());
    mocks.declarativeRouteRequest.mockRejectedValueOnce(
      new mocks.MockEngineError("no_declarative_mapper", "no bundle"),
    );

    const outcome = await tryDeclarativeRoute({
      chainId: 1,
      from: "0x" + "1".repeat(40),
      to: V2_ROUTER,
      calldataHex: calldata,
    });
    expect(outcome).toEqual({
      kind: "miss",
      reason: "no_declarative_mapper",
    });
  });

  it("surfaces engine 'map_failed' as a fault", async () => {
    mocks.resolveAdapter.mockResolvedValueOnce(adapterHit());
    mocks.declarativeRouteRequest.mockRejectedValueOnce(
      new mocks.MockEngineError("map_failed", "interpreter rejected"),
    );

    const outcome = await tryDeclarativeRoute({
      chainId: 1,
      from: "0x" + "1".repeat(40),
      to: V2_ROUTER,
      calldataHex: calldata,
    });
    expect(outcome.kind).toBe("fault");
    if (outcome.kind === "fault") {
      expect(outcome.reason).toBe("map_failed");
    }
  });

  it("surfaces decode failures as faults", async () => {
    mocks.resolveAdapter.mockResolvedValueOnce(adapterHit());
    // WASM rejects because calldata cannot be decoded against the bundle ABI.
    // The TS layer is gone; decode failures now originate in WASM.
    mocks.declarativeRouteRequest.mockRejectedValueOnce(
      new mocks.MockEngineError("decode_failed", "calldata decode failed"),
    );
    const outcome = await tryDeclarativeRoute({
      chainId: 1,
      from: "0x" + "1".repeat(40),
      to: V2_ROUTER,
      calldataHex: "0xdeadbeef" + calldata.slice(10),
    });
    expect(outcome.kind).toBe("fault");
    if (outcome.kind === "fault") {
      expect(outcome.reason).toBe("decode_failed");
    }
    expect(mocks.declarativeRouteRequest).toHaveBeenCalledOnce();
  });
});
