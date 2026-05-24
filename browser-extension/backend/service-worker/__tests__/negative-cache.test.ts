/**
 * Phase 2B — Negative-cache cases.
 *
 * Verifies the three spec-mandated TTL bands and the lazy-expiry policy.
 * Uses `vi.useFakeTimers` so we can advance time deterministically
 * instead of `setTimeout(0)`-ing the suite.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  __resetNegativeCacheForTest,
  negativeCache,
  serializeKey,
} from "../adapter-loader/negative-cache";
import type { CallMatchKey } from "../registry/client";

const KEY: CallMatchKey = {
  chain_id: 1,
  to: "0xAaAa000000000000000000000000000000000001",
  selector: "0xDeAdBeEf",
};

const KEY_OTHER: CallMatchKey = {
  chain_id: 1,
  to: "0xAaAa000000000000000000000000000000000002",
  selector: "0xDeAdBeEf",
};

describe("serializeKey", () => {
  it("lowercases to/selector for stable cache hits across casings", () => {
    expect(serializeKey(KEY)).toBe(
      "1__0xaaaa000000000000000000000000000000000001__0xdeadbeef",
    );
  });

  it("treats different chain_ids as different keys", () => {
    expect(serializeKey({ ...KEY, chain_id: 8453 })).not.toBe(serializeKey(KEY));
  });
});

describe("negativeCache", () => {
  beforeEach(() => {
    __resetNegativeCacheForTest();
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 0, 1));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns null for an unseen key", () => {
    expect(negativeCache.get(KEY)).toBeNull();
  });

  it("round-trips add → get with the reason and a future expiry", () => {
    negativeCache.add(KEY, 300, "no_publisher");
    const entry = negativeCache.get(KEY);
    expect(entry).not.toBeNull();
    expect(entry?.reason).toBe("no_publisher");
    expect(entry?.expiresAt).toBe(Date.now() + 300_000);
  });

  it("lazily expires entries on get (5-min reason after 5 minutes)", () => {
    negativeCache.add(KEY, 300, "no_publisher");
    vi.advanceTimersByTime(299_000);
    expect(negativeCache.get(KEY)?.reason).toBe("no_publisher");

    vi.advanceTimersByTime(2_000); // now 301s in → expired
    expect(negativeCache.get(KEY)).toBeNull();
    // Sweep effect — size drops after the get.
    expect(negativeCache.size()).toBe(0);
  });

  it("30s `timeout` reason expires after 30s", () => {
    negativeCache.add(KEY, 30, "timeout");
    vi.advanceTimersByTime(29_000);
    expect(negativeCache.get(KEY)?.reason).toBe("timeout");

    vi.advanceTimersByTime(2_000);
    expect(negativeCache.get(KEY)).toBeNull();
  });

  it("5-min `integrity_failed` reason expires after 300s", () => {
    negativeCache.add(KEY, 300, "integrity_failed");
    vi.advanceTimersByTime(299_000);
    expect(negativeCache.get(KEY)?.reason).toBe("integrity_failed");
    vi.advanceTimersByTime(2_000);
    expect(negativeCache.get(KEY)).toBeNull();
  });

  it("re-adding a key overwrites the prior reason and TTL", () => {
    negativeCache.add(KEY, 30, "timeout");
    vi.advanceTimersByTime(20_000);
    // Re-add with a longer TTL + different reason.
    negativeCache.add(KEY, 300, "integrity_failed");

    vi.advanceTimersByTime(40_000); // 60s after first add — would have expired
    const entry = negativeCache.get(KEY);
    expect(entry?.reason).toBe("integrity_failed");
  });

  it("isolates entries by key", () => {
    negativeCache.add(KEY, 30, "timeout");
    negativeCache.add(KEY_OTHER, 300, "no_publisher");
    expect(negativeCache.get(KEY)?.reason).toBe("timeout");
    expect(negativeCache.get(KEY_OTHER)?.reason).toBe("no_publisher");
  });

  it("clear() wipes all entries", () => {
    negativeCache.add(KEY, 300, "no_publisher");
    negativeCache.add(KEY_OTHER, 30, "timeout");
    negativeCache.clear();
    expect(negativeCache.size()).toBe(0);
    expect(negativeCache.get(KEY)).toBeNull();
  });
});
