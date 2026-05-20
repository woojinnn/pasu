import { describe, expect, it } from "vitest";
import { TokenBucketRateLimiter } from "../rate-limiter";

describe("TokenBucketRateLimiter", () => {
  it("allows requests up to the burst capacity", () => {
    const l = new TokenBucketRateLimiter({
      burst: 3,
      refillPerSec: 1,
      maxIps: 100,
      nowMs: () => 0,
    });
    expect(l.allow("ip")).toBe(true);
    expect(l.allow("ip")).toBe(true);
    expect(l.allow("ip")).toBe(true);
    expect(l.allow("ip")).toBe(false);
  });
  it("refills tokens over time", () => {
    let now = 0;
    const l = new TokenBucketRateLimiter({
      burst: 2,
      refillPerSec: 1,
      maxIps: 100,
      nowMs: () => now,
    });
    l.allow("ip");
    l.allow("ip");
    expect(l.allow("ip")).toBe(false);
    now = 1000;
    expect(l.allow("ip")).toBe(true);
    expect(l.allow("ip")).toBe(false);
  });
  it("tracks distinct IPs independently", () => {
    const l = new TokenBucketRateLimiter({
      burst: 1,
      refillPerSec: 1,
      maxIps: 100,
      nowMs: () => 0,
    });
    expect(l.allow("a")).toBe(true);
    expect(l.allow("a")).toBe(false);
    expect(l.allow("b")).toBe(true);
  });
  it("bounds the number of tracked IPs (LRU eviction)", () => {
    const l = new TokenBucketRateLimiter({
      burst: 1,
      refillPerSec: 1,
      maxIps: 2,
      nowMs: () => 0,
    });
    l.allow("a");
    l.allow("b");
    l.allow("c"); // evicts "a"
    expect(l.size()).toBe(2);
    expect(l.allow("a")).toBe(true); // fresh bucket
  });
});
