import { describe, expect, it } from "vitest";
import { ObjectCache } from "../cache";

const value = (s: string) => ({
  status: 200 as const,
  body: Buffer.from(s),
  contentType: "application/json",
});

describe("ObjectCache", () => {
  it("returns a stored value before its TTL expires", () => {
    let now = 1000;
    const c = new ObjectCache({
      maxEntries: 8,
      ttlMs: 100,
      negativeTtlMs: 100,
      nowMs: () => now,
    });
    c.set("/a", value("hello"));
    now = 1050;
    expect(c.get("/a")?.body.toString()).toBe("hello");
  });
  it("expires a value after its TTL", () => {
    let now = 1000;
    const c = new ObjectCache({
      maxEntries: 8,
      ttlMs: 100,
      negativeTtlMs: 100,
      nowMs: () => now,
    });
    c.set("/a", value("hello"));
    now = 1200;
    expect(c.get("/a")).toBeUndefined();
  });
  it("uses the negative TTL for a cached 404", () => {
    let now = 1000;
    const c = new ObjectCache({
      maxEntries: 8,
      ttlMs: 10000,
      negativeTtlMs: 100,
      nowMs: () => now,
    });
    c.set("/m", { status: 404 });
    now = 1050;
    expect(c.get("/m")?.status).toBe(404);
    now = 1200;
    expect(c.get("/m")).toBeUndefined();
  });
  it("evicts the least-recently-used entry past capacity", () => {
    const c = new ObjectCache({
      maxEntries: 2,
      ttlMs: 10000,
      negativeTtlMs: 10000,
      nowMs: () => 0,
    });
    c.set("/a", value("a"));
    c.set("/b", value("b"));
    c.get("/a");
    c.set("/c", value("c"));
    expect(c.get("/b")).toBeUndefined();
    expect(c.get("/a")?.body.toString()).toBe("a");
  });
  it("reports hit / miss stats", () => {
    const c = new ObjectCache({
      maxEntries: 8,
      ttlMs: 10000,
      negativeTtlMs: 10000,
      nowMs: () => 0,
    });
    c.set("/a", value("a"));
    c.get("/a");
    c.get("/b");
    expect(c.stats()).toEqual({ hits: 1, misses: 1, size: 1 });
  });
});
