import type { PublicClient } from "viem";
import { describe, expect, it, vi } from "vitest";

import { ChainlinkSource } from "../chainlink";

const wethAddress = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const fakeFeed = "0x5f4ec3df9cbd43714fe2740f5e3616155c5b8419";

interface RoundDataOverrides {
  answer?: bigint;
  updatedAt?: bigint;
  decimals?: number;
}

function buildClient(overrides: RoundDataOverrides = {}): PublicClient {
  const answer = overrides.answer ?? 2_000_00000000n;
  const updatedAt = overrides.updatedAt ?? 1_778_750_000n;
  const decimals = overrides.decimals ?? 8;

  return {
    readContract: vi.fn(async (parameters: { functionName: string }) => {
      if (parameters.functionName === "latestRoundData") {
        return [1n, answer, updatedAt, updatedAt, 1n] as const;
      }
      if (parameters.functionName === "decimals") {
        return decimals;
      }
      throw new Error(`unexpected function ${parameters.functionName}`);
    }),
  } as unknown as PublicClient;
}

describe("ChainlinkSource", () => {
  it("returns the latest round price rescaled to 1e8", async () => {
    const updatedAtSec = 1_778_750_000;
    const client = buildClient({
      answer: 2_000_50000000n,
      updatedAt: BigInt(updatedAtSec),
      decimals: 8,
    });

    const source = new ChainlinkSource({
      feeds: { 1: { [wethAddress]: fakeFeed } },
      publicClient: client,
      nowMs: () => updatedAtSec * 1000 + 30_000,
    });

    const sample = await source.fetch(1, { address: wethAddress });

    expect(sample.sourceId).toBe("chainlink");
    expect(sample.decimals).toBe(8);
    expect(sample.usd).toBe(2_000_50000000n);
    expect(sample.observedAt).toBe(updatedAtSec * 1000);
  });

  it("rescales feeds with non-8 decimals", async () => {
    const updatedAtSec = 1_778_750_000;
    // Feed quotes with 4 decimals -> 25000 means $2.5000
    const client = buildClient({
      answer: 25000n,
      updatedAt: BigInt(updatedAtSec),
      decimals: 4,
    });

    const source = new ChainlinkSource({
      feeds: { 1: { [wethAddress]: fakeFeed } },
      publicClient: client,
      nowMs: () => updatedAtSec * 1000,
    });

    const sample = await source.fetch(1, { address: wethAddress });
    expect(sample.usd).toBe(2_50000000n);
  });

  it("rejects unknown tokens as unsupported_token", async () => {
    const source = new ChainlinkSource({
      feeds: { 1: {} },
      publicClient: buildClient(),
    });

    await expect(
      source.fetch(1, { address: "0xdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef" }),
    ).rejects.toMatchObject({
      code: "unsupported_token",
      sourceId: "chainlink",
    });
  });

  it("rejects stale rounds beyond the configured budget", async () => {
    const oldSec = 1_000;
    const client = buildClient({
      answer: 2_000_00000000n,
      updatedAt: BigInt(oldSec),
      decimals: 8,
    });

    const source = new ChainlinkSource({
      feeds: { 1: { [wethAddress]: fakeFeed } },
      publicClient: client,
      maxAgeSec: 60 * 60,
      nowMs: () => (oldSec + 2 * 3600) * 1000, // 2 hours later
    });

    await expect(source.fetch(1, { address: wethAddress })).rejects.toMatchObject({
      code: "stale",
      sourceId: "chainlink",
    });
  });

  it("rejects non-positive answers as invalid_response", async () => {
    const updatedAtSec = 1_778_750_000;
    const client = buildClient({
      answer: 0n,
      updatedAt: BigInt(updatedAtSec),
      decimals: 8,
    });

    const source = new ChainlinkSource({
      feeds: { 1: { [wethAddress]: fakeFeed } },
      publicClient: client,
      nowMs: () => updatedAtSec * 1000,
    });

    await expect(source.fetch(1, { address: wethAddress })).rejects.toMatchObject({
      code: "invalid_response",
      sourceId: "chainlink",
    });
  });

  it("wraps RPC failures as unavailable", async () => {
    const client = {
      readContract: vi.fn(async () => {
        throw new Error("rpc transport down");
      }),
    } as unknown as PublicClient;

    const source = new ChainlinkSource({
      feeds: { 1: { [wethAddress]: fakeFeed } },
      publicClient: client,
    });

    await expect(source.fetch(1, { address: wethAddress })).rejects.toMatchObject({
      code: "unavailable",
      sourceId: "chainlink",
    });
  });

  it("seeds mainnet feeds for USDC/USDT/WETH/WBTC/DAI by default", async () => {
    const knownTokens = [
      wethAddress,
      "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", // USDC
      "0xdac17f958d2ee523a2206206994597c13d831ec7", // USDT
      "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599", // WBTC
      "0x6b175474e89094c44da98b954eedeac495271d0f", // DAI
    ];
    const updatedAtSec = 1_778_750_000;
    const source = new ChainlinkSource({
      publicClient: buildClient({
        answer: 1_00000000n,
        updatedAt: BigInt(updatedAtSec),
        decimals: 8,
      }),
      nowMs: () => updatedAtSec * 1000,
    });

    for (const address of knownTokens) {
      const sample = await source.fetch(1, { address });
      expect(sample.usd).toBe(1_00000000n);
    }
  });
});
