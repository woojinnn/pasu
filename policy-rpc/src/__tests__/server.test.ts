import type { AddressInfo } from "node:net";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { createPolicyRpcServer } from "../server";

const wethAddress = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";

describe("policy-rpc HTTP server", () => {
  let server: ReturnType<typeof createPolicyRpcServer> | undefined;
  let baseUrl = "";

  beforeEach(async () => {
    server = createPolicyRpcServer({
      fetch: async (input) => {
        return new Response(
          JSON.stringify({
            [wethAddress]: {
              usd: 3500.12,
              last_updated_at: 1778750000,
            },
          }),
          {
            status: 200,
            headers: { "content-type": "application/json" },
          },
        );
      },
      nowMs: () => 1778750005000,
    });

    await new Promise<void>((resolve) => {
      server?.listen(0, "127.0.0.1", resolve);
    });

    const address = server.address() as AddressInfo;
    baseUrl = `http://127.0.0.1:${address.port}`;
  });

  afterEach(async () => {
    if (!server) {
      return;
    }

    await new Promise<void>((resolve, reject) => {
      server?.close((error) => {
        if (error) {
          reject(error);
          return;
        }

        resolve();
      });
    });
    server = undefined;
  });

  it("returns health status", async () => {
    const response = await fetch(`${baseUrl}/health`);

    await expect(response.json()).resolves.toEqual({ ok: true });
  });

  it("lists registered methods (legacy name list)", async () => {
    // Phase 8.5: the endpoint now also returns the full `catalog`
    // field, but the legacy `methods: [...]` array stays for clients
    // (and tests) that only need the names.
    const response = await fetch(`${baseUrl}/v1/methods`);
    const body = (await response.json()) as { methods: string[] };

    expect(body.methods).toEqual([
      "approval.allowance",
      "approval.cover_inputs",
      "clock.now",
      "oracle.effective_rate_bps",
      "oracle.usd_value",
      "portfolio.balance",
      "portfolio.input_fraction_bps",
      "scopeball.evaluate_v3",
      "stat_window.snapshot",
      "stat_window.swap_stats",
    ]);
  });

  it("exposes the full method catalog with params + returns metadata", async () => {
    const response = await fetch(`${baseUrl}/v1/methods`);
    const body = (await response.json()) as {
      catalog: { methods: Record<string, unknown> };
    };
    const entry = body.catalog.methods["oracle.usd_value"] as {
      name: string;
      params: Record<string, { type: string; enum_?: string[] }>;
      returns: { kind: string; type: string };
      origin: string;
    };
    expect(entry.name).toBe("oracle.usd_value");
    expect(entry.returns).toEqual({ kind: "record", type: "UsdValuation" });
    // `source` is the enum that drives the manifest editor's
    // source-dropdown — without this assertion, accidentally dropping
    // the catalog wiring would silently disable the discovery feature.
    expect(entry.params.source).toBeDefined();
    expect(entry.params.source.type).toBe("String");
    expect(entry.params.source.enum_).toContain("coingecko");
    expect(entry.origin).toBe("bundled");
  });

  it("executes host-capability mock methods in one batch", async () => {
    const rpcResponse = await fetch(`${baseUrl}/v1/rpc`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        request_id: "eval-host-mocks",
        calls: [
          { id: "now", method: "clock.now", params: {} },
          {
            id: "allowance",
            method: "approval.allowance",
            params: { allowance: "100", requested_amount: "50" },
          },
          { id: "cover", method: "approval.cover_inputs", params: {} },
          { id: "balance", method: "portfolio.balance", params: { balance: "7" } },
          {
            id: "fraction",
            method: "portfolio.input_fraction_bps",
            params: { bps: 125 },
          },
          {
            id: "rate",
            method: "oracle.effective_rate_bps",
            params: { bps: 9900 },
          },
          {
            id: "stats",
            method: "stat_window.swap_stats",
            params: { swap_volume_usd_24h: "42.0000", swap_count_24h: 3 },
          },
        ],
      }),
    });

    await expect(rpcResponse.json()).resolves.toEqual({
      request_id: "eval-host-mocks",
      results: [
        { id: "now", ok: true, result: { nowTs: 1778750005 } },
        {
          id: "allowance",
          ok: true,
          result: {
            allowance: "100",
            coversRequestedAmount: true,
            hasUnlimitedAllowance: false,
          },
        },
        {
          id: "cover",
          ok: true,
          result: {
            allowancesCoverInputs: true,
            hasUnlimitedAllowance: false,
          },
        },
        { id: "balance", ok: true, result: { balance: "7" } },
        { id: "fraction", ok: true, result: { bps: 125 } },
        { id: "rate", ok: true, result: { bps: 9900 } },
        {
          id: "stats",
          ok: true,
          result: { swapVolumeUsd24h: "42.0000", swapCount24h: 3 },
        },
      ],
    });
  });

  it("executes an oracle.usd_value batch and records recent debug metadata", async () => {
    const rpcResponse = await fetch(`${baseUrl}/v1/rpc`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        request_id: "eval-123",
        calls: [
          {
            id: "swap-total-input-usd",
            method: "oracle.usd_value",
            params: {
              chain_id: 1,
              address: wethAddress,
              amount: "1000000000000000000",
              decimals: 18,
            },
          },
        ],
      }),
    });

    await expect(rpcResponse.json()).resolves.toEqual({
      request_id: "eval-123",
      results: [
        {
          id: "swap-total-input-usd",
          ok: true,
          result: {
            value: "3500.1200",
            asOfTs: 1778750000,
            staleSec: 5,
            sources: ["coingecko"],
          },
        },
      ],
    });

    const debugResponse = await fetch(`${baseUrl}/debug/recent`);
    const debug = await debugResponse.json();

    expect(debug.entries).toHaveLength(1);
    expect(debug.entries[0]).toMatchObject({
      request_id: "eval-123",
      calls: [
        {
          id: "swap-total-input-usd",
          method: "oracle.usd_value",
          ok: true,
        },
      ],
    });
    expect(debug.entries[0].duration_ms).toEqual(expect.any(Number));
    expect(debug.entries[0].calls[0].duration_ms).toEqual(expect.any(Number));
  });

  it("executes ERC-20 and native asset-object params in one batch", async () => {
    const rpcResponse = await fetch(`${baseUrl}/v1/rpc`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        request_id: "eval-assets",
        calls: [
          {
            id: "erc20-input",
            method: "oracle.usd_value",
            params: {
              chain_id: 1,
              asset: {
                kind: "erc20",
                address: wethAddress,
                symbol: "WETH",
                decimals: 18,
              },
              amount: "1000000000000000000",
            },
          },
          {
            id: "native-input",
            method: "oracle.usd_value",
            params: {
              chain_id: 1,
              asset: {
                kind: "native",
                symbol: "ETH",
                decimals: 18,
              },
              amount: "2000000000000000000",
            },
          },
        ],
      }),
    });

    await expect(rpcResponse.json()).resolves.toEqual({
      request_id: "eval-assets",
      results: [
        {
          id: "erc20-input",
          ok: true,
          result: {
            value: "3500.1200",
            asOfTs: 1778750000,
            staleSec: 5,
            sources: ["coingecko"],
          },
        },
        {
          id: "native-input",
          ok: true,
          result: {
            value: "7000.2400",
            asOfTs: 1778750000,
            staleSec: 5,
            sources: ["coingecko"],
          },
        },
      ],
    });
  });

  it("returns per-call invalid_params errors and logs failed calls", async () => {
    const rpcResponse = await fetch(`${baseUrl}/v1/rpc`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        request_id: "eval-invalid",
        calls: [
          {
            id: "nft-value",
            method: "oracle.usd_value",
            params: {
              chain_id: 1,
              asset: {
                kind: "erc721",
                address: wethAddress,
                symbol: "NFT",
                decimals: 0,
              },
              amount: "1",
            },
          },
        ],
      }),
    });

    await expect(rpcResponse.json()).resolves.toEqual({
      request_id: "eval-invalid",
      results: [
        {
          id: "nft-value",
          ok: false,
          error: {
            code: "invalid_params",
            message: "asset.kind must be erc20 or native",
          },
        },
      ],
    });

    const debugResponse = await fetch(`${baseUrl}/debug/recent`);
    const debug = await debugResponse.json();
    expect(debug.entries[0]).toMatchObject({
      request_id: "eval-invalid",
      calls: [
        {
          id: "nft-value",
          method: "oracle.usd_value",
          ok: false,
          error_code: "invalid_params",
        },
      ],
    });
  });
});
