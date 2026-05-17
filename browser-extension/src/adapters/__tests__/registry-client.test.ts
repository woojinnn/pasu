import { describe, expect, it, vi } from "vitest";
import { RegistryClient, type ChainResolution } from "../registry-client";

describe("RegistryClient", () => {
  it("resolves (chainId, address) via /chains endpoint", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          manifest: { name: "erc20-transfer", version: "0.1.0", sdk_version: 1, description: "x", capabilities: ["decoder"], applies_to: [], factory_of: [], proxy_of: [] },
          wasm_url: "/packages/erc20-transfer/v0.1.0/adapter.wasm",
          version: "0.1.0",
          sdk_version: 1,
        }),
        { status: 200, headers: { "content-type": "application/json" } }
      )
    );
    const client = new RegistryClient("http://r", fetchMock);
    const res = (await client.resolve(1, "0xabcd")) as ChainResolution;
    expect(res.version).toBe("0.1.0");
    expect(fetchMock).toHaveBeenCalledWith("http://r/chains/1/0xabcd");
  });

  it("returns null on 404", async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response("", { status: 404 }));
    const client = new RegistryClient("http://r", fetchMock);
    expect(await client.resolve(1, "0xabcd")).toBeNull();
  });

  it("fetches WASM bytes", async () => {
    const wasm = new Uint8Array([0, 0x61, 0x73, 0x6d, 1, 0, 0, 0]);
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(wasm, { status: 200, headers: { "content-type": "application/wasm" } })
    );
    const client = new RegistryClient("http://r", fetchMock);
    const bytes = await client.fetchWasm("/packages/x/v0.0.1/adapter.wasm");
    expect(bytes).toEqual(wasm);
  });
});
