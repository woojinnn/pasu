/**
 * Layer 2 — persistent adapter bundle cache (ADAPTER_LOADER §7).
 *
 * Layer 1 (mountedByCallKey + WASM DECLARATIVE_STATE) 은 in-memory 라 SW
 * 종료(~30s idle) 시 소멸. Layer 2 는 JIT fetch 한 bundle 을
 * chrome.storage.local 에 영속화해 cold SW 가 GCS 재fetch 없이 재사용.
 *
 * Eviction: insertion-order LRU (Map order, token-client.ts 와 동일) cap +
 * TTL (registry 대비 cheap freshness check 불가 → stale bundle 재fetch 유도).
 */
import Browser from "webextension-polyfill";
import type { AdapterFunctionBundle } from "./bundle-schema";
import { serializeKey } from "./negative-cache";
import type { CallMatchKey } from "../registry/client";

const STORAGE_KEY = "registry:adapter-bundles";
const MAX_ADAPTER_CACHE_ENTRIES = 256;
const TTL_MS = 24 * 60 * 60 * 1000;

export interface AdapterCacheEntry {
  bundle: AdapterFunctionBundle;
  bundle_sha256: string;
  fetchedAtMs: number;
}

function isAdapterCacheEntry(v: unknown): v is AdapterCacheEntry {
  if (!v || typeof v !== "object") return false;
  const o = v as Record<string, unknown>;
  return (
    typeof o.bundle === "object" && o.bundle !== null &&
    typeof o.bundle_sha256 === "string" &&
    typeof o.fetchedAtMs === "number"
  );
}

class PersistentAdapterCache {
  private readonly mem = new Map<string, AdapterCacheEntry>();
  private hydrated = false;

  private async hydrate(): Promise<void> {
    if (this.hydrated) return;
    try {
      const got = (await Browser.storage.local.get(STORAGE_KEY)) as Record<string, unknown>;
      const stored = got[STORAGE_KEY] as Record<string, AdapterCacheEntry> | undefined;
      if (stored) {
        const now = Date.now();
        for (const [k, v] of Object.entries(stored)) {
          if (isAdapterCacheEntry(v) && now - v.fetchedAtMs < TTL_MS) this.mem.set(k, v);
        }
      }
    } catch { /* degrade to fetch-every-time */ }
    this.hydrated = true;
  }

  private async persist(): Promise<void> {
    try {
      const record: Record<string, AdapterCacheEntry> = {};
      for (const [k, v] of this.mem) record[k] = v;
      await Browser.storage.local.set({ [STORAGE_KEY]: record });
    } catch { /* non-fatal — in-memory copy still serves this lifetime */ }
  }

  async get(key: CallMatchKey): Promise<AdapterCacheEntry | null> {
    await this.hydrate();
    const k = serializeKey(key);
    const entry = this.mem.get(k);
    if (!entry) return null;
    if (Date.now() - entry.fetchedAtMs >= TTL_MS) {
      this.mem.delete(k);
      void this.persist();
      return null;
    }
    this.mem.delete(k);          // touch — LRU 순서 갱신
    this.mem.set(k, entry);
    return entry;
  }

  async put(key: CallMatchKey, bundle: AdapterFunctionBundle, bundle_sha256: string): Promise<void> {
    await this.hydrate();
    const k = serializeKey(key);
    this.mem.delete(k);
    this.mem.set(k, { bundle, bundle_sha256, fetchedAtMs: Date.now() });
    while (this.mem.size > MAX_ADAPTER_CACHE_ENTRIES) {
      const oldest = this.mem.keys().next().value;
      if (oldest === undefined) break;
      this.mem.delete(oldest);
    }
    await this.persist();
  }

  async delete(key: CallMatchKey): Promise<void> {
    await this.hydrate();
    if (this.mem.delete(serializeKey(key))) await this.persist();
  }

  reset(): void { this.mem.clear(); this.hydrated = false; }
}

export const adapterCache = new PersistentAdapterCache();
export function __resetAdapterCacheForTest(): void { adapterCache.reset(); }
