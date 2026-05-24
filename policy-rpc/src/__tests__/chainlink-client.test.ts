import { describe, expect, it } from "vitest";
import {
  ChainlinkClient,
  chainlinkFeedFor,
  chainRpcUrlsFromEnv,
  chainRpcUrlsFromMap,
} from "../chainlink-client.js";
import { RpcMethodError, type FetchLike } from "../types.js";

// ── RPC URL config ────────────────────────────────────────────────

describe("chainRpcUrlsFromMap", () => {
  it("returns the ordered URL list for configured chains", () => {
    const cfg = chainRpcUrlsFromMap({
      "1": ["https://primary/rpc", "https://backup/rpc"],
    });
    expect([...cfg.forChain(1)]).toEqual([
      "https://primary/rpc",
      "https://backup/rpc",
    ]);
  });

  it("accepts a bare string and normalizes to a single-element array", () => {
    const cfg = chainRpcUrlsFromMap({ "1": "https://mainnet.example/rpc" });
    expect([...cfg.forChain(1)]).toEqual(["https://mainnet.example/rpc"]);
  });

  it("throws unsupported_chain for chains with no URLs", () => {
    const cfg = chainRpcUrlsFromMap({ "1": "https://only-mainnet" });
    let captured: unknown;
    try {
      cfg.forChain(99);
    } catch (e) {
      captured = e;
    }
    expect(captured).toBeInstanceOf(RpcMethodError);
    expect((captured as RpcMethodError).code).toBe("unsupported_chain");
  });

  it("rejects malformed entries at construction", () => {
    expect(() => chainRpcUrlsFromMap({ abc: "https://x" })).toThrow(
      /invalid chain id/,
    );
    expect(() => chainRpcUrlsFromMap({ "1": "" })).toThrow(
      /non-empty string/,
    );
    expect(() => chainRpcUrlsFromMap({ "1": [] })).toThrow(
      /must not be empty/,
    );
    expect(() =>
      chainRpcUrlsFromMap({ "1": ["valid", ""] }),
    ).toThrow(/non-empty string/);
  });
});

describe("chainRpcUrlsFromEnv", () => {
  it("returns the bundled public RPCs when no env var is set", () => {
    const cfg = chainRpcUrlsFromEnv({ env: {} });
    // Default table ships mainnet — we just check the chain resolves to
    // at least one URL, not the exact list (the list is curated and may
    // change as endpoints come and go).
    const mainnet = cfg.forChain(1);
    expect(mainnet.length).toBeGreaterThan(0);
    expect(mainnet[0]).toMatch(/^https:\/\//);
  });

  it("falls back to unsupported_chain for chains outside the default table", () => {
    const cfg = chainRpcUrlsFromEnv({ env: {} });
    expect(() => cfg.forChain(99_999)).toThrow(/No RPC URL configured for chain/);
  });

  it("prepends user-supplied URLs over the bundled defaults", () => {
    const cfg = chainRpcUrlsFromEnv({
      env: { POLICY_RPC_CHAIN_RPCS: '{"1":"https://my-rpc/v1"}' },
    });
    const mainnet = cfg.forChain(1);
    expect(mainnet[0]).toBe("https://my-rpc/v1");
    // Public defaults still attached as backup.
    expect(mainnet.length).toBeGreaterThan(1);
  });

  it("accepts an array env value and preserves order", () => {
    const cfg = chainRpcUrlsFromEnv({
      env: {
        POLICY_RPC_CHAIN_RPCS:
          '{"1":["https://my-primary","https://my-backup"]}',
      },
    });
    const mainnet = cfg.forChain(1);
    expect(mainnet.slice(0, 2)).toEqual([
      "https://my-primary",
      "https://my-backup",
    ]);
  });

  it("deduplicates URLs that appear in both user and default lists", () => {
    // User pinning a URL that's already in the default table shouldn't
    // make us try it twice on every request.
    const cfg = chainRpcUrlsFromEnv({
      env: { POLICY_RPC_CHAIN_RPCS: '{"1":"https://eth.llamarpc.com"}' },
    });
    const mainnet = cfg.forChain(1);
    const llamarpcCount = mainnet.filter(
      (u) => u === "https://eth.llamarpc.com",
    ).length;
    expect(llamarpcCount).toBe(1);
  });

  it("disables defaults entirely when POLICY_RPC_DISABLE_PUBLIC_RPCS=1", () => {
    const cfg = chainRpcUrlsFromEnv({
      env: {
        POLICY_RPC_CHAIN_RPCS: '{"1":"https://my-rpc"}',
        POLICY_RPC_DISABLE_PUBLIC_RPCS: "1",
      },
    });
    expect([...cfg.forChain(1)]).toEqual(["https://my-rpc"]);
    // Chains without user URLs no longer resolve.
    expect(() => cfg.forChain(10)).toThrow(/No RPC URL configured/);
  });

  it("disablePublicRpcs option overrides env var", () => {
    const cfg = chainRpcUrlsFromEnv({
      disablePublicRpcs: true,
      env: { POLICY_RPC_CHAIN_RPCS: '{"1":"https://my-rpc"}' },
    });
    expect([...cfg.forChain(1)]).toEqual(["https://my-rpc"]);
  });

  it("throws loudly on malformed JSON instead of silently disabling", () => {
    expect(() =>
      chainRpcUrlsFromEnv({ env: { POLICY_RPC_CHAIN_RPCS: "not-json" } }),
    ).toThrow(/POLICY_RPC_CHAIN_RPCS is not valid JSON/);
    expect(() =>
      chainRpcUrlsFromEnv({ env: { POLICY_RPC_CHAIN_RPCS: "[]" } }),
    ).toThrow(/must be a JSON object/);
    expect(() =>
      chainRpcUrlsFromEnv({
        env: { POLICY_RPC_CHAIN_RPCS: '{"1": 12345}' },
      }),
    ).toThrow(/must be a string or array of strings/);
  });
});

// ── Feed directory ─────────────────────────────────────────────────

describe("chainlinkFeedFor", () => {
  it("looks up WETH on Ethereum mainnet", () => {
    const feed = chainlinkFeedFor(1, "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2");
    expect(feed).toBeDefined();
    expect(feed?.feedDecimals).toBe(8);
  });

  it("is case-insensitive on the token address", () => {
    const lower = chainlinkFeedFor(1, "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2");
    const upper = chainlinkFeedFor(1, "0xC02AAA39B223FE8D0A0E5C4F27EAD9083C756CC2");
    expect(upper).toEqual(lower);
  });

  it("returns undefined for unknown (chain, token) pairs", () => {
    expect(chainlinkFeedFor(1, "0x0000000000000000000000000000000000000000")).toBeUndefined();
    expect(chainlinkFeedFor(999, "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2")).toBeUndefined();
  });
});

// ── Client (eth_call + ABI decode + failover) ───────────────────────

/**
 * Build a fake `eth_call` reply where the 5-word return tuple
 * encodes the given `answer` (int256) and `updatedAt` (uint256).
 * Word 0/2/4 are filler zeros — the decoder skips them.
 */
function encodeLatestRoundDataReply(answer: bigint, updatedAt: number): string {
  const word = (n: bigint) =>
    n.toString(16).padStart(64, n >= 0n ? "0" : "f");
  const twosComplementInt256 = (n: bigint): bigint =>
    n < 0n ? n + (1n << 256n) : n;
  const parts = [
    "0".repeat(64), // roundId
    word(twosComplementInt256(answer)),
    "0".repeat(64), // startedAt
    word(BigInt(updatedAt)),
    "0".repeat(64), // answeredInRound
  ];
  return `0x${parts.join("")}`;
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function successFetch(answer: bigint, updatedAt: number): FetchLike {
  return async () =>
    jsonResponse({
      jsonrpc: "2.0",
      id: 1,
      result: encodeLatestRoundDataReply(answer, updatedAt),
    });
}

describe("ChainlinkClient.tokenUsdPrice", () => {
  const WETH = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";

  it("decodes a positive answer and updatedAt into priceUsd + asOfTs", async () => {
    // 3500.12345678 USD per ETH at 8 decimals → answer = 350012345678
    const client = new ChainlinkClient({
      fetch: successFetch(350012345678n, 1_700_000_000),
      rpcs: chainRpcUrlsFromMap({ "1": "https://mock/rpc" }),
    });
    const out = await client.tokenUsdPrice(1, WETH);
    expect(out.priceUsd).toBe("3500.12345678");
    expect(out.asOfTs).toBe(1_700_000_000);
  });

  it("rejects unknown (chain, token) before hitting the RPC", async () => {
    let called = false;
    const client = new ChainlinkClient({
      fetch: (async () => {
        called = true;
        return jsonResponse({});
      }) as FetchLike,
      rpcs: chainRpcUrlsFromMap({ "1": "https://mock/rpc" }),
    });
    await expect(
      client.tokenUsdPrice(1, "0x0000000000000000000000000000000000000000"),
    ).rejects.toMatchObject({ code: "not_found" });
    expect(called).toBe(false);
  });

  it("rejects negative or zero answers as upstream_error", async () => {
    const client = new ChainlinkClient({
      fetch: successFetch(0n, 1_700_000_000),
      rpcs: chainRpcUrlsFromMap({ "1": "https://mock/rpc" }),
    });
    await expect(client.tokenUsdPrice(1, WETH)).rejects.toMatchObject({
      code: "upstream_error",
    });
  });

  it("surfaces eth_call JSON-RPC errors as upstream_error (after exhausting fallbacks)", async () => {
    const client = new ChainlinkClient({
      fetch: (async () =>
        jsonResponse({
          jsonrpc: "2.0",
          id: 1,
          error: { message: "node down" },
        })) as FetchLike,
      rpcs: chainRpcUrlsFromMap({ "1": "https://mock/rpc" }),
    });
    await expect(client.tokenUsdPrice(1, WETH)).rejects.toMatchObject({
      code: "upstream_error",
    });
  });

  it("fails with unsupported_chain when the chain has no RPC and no defaults", async () => {
    const client = new ChainlinkClient({
      fetch: successFetch(1n, 0),
      rpcs: chainRpcUrlsFromMap({}),
    });
    await expect(client.tokenUsdPrice(1, WETH)).rejects.toMatchObject({
      code: "unsupported_chain",
    });
  });
});

describe("ChainlinkClient failover", () => {
  const WETH = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";

  it("retries the next URL when the first returns HTTP 429", async () => {
    const seenUrls: string[] = [];
    const fetchImpl: FetchLike = async (input) => {
      const url = typeof input === "string" ? input : input.toString();
      seenUrls.push(url);
      if (url === "https://primary/rpc") {
        return new Response("rate limited", { status: 429 });
      }
      return jsonResponse({
        jsonrpc: "2.0",
        id: 1,
        result: encodeLatestRoundDataReply(100000000n, 1_700_000_001),
      });
    };
    const client = new ChainlinkClient({
      fetch: fetchImpl,
      rpcs: chainRpcUrlsFromMap({
        "1": ["https://primary/rpc", "https://backup/rpc"],
      }),
    });
    const out = await client.tokenUsdPrice(1, WETH);
    expect(out.priceUsd).toBe("1.00000000");
    expect(seenUrls).toEqual([
      "https://primary/rpc",
      "https://backup/rpc",
    ]);
  });

  it("retries the next URL when the first throws a network error", async () => {
    let calls = 0;
    const fetchImpl: FetchLike = async (input) => {
      calls += 1;
      if (calls === 1) throw new TypeError("fetch failed");
      return jsonResponse({
        jsonrpc: "2.0",
        id: 1,
        result: encodeLatestRoundDataReply(200000000n, 1_700_000_002),
      });
    };
    const client = new ChainlinkClient({
      fetch: fetchImpl,
      rpcs: chainRpcUrlsFromMap({
        "1": ["https://primary", "https://backup"],
      }),
    });
    const out = await client.tokenUsdPrice(1, WETH);
    expect(out.priceUsd).toBe("2.00000000");
    expect(calls).toBe(2);
  });

  it("returns immediately on the first success without trying further URLs", async () => {
    let calls = 0;
    const fetchImpl: FetchLike = async () => {
      calls += 1;
      return jsonResponse({
        jsonrpc: "2.0",
        id: 1,
        result: encodeLatestRoundDataReply(500000000n, 1_700_000_003),
      });
    };
    const client = new ChainlinkClient({
      fetch: fetchImpl,
      rpcs: chainRpcUrlsFromMap({
        "1": ["https://primary", "https://backup", "https://tertiary"],
      }),
    });
    await client.tokenUsdPrice(1, WETH);
    expect(calls).toBe(1);
  });

  it("aggregates exhausted endpoints into a single upstream_error with the last reason", async () => {
    const fetchImpl: FetchLike = async (input) => {
      const url = typeof input === "string" ? input : input.toString();
      if (url.endsWith("backup")) {
        return new Response("bad gateway", { status: 502 });
      }
      return new Response("rate limited", { status: 429 });
    };
    const client = new ChainlinkClient({
      fetch: fetchImpl,
      rpcs: chainRpcUrlsFromMap({
        "1": ["https://primary", "https://backup"],
      }),
    });
    await expect(client.tokenUsdPrice(1, WETH)).rejects.toMatchObject({
      code: "upstream_error",
      message: expect.stringMatching(/All 2 Chainlink RPC endpoint\(s\) failed/),
    });
  });
});
