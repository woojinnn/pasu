import { describe, expect, it } from "vitest";

import { createOracleUsdValueMethod } from "../oracle-usd-value";

const wethAddress = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";

describe("oracle.usd_value", () => {
  it("scales raw token amounts with bigint-safe decimal math", async () => {
    const requestedUrls: string[] = [];
    const method = createOracleUsdValueMethod({
      fetch: async (input) => {
        requestedUrls.push(String(input));

        return new Response(
          JSON.stringify({
            [wethAddress]: {
              usd: "2.5000",
              last_updated_at: 1778750000,
            },
          }),
          {
            status: 200,
            headers: { "content-type": "application/json" },
          },
        );
      },
      nowMs: () => 1778750009000,
    });

    const result = await method({
      chain_id: 1,
      address: wethAddress,
      amount: "123456789012345678901234567890",
      decimals: 18,
    });

    expect(result).toEqual({
      value: "308641972530.8641",
      asOfTs: 1778750000,
      staleSec: 9,
      sources: ["coingecko"],
    });
    expect(requestedUrls[0]).toContain("/simple/token_price/ethereum");
    expect(requestedUrls[0]).toContain(`contract_addresses=${wethAddress}`);
  });

  it("accepts action asset params for ERC-20 inputs", async () => {
    const requestedUrls: string[] = [];
    const method = createOracleUsdValueMethod({
      fetch: async (input) => {
        requestedUrls.push(String(input));

        return new Response(
          JSON.stringify({
            [wethAddress]: {
              usd: "2000.0000",
              last_updated_at: 1778750000,
            },
          }),
          {
            status: 200,
            headers: { "content-type": "application/json" },
          },
        );
      },
      nowMs: () => 1778750009000,
    });

    const result = await method({
      chain_id: 1,
      asset: {
        kind: "erc20",
        address: wethAddress,
        symbol: "WETH",
        decimals: 18,
      },
      amount: "1000000000000000000",
    });

    expect(result.value).toBe("2000.0000");
    expect(requestedUrls[0]).toContain(`contract_addresses=${wethAddress}`);
  });

  it("prices native assets through the wrapped native token address", async () => {
    const requestedUrls: string[] = [];
    const method = createOracleUsdValueMethod({
      fetch: async (input) => {
        requestedUrls.push(String(input));

        return new Response(
          JSON.stringify({
            [wethAddress]: {
              usd: "2100.0000",
              last_updated_at: 1778750000,
            },
          }),
          {
            status: 200,
            headers: { "content-type": "application/json" },
          },
        );
      },
      nowMs: () => 1778750009000,
    });

    const result = await method({
      chain_id: 1,
      asset: {
        kind: "native",
        symbol: "ETH",
        decimals: 18,
      },
      amount: "2000000000000000000",
    });

    expect(result.value).toBe("4200.0000");
    expect(requestedUrls[0]).toContain(`contract_addresses=${wethAddress}`);
  });

  it("rejects unsupported asset kinds", async () => {
    const method = createOracleUsdValueMethod({
      fetch: async () => {
        throw new Error("fetch should not be called");
      },
    });

    await expect(
      method({
        chain_id: 1,
        asset: {
          kind: "erc721",
          address: wethAddress,
          decimals: 0,
        },
        amount: "1",
      }),
    ).rejects.toMatchObject({
      code: "invalid_params",
      message: "asset.kind must be erc20 or native",
    });
  });

  it("returns a not_found method error when CoinGecko has no USD price", async () => {
    const method = createOracleUsdValueMethod({
      fetch: async () =>
        new Response(JSON.stringify({}), {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
      nowMs: () => 1778750009000,
    });

    await expect(
      method({
        chain_id: 1,
        address: wethAddress,
        amount: "1000000000000000000",
        decimals: 18,
      }),
    ).rejects.toMatchObject({
      code: "not_found",
      message: "CoinGecko returned no USD price",
    });
  });
});
