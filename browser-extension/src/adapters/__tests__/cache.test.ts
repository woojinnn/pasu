import { describe, expect, it } from "vitest";
import { AdapterCache, type CacheBackend, type CacheEntry } from "../cache";

class MemoryBackend implements CacheBackend {
  store = new Map<string, string>();
  async get(key: string) { return this.store.get(key); }
  async set(key: string, value: string) { this.store.set(key, value); }
  async delete(key: string) { this.store.delete(key); }
  async keys() { return Array.from(this.store.keys()); }
}

describe("AdapterCache", () => {
  it("stores and retrieves a resolution", async () => {
    const cache = new AdapterCache(new MemoryBackend(), { capacity: 4, ttlMs: 60_000 });
    await cache.put(1, "0xabcd", {
      version: "0.1.0",
      manifest: {} as any,
      wasm: new Uint8Array([1, 2, 3]),
      fetchedAt: Date.now(),
    });
    const got = await cache.get(1, "0xabcd");
    expect(got?.version).toBe("0.1.0");
    expect(got?.wasm).toEqual(new Uint8Array([1, 2, 3]));
  });

  it("misses after TTL expiry", async () => {
    const cache = new AdapterCache(new MemoryBackend(), { capacity: 4, ttlMs: 1 });
    await cache.put(1, "0xabcd", {
      version: "0.1.0",
      manifest: {} as any,
      wasm: new Uint8Array(),
      fetchedAt: Date.now() - 1_000,
    });
    expect(await cache.get(1, "0xabcd")).toBeNull();
  });

  it("evicts LRU when over capacity", async () => {
    const cache = new AdapterCache(new MemoryBackend(), { capacity: 2, ttlMs: 60_000 });
    const mkEntry = (version: string): CacheEntry => ({
      version,
      manifest: {} as any,
      wasm: new Uint8Array(),
      fetchedAt: Date.now(),
    });
    await cache.put(1, "0xaa", mkEntry("a"));
    await cache.put(1, "0xbb", mkEntry("b"));
    await cache.get(1, "0xaa");
    await cache.put(1, "0xcc", mkEntry("c"));
    expect(await cache.get(1, "0xbb")).toBeNull();
    expect((await cache.get(1, "0xaa"))?.version).toBe("a");
    expect((await cache.get(1, "0xcc"))?.version).toBe("c");
  });
});
