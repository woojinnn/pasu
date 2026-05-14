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

  it("lists oracle.usd_value in the registered methods", async () => {
    const response = await fetch(`${baseUrl}/v1/methods`);

    await expect(response.json()).resolves.toEqual({
      methods: ["oracle.usd_value"],
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
});
