/**
 * Phase 2B — In-memory negative cache.
 *
 * Spec: `ADAPTER_LOADER_ARCHITECTURE.md` §7.3:824-866 and §7.4 row
 * "JIT retry 정책".
 *
 * Why memoise misses? Without a negative cache, every call to a known-bad
 * selector triggers a fresh 2-second JIT round-trip. The cache pins:
 *   - `no_publisher`    — registry 404. Permanent absence; 5-min TTL.
 *   - `integrity_failed`— registry served the bundle but hash mismatched
 *                          (publisher impersonation / CDN tamper). 5-min
 *                          TTL — treat as suspicious, not permanent.
 *   - `timeout`         — fetch / network / non-404 5xx. Self-healing;
 *                          30-second TTL so the next call auto-retries.
 *
 * Phase 2B uses an in-memory `Map` only. The IndexedDB-backed persistent
 * variant (with HMAC-SHA256 key obfuscation per §7.5) lands in a later
 * phase — the interface here is shaped so the IndexedDB swap-in stays
 * a one-file change.
 *
 * Key format: `${chain_id}__${to.toLowerCase()}__${selector.toLowerCase()}`
 * — identical to the registry callkey URL (§6.2) and `build-index.ts`'s
 * filename convention, so the same string serves cache + URL lookups.
 */

import type { CallMatchKey } from "../registry/client";

export type NegativeReason =
  | "no_publisher"
  | "integrity_failed"
  | "timeout";

export interface NegativeCacheEntry {
  reason: NegativeReason;
  /** ms since epoch — entry is dead once `Date.now() >= expiresAt`. */
  expiresAt: number;
}

/**
 * Serialise a CallMatchKey to the canonical cache/URL key. Used by both
 * the negative cache and the jit-fetcher's `inflight` Map so a single
 * helper handles both.
 */
export function serializeKey(key: CallMatchKey): string {
  return `${key.chain_id}__${key.to.toLowerCase()}__${key.selector.toLowerCase()}`;
}

export interface NegativeCache {
  add(key: CallMatchKey, ttlSec: number, reason: NegativeReason): void;
  get(key: CallMatchKey): NegativeCacheEntry | null;
  /** Test helper — wipe all entries. */
  clear(): void;
  /** Test helper — count live entries (post-expiry sweep). */
  size(): number;
}

class InMemoryNegativeCache implements NegativeCache {
  private readonly entries = new Map<string, NegativeCacheEntry>();

  add(key: CallMatchKey, ttlSec: number, reason: NegativeReason): void {
    const k = serializeKey(key);
    this.entries.set(k, {
      reason,
      expiresAt: Date.now() + ttlSec * 1000,
    });
  }

  /**
   * Lazy expiry: we do not run a background timer (Service Worker can be
   * killed at any moment). Instead, every `get` checks `expiresAt` and
   * evicts an expired entry inline. This keeps the cache zero-cost when
   * idle and avoids leaking the `Map` after SW termination.
   */
  get(key: CallMatchKey): NegativeCacheEntry | null {
    const k = serializeKey(key);
    const entry = this.entries.get(k);
    if (!entry) return null;
    if (Date.now() >= entry.expiresAt) {
      this.entries.delete(k);
      return null;
    }
    return entry;
  }

  clear(): void {
    this.entries.clear();
  }

  size(): number {
    // Sweep expired entries opportunistically so tests see an accurate count.
    const now = Date.now();
    for (const [k, e] of this.entries.entries()) {
      if (now >= e.expiresAt) this.entries.delete(k);
    }
    return this.entries.size;
  }
}

/**
 * Process-singleton instance. The Service Worker boots a single
 * adapter-loader stack, so a module-level singleton is exactly the right
 * granularity. Tests can call `.clear()` to reset between cases — the
 * `__resetNegativeCacheForTest` helper formalises that.
 */
export const negativeCache: NegativeCache = new InMemoryNegativeCache();

/**
 * Test helper — vitest cases can call this in `beforeEach` to start from a
 * clean slate without having to import the underlying class.
 */
export function __resetNegativeCacheForTest(): void {
  negativeCache.clear();
}
