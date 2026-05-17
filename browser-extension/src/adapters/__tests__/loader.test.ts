import { describe, expect, it, vi } from "vitest";
import { Loader } from "../loader";
import { RegistryClient } from "../registry-client";
import { AdapterCache, type CacheBackend } from "../cache";

class MemBackend implements CacheBackend {
  store = new Map<string, string>();
  async get(k: string) {
    return this.store.get(k);
  }
  async set(k: string, v: string) {
    this.store.set(k, v);
  }
  async delete(k: string) {
    this.store.delete(k);
  }
  async keys() {
    return Array.from(this.store.keys());
  }
}

describe("Loader", () => {
  it("returns null when registry has no entry", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response("", { status: 404 }));
    const reg = new RegistryClient("http://r", fetchMock);
    const cache = new AdapterCache(new MemBackend(), {
      capacity: 32,
      ttlMs: 60_000,
    });
    const loader = new Loader({ registry: reg, cache });
    expect(await loader.load(1, "0xabcd")).toBeNull();
  });

  it("dedupes inflight requests for same key", async () => {
    let calls = 0;
    const fetchMock = vi.fn().mockImplementation(async () => {
      calls += 1;
      // Simulate slow 404 so two concurrent calls observe the inflight state
      await new Promise((r) => setTimeout(r, 5));
      return new Response("", { status: 404 });
    });
    const reg = new RegistryClient("http://r", fetchMock);
    const cache = new AdapterCache(new MemBackend(), {
      capacity: 32,
      ttlMs: 60_000,
    });
    const loader = new Loader({ registry: reg, cache });
    const a = loader.load(1, "0xabcd");
    const b = loader.load(1, "0xabcd");
    await Promise.all([a, b]);
    expect(calls).toBe(1);
  });
});
