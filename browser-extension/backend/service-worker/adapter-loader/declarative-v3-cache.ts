/**
 * Layer 2 — persistent v3 adapter bundle cache (plan §M3 mirror).
 *
 * Mirrors {@link adapter-cache.ts}'s `PersistentAdapterCache` but stores
 * v3 ({@link V3Bundle}) hydrations keyed by the already-serialized
 * callkey string produced by `v3CallkeyCacheKey`. Layer 1
 * (`v3InstallCache` + WASM `DECLARATIVE_V3_STATE`) is in-memory and
 * disappears with the SW (~30s idle). Layer 2 persists each JIT-fetched
 * v3 bundle into `chrome.storage.local` so a cold SW can re-install
 * without another `registry-api-v3` round-trip.
 *
 * Storage shape (schemaVersion 2):
 *   - `bundles`: `bundleId → V3Bundle` (deduplicated body store)
 *   - `callkeys`: `callkey → { bundleId, decoderId, bundleSha256, fetchedAtMs }`
 *
 * The same `V3Bundle` reached via multiple callkeys (cartesian over
 * `chain_to_addresses`) is stored ONCE in `bundles` and referenced N times
 * from `callkeys`. The pre-v2 cache stored a full bundle copy under every
 * callkey, which blew past chrome.storage.local's 5MB quota for adapters
 * with many deployment addresses.
 *
 * Eviction: callkey-side insertion-order LRU + TTL (24h) + byte cap.
 * Evicting a callkey GC's its bundle iff no other callkey still references
 * the same `bundleId`. The byte cap is the upper bound (2MB) — well under
 * the 5MB chrome.storage.local quota — and the entry cap (256) is the
 * historical bound, both enforced after every put.
 */
import Browser from "webextension-polyfill";
import type { V3Bundle } from "./bundle-schema";

const STORAGE_KEY_V3 = "registry:adapter-bundles-v3";
const SCHEMA_VERSION = 2;
const MAX_CALLKEYS = 256;
const MAX_BYTES = 2 * 1024 * 1024;
const TTL_MS = 24 * 60 * 60 * 1000;

export interface DeclarativeV3CacheEntry {
  bundle: V3Bundle;
  bundleId: string;
  decoderId: string;
  bundleSha256: string;
  fetchedAtMs: number;
}

interface CallkeyMeta {
  bundleId: string;
  decoderId: string;
  bundleSha256: string;
  fetchedAtMs: number;
}

interface PersistedV3Cache {
  schemaVersion: number;
  bundles: Record<string, V3Bundle>;
  callkeys: Record<string, CallkeyMeta>;
}

function isCallkeyMeta(v: unknown): v is CallkeyMeta {
  if (!v || typeof v !== "object") return false;
  const o = v as Record<string, unknown>;
  return (
    typeof o.bundleId === "string" &&
    typeof o.decoderId === "string" &&
    typeof o.bundleSha256 === "string" &&
    typeof o.fetchedAtMs === "number"
  );
}

function isV3Bundle(v: unknown): v is V3Bundle {
  return !!v && typeof v === "object" && typeof (v as { id?: unknown }).id === "string";
}

function isPersistedV3Cache(v: unknown): v is PersistedV3Cache {
  if (!v || typeof v !== "object") return false;
  const o = v as Record<string, unknown>;
  return (
    o.schemaVersion === SCHEMA_VERSION &&
    typeof o.bundles === "object" && o.bundles !== null &&
    typeof o.callkeys === "object" && o.callkeys !== null
  );
}

class PersistentDeclarativeV3Cache {
  private readonly bundles = new Map<string, V3Bundle>();
  private readonly callkeys = new Map<string, CallkeyMeta>();
  /** Approximate stringified byte cost per bundle — refreshed on hydrate / put. */
  private readonly bundleBytes = new Map<string, number>();
  private totalBundleBytes = 0;
  private hydrated = false;

  private async hydrate(): Promise<void> {
    if (this.hydrated) return;
    try {
      const got = (await Browser.storage.local.get(
        STORAGE_KEY_V3,
      )) as Record<string, unknown>;
      const stored = got[STORAGE_KEY_V3];
      // Legacy fat-format payloads (pre-schemaVersion 2) stored a full bundle
      // copy under every callkey and could bloat past the 5MB quota. Drop
      // them on first hydrate so the next put() starts from a clean slate.
      if (!isPersistedV3Cache(stored)) {
        if (stored !== undefined) {
          try { await Browser.storage.local.set({ [STORAGE_KEY_V3]: this.emptyRecord() }); }
          catch { /* non-fatal */ }
        }
        this.hydrated = true;
        return;
      }
      const now = Date.now();
      for (const [bid, bundle] of Object.entries(stored.bundles)) {
        if (isV3Bundle(bundle)) {
          this.bundles.set(bid, bundle);
          const bytes = JSON.stringify(bundle).length;
          this.bundleBytes.set(bid, bytes);
          this.totalBundleBytes += bytes;
        }
      }
      for (const [callkey, meta] of Object.entries(stored.callkeys)) {
        if (
          isCallkeyMeta(meta) &&
          now - meta.fetchedAtMs < TTL_MS &&
          this.bundles.has(meta.bundleId)
        ) {
          this.callkeys.set(callkey, meta);
        }
      }
      // GC bundles that no surviving callkey references.
      this.gcUnreferencedBundles();
    } catch {
      /* degrade to fetch-every-time */
    }
    this.hydrated = true;
  }

  private emptyRecord(): PersistedV3Cache {
    return { schemaVersion: SCHEMA_VERSION, bundles: {}, callkeys: {} };
  }

  private async persist(): Promise<void> {
    try {
      const record: PersistedV3Cache = {
        schemaVersion: SCHEMA_VERSION,
        bundles: Object.fromEntries(this.bundles),
        callkeys: Object.fromEntries(this.callkeys),
      };
      await Browser.storage.local.set({ [STORAGE_KEY_V3]: record });
    } catch {
      /* non-fatal — in-memory copy still serves this lifetime */
    }
  }

  private gcUnreferencedBundles(): void {
    const referenced = new Set<string>();
    for (const meta of this.callkeys.values()) referenced.add(meta.bundleId);
    for (const bid of [...this.bundles.keys()]) {
      if (!referenced.has(bid)) {
        this.bundles.delete(bid);
        const bytes = this.bundleBytes.get(bid) ?? 0;
        this.totalBundleBytes -= bytes;
        this.bundleBytes.delete(bid);
      }
    }
  }

  /**
   * Evict the oldest callkey (Map insertion order = LRU) and GC its bundle
   * if it becomes unreferenced. Returns true iff an eviction happened.
   */
  private evictOneCallkey(): boolean {
    const oldest = this.callkeys.keys().next().value;
    if (oldest === undefined) return false;
    const meta = this.callkeys.get(oldest);
    this.callkeys.delete(oldest);
    if (meta) {
      let stillReferenced = false;
      for (const m of this.callkeys.values()) {
        if (m.bundleId === meta.bundleId) { stillReferenced = true; break; }
      }
      if (!stillReferenced) {
        this.bundles.delete(meta.bundleId);
        const bytes = this.bundleBytes.get(meta.bundleId) ?? 0;
        this.totalBundleBytes -= bytes;
        this.bundleBytes.delete(meta.bundleId);
      }
    }
    return true;
  }

  private enforceBounds(): void {
    while (this.callkeys.size > MAX_CALLKEYS) {
      if (!this.evictOneCallkey()) break;
    }
    while (this.totalBundleBytes > MAX_BYTES) {
      if (!this.evictOneCallkey()) break;
    }
  }

  /**
   * Look up a v3 cache entry by the already-serialized callkey string
   * (matches what `v3CallkeyCacheKey` produces). Returns `null` when
   * absent or TTL-expired; touches the callkey on hit to refresh its LRU
   * position.
   */
  async get(callkey: string): Promise<DeclarativeV3CacheEntry | null> {
    await this.hydrate();
    const meta = this.callkeys.get(callkey);
    if (!meta) return null;
    if (Date.now() - meta.fetchedAtMs >= TTL_MS) {
      this.callkeys.delete(callkey);
      this.gcUnreferencedBundles();
      void this.persist();
      return null;
    }
    const bundle = this.bundles.get(meta.bundleId);
    if (!bundle) {
      // Inconsistent state — drop the dangling callkey.
      this.callkeys.delete(callkey);
      void this.persist();
      return null;
    }
    this.callkeys.delete(callkey); // touch — LRU 갱신
    this.callkeys.set(callkey, meta);
    return {
      bundle,
      bundleId: meta.bundleId,
      decoderId: meta.decoderId,
      bundleSha256: meta.bundleSha256,
      fetchedAtMs: meta.fetchedAtMs,
    };
  }

  /**
   * Persist a v3 cache entry under `callkey`. The bundle body is stored
   * once per `bundleId` and shared across all callkeys that reach the
   * same bundle, so 8 callkeys for one Universal Router deployment hold
   * one bundle copy + 8 metas (~150B each) instead of 8 full bundles.
   *
   * Evicts the oldest callkey(s) once `MAX_CALLKEYS` or `MAX_BYTES` is
   * exceeded, GC'ing any bundle that loses its last referencing callkey.
   */
  async put(callkey: string, entry: DeclarativeV3CacheEntry): Promise<void> {
    await this.hydrate();
    const meta: CallkeyMeta = {
      bundleId: entry.bundleId,
      decoderId: entry.decoderId,
      bundleSha256: entry.bundleSha256,
      fetchedAtMs: entry.fetchedAtMs,
    };
    if (!this.bundles.has(entry.bundleId)) {
      this.bundles.set(entry.bundleId, entry.bundle);
      const bytes = JSON.stringify(entry.bundle).length;
      this.bundleBytes.set(entry.bundleId, bytes);
      this.totalBundleBytes += bytes;
    }
    this.callkeys.delete(callkey);
    this.callkeys.set(callkey, meta);
    this.enforceBounds();
    await this.persist();
  }

  /**
   * Debug / inspection helper — returns a shallow copy of the current
   * in-memory callkey→entry map (after hydrating from storage). Callers
   * MUST treat the returned map as read-only.
   */
  async getAllEntries(): Promise<Map<string, DeclarativeV3CacheEntry>> {
    await this.hydrate();
    const out = new Map<string, DeclarativeV3CacheEntry>();
    for (const [callkey, meta] of this.callkeys) {
      const bundle = this.bundles.get(meta.bundleId);
      if (!bundle) continue;
      out.set(callkey, {
        bundle,
        bundleId: meta.bundleId,
        decoderId: meta.decoderId,
        bundleSha256: meta.bundleSha256,
        fetchedAtMs: meta.fetchedAtMs,
      });
    }
    return out;
  }

  reset(): void {
    this.bundles.clear();
    this.callkeys.clear();
    this.bundleBytes.clear();
    this.totalBundleBytes = 0;
    this.hydrated = false;
  }
}

export const declarativeV3Cache = new PersistentDeclarativeV3Cache();

/**
 * Test helper — drops the in-memory maps and clears the hydration
 * latch. Mirrors `__resetAdapterCacheForTest` so vitest cases start
 * from a cold slate. Does NOT touch `chrome.storage.local` itself;
 * the caller must clear the mock store separately when needed.
 */
export function __resetDeclarativeV3CacheStorageForTest(): void {
  declarativeV3Cache.reset();
}
