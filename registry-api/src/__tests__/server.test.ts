import type { AddressInfo } from "node:net";
import { afterEach, describe, expect, it } from "vitest";
import { createRegistryApiServer } from "../server";
import type { ObjectReader, ObjectResult } from "../gcs-client";

const CALLKEY_PATH =
  "/index/by-callkey/1__0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2__0x38ed1739.json";
const TOKEN_PATH = "/tokens/1/0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2.json";
const TYPED_DATA_PATH =
  "/index/by-typed-data/1__0x000000000022d473030f116ddee9f6b43ac78ba3__PermitSingle.json";

function fakeReader(
  objects: Record<string, string>,
): ObjectReader & { reads: string[] } {
  const reads: string[] = [];
  return {
    reads,
    async read(name: string): Promise<ObjectResult> {
      reads.push(name);
      const body = objects[name];
      if (body === undefined) return { kind: "not_found" };
      return {
        kind: "found",
        body: Buffer.from(body),
        contentType: "application/json; charset=utf-8",
      };
    },
  };
}

const baseConfig = {
  host: "127.0.0.1",
  port: 0,
  bucketName: "test-bucket",
  cacheMaxEntries: 64,
  cacheTtlMs: 10000,
  cacheNegativeTtlMs: 10000,
  cacheControlValue: "public, max-age=300",
  rateLimitBurst: 1000,
  rateLimitRefillPerSec: 1000,
  rateLimitMaxIps: 1000,
  trustedProxyHops: 0,
};

describe("registry-api HTTP server", () => {
  const started: ReturnType<typeof createRegistryApiServer>[] = [];

  async function start(reader: ObjectReader, config = baseConfig) {
    const s = createRegistryApiServer({ reader, config });
    await new Promise<void>((r) => s.listen(0, "127.0.0.1", r));
    started.push(s);
    return `http://127.0.0.1:${(s.address() as AddressInfo).port}`;
  }

  afterEach(async () => {
    await Promise.all(
      started
        .splice(0)
        .map((s) => new Promise<void>((res) => s.close(() => res()))),
    );
  });

  it("returns health status", async () => {
    const url = await start(fakeReader({}));
    expect((await fetch(`${url}/health`)).status).toBe(200);
  });

  it("proxies a callkey object (200 + headers)", async () => {
    const url = await start(
      fakeReader({
        "index/by-callkey/1__0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2__0x38ed1739.json":
          '{"matched":true}',
      }),
    );
    const res = await fetch(`${url}${CALLKEY_PATH}`);
    expect(res.status).toBe(200);
    expect(res.headers.get("cache-control")).toBe("public, max-age=300");
    expect(res.headers.get("access-control-allow-origin")).toBe("*");
  });

  it("proxies a by-selector object (address-agnostic adapter, 200)", async () => {
    const url = await start(
      fakeReader({
        "index/by-selector/1__0xa22cb465.json": '{"matched":true}',
      }),
    );
    const res = await fetch(`${url}/index/by-selector/1__0xa22cb465.json`);
    expect(res.status).toBe(200);
    expect(res.headers.get("cache-control")).toBe("public, max-age=300");
    expect(res.headers.get("access-control-allow-origin")).toBe("*");
  });

  it("rejects a callkey-shaped by-selector key with 404 (no address segment)", async () => {
    const url = await start(fakeReader({}));
    // 3-part <chain>__<addr>__<selector> must NOT validate as a 2-part selector key
    const res = await fetch(
      `${url}/index/by-selector/1__0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2__0xa22cb465.json`,
    );
    expect(res.status).toBe(404);
  });

  it("materializes a ref callkey entry into the legacy full bundle response", async () => {
    const bundleRef =
      "bundles/0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.json";
    const contextRef =
      "contexts/curve/factory_stable_ng_2coin_mainnet/1/0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2.json";
    const url = await start(
      fakeReader({
        "index/by-callkey/1__0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2__0x38ed1739.json":
          JSON.stringify({
            matched: true,
            schema_version: "3-ref",
            bundle_id:
              "curve/stableswap-ng/source/exchange/1-test-33333333@1.0.0",
            manifest_path: "manifests/curve/exchange.json",
            bundle_sha256: "0x" + "b".repeat(64),
            bundle_ref: bundleRef,
            context_ref: contextRef,
          }),
        [bundleRef]: JSON.stringify({
          type: "adapter_action",
          id: "curve/stableswap-ng/source/exchange@1.0.0",
          schema_version: "3",
          match: {
            selector: "0x38ed1739",
            chain_to_addresses_source: "curve:factory_stable_ng_2coin_mainnet",
            chain_ids: [1],
          },
          source_materialize: { kind: "per_address_context" },
          abi_fragment: {
            function_name: "exchange",
            abi: { type: "function", name: "exchange", inputs: [] },
          },
          emit: {
            strategy: "single_emit",
            body: {
              token_in: "$source.coins.0",
            },
          },
        }),
        [contextRef]: JSON.stringify({
          schema_version: "3-source-context",
          source: "curve:factory_stable_ng_2coin_mainnet",
          chain_id: 1,
          address: "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
          context: {
            id_suffix: "1-test-33333333",
            coins: ["0x1111111111111111111111111111111111111111"],
          },
        }),
      }),
    );

    const res = await fetch(`${url}${CALLKEY_PATH}`);
    expect(res.status).toBe(200);
    const body = (await res.json()) as {
      matched: boolean;
      bundle_id: string;
      bundle: {
        id: string;
        source_materialize?: unknown;
        match: { chain_to_addresses: Record<string, string[]> };
        emit: { body: { token_in: string } };
      };
    };
    expect(body.matched).toBe(true);
    expect(body.bundle_id).toBe(
      "curve/stableswap-ng/source/exchange/1-test-33333333@1.0.0",
    );
    expect(body.bundle.id).toBe(body.bundle_id);
    expect(body.bundle.source_materialize).toBeUndefined();
    expect(body.bundle.match.chain_to_addresses).toEqual({
      "1": ["0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"],
    });
    expect(body.bundle.emit.body.token_in).toBe(
      "0x1111111111111111111111111111111111111111",
    );
  });

  it("fetches a shared bundle template only once across sibling callkeys (sub-read cache)", async () => {
    const bundleRef =
      "bundles/0xcccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc.json";
    const refEntry = (addr: string) =>
      JSON.stringify({
        matched: true,
        schema_version: "3-ref",
        bundle_id: `curve/pool/exchange/${addr}@1.0.0`,
        manifest_path: "manifests/curve/exchange.json",
        bundle_sha256: "0x" + "c".repeat(64),
        bundle_ref: bundleRef,
      });
    const KEY1 =
      "/index/by-callkey/1__0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2__0x38ed1739.json";
    const KEY2 =
      "/index/by-callkey/1__0x1111111111111111111111111111111111111111__0x38ed1739.json";
    const reader = fakeReader({
      "index/by-callkey/1__0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2__0x38ed1739.json":
        refEntry("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
      "index/by-callkey/1__0x1111111111111111111111111111111111111111__0x38ed1739.json":
        refEntry("0x1111111111111111111111111111111111111111"),
      [bundleRef]: JSON.stringify({ id: "shared", emit: { strategy: "single_emit" } }),
    });
    const url = await start(reader);
    expect((await fetch(`${url}${KEY1}`)).status).toBe(200);
    expect((await fetch(`${url}${KEY2}`)).status).toBe(200);
    // Both callkeys resolve the same bundle template, but it is fetched once.
    expect(reader.reads.filter((r) => r === bundleRef)).toHaveLength(1);
  });

  it("passes a GCS miss through as a real HTTP 404", async () => {
    const url = await start(fakeReader({}));
    expect((await fetch(`${url}${CALLKEY_PATH}`)).status).toBe(404);
  });

  it("serves a repeated request from the cache (one GCS read)", async () => {
    const reader = fakeReader({
      "index/by-callkey/1__0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2__0x38ed1739.json":
        "{}",
    });
    const url = await start(reader);
    await fetch(`${url}${CALLKEY_PATH}`);
    await fetch(`${url}${CALLKEY_PATH}`);
    expect(reader.reads).toHaveLength(1);
  });

  it("rejects path traversal with 404 and no GCS read", async () => {
    const reader = fakeReader({});
    const url = await start(reader);
    const res = await fetch(
      `${url}/index/by-callkey/..%2f..%2fmanifests%2fx.json`,
    );
    expect(res.status).toBe(404);
    expect(reader.reads).toHaveLength(0);
  });

  it("rejects a non-GET with 405", async () => {
    const url = await start(fakeReader({}));
    expect(
      (await fetch(`${url}${CALLKEY_PATH}`, { method: "POST" })).status,
    ).toBe(405);
  });

  it("answers a CORS preflight with 204", async () => {
    const url = await start(fakeReader({}));
    const res = await fetch(`${url}${CALLKEY_PATH}`, { method: "OPTIONS" });
    expect(res.status).toBe(204);
    expect(res.headers.get("access-control-allow-methods")).toContain("GET");
  });

  it("returns 502 on a GCS upstream error", async () => {
    const url = await start({
      async read() {
        return { kind: "upstream_error", message: "boom" };
      },
    });
    expect((await fetch(`${url}${CALLKEY_PATH}`)).status).toBe(502);
  });

  it("returns 429 when the per-IP rate limit is exceeded", async () => {
    const url = await start(
      fakeReader({
        "index/by-callkey/1__0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2__0x38ed1739.json":
          "{}",
      }),
      { ...baseConfig, rateLimitBurst: 2, rateLimitRefillPerSec: 0 },
    );
    const codes = [
      (await fetch(`${url}${CALLKEY_PATH}`)).status,
      (await fetch(`${url}${CALLKEY_PATH}`)).status,
      (await fetch(`${url}${CALLKEY_PATH}`)).status,
    ];
    expect(codes).toEqual([200, 200, 429]);
  });

  it("keys the rate limit on the trusted rightmost XFF hop, not a spoofable leftmost (A1)", async () => {
    // Simulates Cloud Run: the genuine client IP (8.8.8.8) is appended by the
    // trusted edge as the rightmost entry; the attacker rotates the leftmost
    // client-supplied value every request hoping for a fresh bucket. With the
    // old leftmost logic each request would mint a new bucket and never 429.
    const url = await start(
      fakeReader({
        "index/by-callkey/1__0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2__0x38ed1739.json":
          "{}",
      }),
      { ...baseConfig, rateLimitBurst: 2, rateLimitRefillPerSec: 0 },
    );
    const codes: number[] = [];
    for (let i = 0; i < 3; i += 1) {
      const res = await fetch(`${url}${CALLKEY_PATH}`, {
        headers: { "x-forwarded-for": `10.0.0.${i}, 8.8.8.8` },
      });
      codes.push(res.status);
    }
    expect(codes).toEqual([200, 200, 429]);
  });

  it("records recent requests for /debug/recent", async () => {
    const url = await start(
      fakeReader({
        "index/by-callkey/1__0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2__0x38ed1739.json":
          "{}",
      }),
    );
    await fetch(`${url}${CALLKEY_PATH}`);
    const body = (await (await fetch(`${url}/debug/recent`)).json()) as {
      entries: unknown[];
    };
    expect(body.entries.length).toBeGreaterThanOrEqual(1);
  });

  it("proxies a token object", async () => {
    const url = await start(
      fakeReader({
        "tokens/1/0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2.json":
          '{"kind":"erc20"}',
      }),
    );
    expect((await fetch(`${url}${TOKEN_PATH}`)).status).toBe(200);
  });

  it("proxies a typed-data object (200)", async () => {
    const url = await start(
      fakeReader({
        "index/by-typed-data/1__0x000000000022d473030f116ddee9f6b43ac78ba3__PermitSingle.json":
          '{"matched":true}',
      }),
    );
    expect((await fetch(`${url}${TYPED_DATA_PATH}`)).status).toBe(200);
  });

  it("passes a typed-data GCS miss through as a real HTTP 404", async () => {
    const url = await start(fakeReader({}));
    expect((await fetch(`${url}${TYPED_DATA_PATH}`)).status).toBe(404);
  });

  it("rejects a malformed typed-data path with 404 and no GCS read", async () => {
    const reader = fakeReader({});
    const url = await start(reader);
    const res = await fetch(`${url}/index/by-typed-data/0x1__bad__Type.json`);
    expect(res.status).toBe(404);
    expect(reader.reads).toHaveLength(0);
  });
});
