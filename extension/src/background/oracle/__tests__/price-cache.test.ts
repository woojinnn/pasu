import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  __test_internals__,
  cachedPriceLastUpdatedAt,
  clearAll,
  lookup,
  store,
} from "../price-cache";

const storage = new Map<string, unknown>();

function installChromeStorageMock(): void {
  Object.defineProperty(globalThis, "chrome", {
    configurable: true,
    value: {
      storage: {
        local: {
          get: vi.fn(async (keys: string | string[]) => {
            const out: Record<string, unknown> = {};
            for (const key of Array.isArray(keys) ? keys : [keys]) {
              out[key] = storage.get(key);
            }
            return out;
          }),
          set: vi.fn(async (entries: Record<string, unknown>) => {
            for (const [key, value] of Object.entries(entries))
              storage.set(key, value);
          }),
          remove: vi.fn(async (keys: string | string[]) => {
            for (const key of Array.isArray(keys) ? keys : [keys])
              storage.delete(key);
          }),
        },
      },
    },
  });
}

describe("price-cache", () => {
  beforeEach(async () => {
    storage.clear();
    installChromeStorageMock();
    await clearAll(1, ["0xabc", "0xdef"]);
  });

  it("lookup misses empty cache using lowercased addresses", async () => {
    const result = await lookup(1, ["0xABC"], 1000);
    expect(result.hits).toEqual(new Map());
    expect(result.misses).toEqual(["0xabc"]);
  });

  it("store and lookup round-trip numbers with per-price storage keys", async () => {
    await store(1, new Map([["0xABC", 3500]]), 1000);
    const result = await lookup(1, ["0xabc"], 1000);
    expect(result.hits.get("0xabc")).toBe(3500);
    expect(storage.has("price:1:0xabc")).toBe(true);
  });

  it("expires entries past the 60 second TTL", async () => {
    await store(1, new Map([["0xabc", 1]]), 0);
    const result = await lookup(1, ["0xabc"], __test_internals__.TTL_MS + 1);
    expect(result.hits.size).toBe(0);
    expect(result.misses).toEqual(["0xabc"]);
  });

  it("threads last_updated_at metadata through cache hits", async () => {
    await store(
      1,
      new Map([["0xabc", 1]]),
      10_000,
      new Map([["0xabc", 7_000]]),
    );
    const result = await lookup(1, ["0xabc"], 10_000);
    expect(cachedPriceLastUpdatedAt(result.hits, "0xABC")).toBe(7_000);
  });
});
