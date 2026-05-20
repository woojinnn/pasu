// Sidecar loader tests — stub `fetch` so we never make a real HTTP
// call. The same `fetch` mock serves both the catalog-discovery GET
// and the RPC-forward POST so we can assert the forwarder routes
// requests to the right URL with the right body.

import { describe, expect, it, vi } from "vitest";

import { loadSidecarEntries } from "../sidecar-loader.js";
import type { SidecarConfig } from "../catalog.js";
import { RpcMethodError } from "../../types.js";

function okResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

function makeCatalogResponse(methods: Record<string, unknown>): Response {
  return okResponse({
    methods: Object.keys(methods),
    catalog: { methods },
  });
}

const RISK_SIDECAR: SidecarConfig = {
  name: "risk-svc",
  url: "http://localhost:9001",
  methodPrefix: "risk.",
};

describe("loadSidecarEntries", () => {
  it("discovers catalog entries and forces origin='sidecar'", async () => {
    const fetchImpl = vi.fn(async () =>
      makeCatalogResponse({
        "risk.score": {
          name: "risk.score",
          params: { wallet: { type: "String", required: true } },
          returns: { kind: "scalar", type: "Long", from: "$.result.value" },
          origin: "bundled", // sidecar tries to claim bundled — must be overridden
        },
      }),
    );
    const entries = await loadSidecarEntries({
      sidecars: [RISK_SIDECAR],
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });
    expect(entries).toHaveLength(1);
    expect(entries[0].catalog.name).toBe("risk.score");
    expect(entries[0].catalog.origin).toBe("sidecar");
    // Discovery hits /v1/methods on the configured URL.
    expect(fetchImpl).toHaveBeenCalledWith(
      "http://localhost:9001/v1/methods",
      { method: "GET" },
    );
  });

  it("rejects sidecar-published methods outside its declared prefix", async () => {
    const warn = vi.fn();
    const fetchImpl = vi.fn(async () =>
      makeCatalogResponse({
        "risk.score": {
          name: "risk.score",
          params: {},
          returns: { kind: "scalar", type: "Long", from: "$.result.value" },
          origin: "sidecar",
        },
        "oracle.usd_value": {
          name: "oracle.usd_value", // doesn't start with `risk.`
          params: {},
          returns: { kind: "record", type: "UsdValuation" },
          origin: "sidecar",
        },
      }),
    );
    const entries = await loadSidecarEntries({
      sidecars: [RISK_SIDECAR],
      fetchImpl: fetchImpl as unknown as typeof fetch,
      warn,
    });
    expect(entries.map((e) => e.catalog.name)).toEqual(["risk.score"]);
    expect(warn).toHaveBeenCalledWith(
      expect.stringContaining('outside its prefix "risk."'),
    );
  });

  it("warns and returns [] when sidecar is unreachable", async () => {
    const warn = vi.fn();
    const fetchImpl = vi.fn(async () => {
      throw new Error("ECONNREFUSED");
    });
    const entries = await loadSidecarEntries({
      sidecars: [RISK_SIDECAR],
      fetchImpl: fetchImpl as unknown as typeof fetch,
      warn,
    });
    expect(entries).toEqual([]);
    expect(warn).toHaveBeenCalledWith(
      expect.stringContaining("unreachable"),
    );
  });

  it("rejects sidecars with invalid configs", async () => {
    const warn = vi.fn();
    const fetchImpl = vi.fn();
    const entries = await loadSidecarEntries({
      sidecars: [
        { name: "", url: "http://localhost", methodPrefix: "x." },
        { name: "x", url: "not-a-url", methodPrefix: "x." },
        { name: "x", url: "http://localhost", methodPrefix: "" },
      ] as SidecarConfig[],
      fetchImpl: fetchImpl as unknown as typeof fetch,
      warn,
    });
    expect(entries).toEqual([]);
    expect(fetchImpl).not.toHaveBeenCalled();
    expect(warn).toHaveBeenCalledTimes(3);
  });

  it("forwarder POSTs to /v1/rpc and unwraps the first result on success", async () => {
    // First call serves the catalog; second call is the actual forward.
    const fetchImpl = vi
      .fn()
      .mockImplementationOnce(async () =>
        makeCatalogResponse({
          "risk.score": {
            name: "risk.score",
            params: {
              wallet: { type: "String", required: true },
            },
            returns: { kind: "scalar", type: "Long", from: "$.result.value" },
            origin: "sidecar",
          },
        }),
      )
      .mockImplementationOnce(async (url: string, init?: RequestInit) => {
        expect(url).toBe("http://localhost:9001/v1/rpc");
        expect(init?.method).toBe("POST");
        const body = JSON.parse(String(init!.body));
        expect(body.calls).toHaveLength(1);
        expect(body.calls[0].method).toBe("risk.score");
        expect(body.calls[0].params).toEqual({ wallet: "0xdeadbeef" });
        return okResponse({
          request_id: body.request_id,
          results: [
            { id: body.calls[0].id, ok: true, result: { value: 87 } },
          ],
        });
      });

    const entries = await loadSidecarEntries({
      sidecars: [RISK_SIDECAR],
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });
    const result = await entries[0].fn({ wallet: "0xdeadbeef" });
    expect(result).toEqual({ value: 87 });
  });

  it("forwarder surfaces sidecar errors as RpcMethodError", async () => {
    // First call is catalog discovery; every subsequent call is a
    // forward returning the same error envelope. `mockImplementation`
    // (no "Once") makes the forward stub reusable so the assertion
    // helper can invoke `fn({})` more than once without the second
    // call falling off the end of the queue.
    const fetchImpl = vi
      .fn()
      .mockImplementationOnce(async () =>
        makeCatalogResponse({
          "risk.score": {
            name: "risk.score",
            params: {},
            returns: { kind: "scalar", type: "Long", from: "$.result.value" },
            origin: "sidecar",
          },
        }),
      )
      .mockImplementation(async () =>
        okResponse({
          request_id: "x",
          results: [
            {
              id: "call-1",
              ok: false,
              error: { code: "invalid_params", message: "missing wallet" },
            },
          ],
        }),
      );
    const entries = await loadSidecarEntries({
      sidecars: [RISK_SIDECAR],
      fetchImpl: fetchImpl as unknown as typeof fetch,
    });
    // Capture once, assert two facets — avoids invoking the forwarder
    // twice (each call burns a mock impl) and keeps the test cheap.
    const err = await entries[0].fn({}).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(RpcMethodError);
    expect((err as Error).message).toMatch(/missing wallet/);
  });
});
