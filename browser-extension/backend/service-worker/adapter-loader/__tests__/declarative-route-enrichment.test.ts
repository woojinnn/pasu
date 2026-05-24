/**
 * Phase 7E — `enrichEnvelopeAssets` cases.
 *
 * The enricher receives skeleton envelopes (only `kind` + `address` on
 * every AssetRef) and a `TokenRegistryClient`, and returns a deep
 * clone with `symbol`/`decimals` filled in wherever the registry has a
 * record. These tests exercise the generic walker — they cover both
 * "every action shape we currently emit" and the degraded paths
 * (unknown token, multi-action envelope sets).
 *
 * Token client is mocked; we assert call counts directly so dedupe
 * regressions show up immediately. `webextension-polyfill` is mocked to
 * keep happy-dom from importing the real polyfill.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

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

// The enrichment helper is pure, but it lives in declarative-route.ts
// which transitively loads `wasm-bridge` → the real WASM glue file.
// Stub the wasm bridge + jit-fetcher so the suite stays unit-scoped.
vi.mock("../../wasm-bridge", () => ({
  EngineError: mocks.MockEngineError,
  declarativeRouteRequest: mocks.declarativeRouteRequest,
}));

vi.mock("../jit-fetcher", () => ({
  resolveAdapter: mocks.resolveAdapter,
}));

import { enrichEnvelopeAssets } from "../declarative-route";
import type {
  TokenMetadata,
  TokenRegistryClient,
} from "../../registry/token-client";

const WETH_MAINNET = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const USDC_MAINNET = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
const UNKNOWN_ADDR = "0x0000000000000000000000000000000000000099";

const WETH_META: TokenMetadata = {
  kind: "erc20",
  chainId: 1,
  address: WETH_MAINNET,
  symbol: "WETH",
  decimals: 18,
  name: "Wrapped Ether",
};
const USDC_META: TokenMetadata = {
  kind: "erc20",
  chainId: 1,
  address: USDC_MAINNET,
  symbol: "USDC",
  decimals: 6,
  name: "USD Coin",
};

const META_BY_ADDRESS: Record<string, TokenMetadata> = {
  [WETH_MAINNET]: WETH_META,
  [USDC_MAINNET]: USDC_META,
};

function makeClient(): {
  client: TokenRegistryClient;
  lookup: ReturnType<typeof vi.fn>;
} {
  const lookup = vi.fn(async (_chainId: number, address: string) => {
    return META_BY_ADDRESS[address.toLowerCase()] ?? null;
  });
  const client: TokenRegistryClient = { lookup };
  return { client, lookup };
}

describe("enrichEnvelopeAssets", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it("returns the input as-is for an empty envelope set", async () => {
    const { client, lookup } = makeClient();
    const out = await enrichEnvelopeAssets([], 1, client);
    expect(out).toEqual([]);
    expect(lookup).not.toHaveBeenCalled();
  });

  it("fills symbol/decimals on V2 swap skeleton envelope (WETH→USDC)", async () => {
    const { client, lookup } = makeClient();
    const swap = {
      category: "dex",
      action: "swap",
      fields: {
        swapMode: "exact_in",
        inputToken: {
          asset: { kind: "erc20", address: WETH_MAINNET },
          amount: { kind: "exact", value: "1000000000000000000" },
        },
        outputToken: {
          asset: { kind: "erc20", address: USDC_MAINNET },
          amount: { kind: "min", value: "1900000" },
        },
        recipient: "0x2222222222222222222222222222222222222222",
      },
    };

    const [out] = await enrichEnvelopeAssets([swap], 1, client);
    const fields = (out as { fields: Record<string, unknown> }).fields as {
      inputToken: { asset: Record<string, unknown> };
      outputToken: { asset: Record<string, unknown> };
    };

    expect(fields.inputToken.asset).toMatchObject({
      kind: "erc20",
      address: WETH_MAINNET,
      symbol: "WETH",
      decimals: 18,
    });
    expect(fields.outputToken.asset).toMatchObject({
      kind: "erc20",
      address: USDC_MAINNET,
      symbol: "USDC",
      decimals: 6,
    });
    expect(lookup).toHaveBeenCalledTimes(2);
  });

  it("skips AssetRefs that the registry cannot resolve", async () => {
    const { client, lookup } = makeClient();
    const swap = {
      category: "dex",
      action: "swap",
      fields: {
        swapMode: "exact_in",
        inputToken: {
          asset: { kind: "erc20", address: UNKNOWN_ADDR },
          amount: { kind: "exact", value: "1000" },
        },
        outputToken: {
          asset: { kind: "erc20", address: USDC_MAINNET },
          amount: { kind: "min", value: "100" },
        },
        recipient: "0x2222222222222222222222222222222222222222",
      },
    };

    const [out] = await enrichEnvelopeAssets([swap], 1, client);
    const fields = (out as { fields: Record<string, unknown> }).fields as {
      inputToken: { asset: Record<string, unknown> };
      outputToken: { asset: Record<string, unknown> };
    };

    // Unknown → registry returns null → skeleton stays untouched.
    expect(fields.inputToken.asset).toEqual({
      kind: "erc20",
      address: UNKNOWN_ADDR,
    });
    // Resolvable → enriched as usual.
    expect(fields.outputToken.asset).toMatchObject({
      symbol: "USDC",
      decimals: 6,
    });
    expect(lookup).toHaveBeenCalledTimes(2);
  });

  it("does not call lookup for native assets (no address)", async () => {
    const { client, lookup } = makeClient();
    const wrap = {
      category: "misc",
      action: "wrap",
      fields: {
        nativeAsset: {
          // No address — `{kind: "native"}` is intentionally not enrichable.
          asset: { kind: "native", symbol: "ETH", decimals: 18 },
          amount: { kind: "exact", value: "1000" },
        },
        wrappedAsset: {
          asset: { kind: "erc20", address: WETH_MAINNET },
          amount: { kind: "exact", value: "1000" },
        },
        recipient: "0x3030303030303030303030303030303030303030",
      },
    };

    const [out] = await enrichEnvelopeAssets([wrap], 1, client);
    const fields = (out as { fields: Record<string, unknown> }).fields as {
      nativeAsset: { asset: Record<string, unknown> };
      wrappedAsset: { asset: Record<string, unknown> };
    };

    expect(fields.nativeAsset.asset).toEqual({
      kind: "native",
      symbol: "ETH",
      decimals: 18,
    });
    expect(fields.wrappedAsset.asset).toMatchObject({
      kind: "erc20",
      address: WETH_MAINNET,
      symbol: "WETH",
      decimals: 18,
    });
    expect(lookup).toHaveBeenCalledTimes(1);
    expect(lookup).toHaveBeenCalledWith(1, WETH_MAINNET);
  });

  it("traverses transfer envelope (AssetRef under fields.token.asset)", async () => {
    const { client, lookup } = makeClient();
    const transfer = {
      category: "misc",
      action: "transfer",
      fields: {
        token: {
          asset: { kind: "erc20", address: USDC_MAINNET },
          amount: { kind: "exact", value: "1000" },
        },
        from: "0x5050505050505050505050505050505050505050",
        recipient: "0x5151515151515151515151515151515151515151",
      },
    };

    const [out] = await enrichEnvelopeAssets([transfer], 1, client);
    const fields = (out as { fields: Record<string, unknown> }).fields as {
      token: { asset: Record<string, unknown> };
    };

    expect(fields.token.asset).toMatchObject({
      symbol: "USDC",
      decimals: 6,
    });
    expect(lookup).toHaveBeenCalledTimes(1);
  });

  it("traverses permit envelope (AssetRef under fields.token, no asset wrapper)", async () => {
    const { client, lookup } = makeClient();
    const permit = {
      category: "misc",
      action: "permit",
      fields: {
        permitKind: "eip2612",
        token: { kind: "erc20", address: USDC_MAINNET },
        owner: "0x5252525252525252525252525252525252525252",
        spender: "0x5353535353535353535353535353535353535353",
        amount: { kind: "exact", value: "1000" },
        validity: { expiresAt: "1700000000", source: "signature-deadline" },
      },
    };

    const [out] = await enrichEnvelopeAssets([permit], 1, client);
    const fields = (out as { fields: Record<string, unknown> }).fields as {
      token: Record<string, unknown>;
      amount: Record<string, unknown>;
    };

    expect(fields.token).toMatchObject({
      kind: "erc20",
      address: USDC_MAINNET,
      symbol: "USDC",
      decimals: 6,
    });
    // `{kind: "exact", value: ...}` (AmountConstraint) lacks `address` so
    // the shape probe filters it out — no spurious lookup.
    expect(fields.amount).toEqual({ kind: "exact", value: "1000" });
    expect(lookup).toHaveBeenCalledTimes(1);
  });

  it("enriches multiple envelopes (mixed swap + wrap)", async () => {
    const { client, lookup } = makeClient();
    const envelopes = [
      {
        category: "dex",
        action: "swap",
        fields: {
          swapMode: "exact_in",
          inputToken: {
            asset: { kind: "erc20", address: WETH_MAINNET },
            amount: { kind: "exact", value: "1000" },
          },
          outputToken: {
            asset: { kind: "erc20", address: USDC_MAINNET },
            amount: { kind: "min", value: "100" },
          },
          recipient: "0x2222222222222222222222222222222222222222",
        },
      },
      {
        category: "misc",
        action: "wrap",
        fields: {
          nativeAsset: {
            asset: { kind: "native" },
            amount: { kind: "exact", value: "1000" },
          },
          wrappedAsset: {
            asset: { kind: "erc20", address: WETH_MAINNET },
            amount: { kind: "exact", value: "1000" },
          },
          recipient: "0x3030303030303030303030303030303030303030",
        },
      },
    ];

    const out = await enrichEnvelopeAssets(envelopes, 1, client);

    expect(out).toHaveLength(2);
    const swap = out[0] as { fields: Record<string, unknown> };
    const wrap = out[1] as { fields: Record<string, unknown> };
    expect(
      (swap.fields as { inputToken: { asset: Record<string, unknown> } })
        .inputToken.asset,
    ).toMatchObject({ symbol: "WETH", decimals: 18 });
    expect(
      (wrap.fields as { wrappedAsset: { asset: Record<string, unknown> } })
        .wrappedAsset.asset,
    ).toMatchObject({ symbol: "WETH", decimals: 18 });
    // 3 calls: swap.WETH, swap.USDC, wrap.WETH — the underlying client's
    // inflight dedupe collapses concurrent same-token calls in real
    // usage, but the mock has no dedupe so 3 invocations is correct.
    expect(lookup).toHaveBeenCalledTimes(3);
  });

  it("preserves caller-supplied symbol/decimals over registry values", async () => {
    const { client, lookup } = makeClient();
    const swap = {
      category: "dex",
      action: "swap",
      fields: {
        swapMode: "exact_in",
        inputToken: {
          asset: {
            kind: "erc20",
            address: WETH_MAINNET,
            // Publisher already enriched — registry should not stomp this.
            symbol: "CUSTOM",
            decimals: 9,
          },
          amount: { kind: "exact", value: "1000" },
        },
        outputToken: {
          asset: { kind: "erc20", address: USDC_MAINNET },
          amount: { kind: "min", value: "100" },
        },
        recipient: "0x2222222222222222222222222222222222222222",
      },
    };

    const [out] = await enrichEnvelopeAssets([swap], 1, client);
    const fields = (out as { fields: Record<string, unknown> }).fields as {
      inputToken: { asset: Record<string, unknown> };
      outputToken: { asset: Record<string, unknown> };
    };

    expect(fields.inputToken.asset).toMatchObject({
      symbol: "CUSTOM",
      decimals: 9,
    });
    expect(fields.outputToken.asset).toMatchObject({
      symbol: "USDC",
      decimals: 6,
    });
    expect(lookup).toHaveBeenCalledTimes(2);
  });

  it("absorbs token client errors and returns the bare skeleton", async () => {
    const throwingClient: TokenRegistryClient = {
      lookup: vi.fn(async () => {
        throw new Error("registry network failure");
      }),
    };
    const swap = {
      category: "dex",
      action: "swap",
      fields: {
        swapMode: "exact_in",
        inputToken: {
          asset: { kind: "erc20", address: WETH_MAINNET },
          amount: { kind: "exact", value: "1000" },
        },
        outputToken: {
          asset: { kind: "erc20", address: USDC_MAINNET },
          amount: { kind: "min", value: "100" },
        },
        recipient: "0x2222222222222222222222222222222222222222",
      },
    };

    const [out] = await enrichEnvelopeAssets([swap], 1, throwingClient);
    const fields = (out as { fields: Record<string, unknown> }).fields as {
      inputToken: { asset: Record<string, unknown> };
    };

    // Lookup failed → skeleton intact, no `symbol`/`decimals`.
    expect(fields.inputToken.asset).toEqual({
      kind: "erc20",
      address: WETH_MAINNET,
    });
  });

  it("returns a deep clone — input envelopes are not mutated", async () => {
    const { client } = makeClient();
    const original = {
      category: "dex",
      action: "swap",
      fields: {
        swapMode: "exact_in",
        inputToken: {
          asset: { kind: "erc20", address: WETH_MAINNET },
          amount: { kind: "exact", value: "1000" },
        },
        outputToken: {
          asset: { kind: "erc20", address: USDC_MAINNET },
          amount: { kind: "min", value: "100" },
        },
        recipient: "0x2222222222222222222222222222222222222222",
      },
    };
    const snapshot = JSON.parse(JSON.stringify(original));

    const out = await enrichEnvelopeAssets([original], 1, client);
    expect(original).toEqual(snapshot);
    expect(out[0]).not.toBe(original);
  });
});
