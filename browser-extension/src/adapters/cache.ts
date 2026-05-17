import type { ChainId, Hex, Manifest } from "./types";

export interface CacheEntry {
  version: string;
  manifest: Manifest;
  wasm: Uint8Array;
  fetchedAt: number;
}

export interface CacheBackend {
  get(key: string): Promise<string | undefined>;
  set(key: string, value: string): Promise<void>;
  delete(key: string): Promise<void>;
  keys(): Promise<string[]>;
}

export interface CacheOptions {
  capacity: number;
  ttlMs: number;
}

const META_KEY = "__adapter_cache_lru__";

interface Meta {
  order: string[];
}

export class AdapterCache {
  constructor(private backend: CacheBackend, private opts: CacheOptions) {}

  private cacheKey(chain: ChainId, address: Hex): string {
    return `adapter:${chain}:${address.toLowerCase()}`;
  }

  private async loadMeta(): Promise<Meta> {
    const raw = await this.backend.get(META_KEY);
    if (!raw) return { order: [] };
    try { return JSON.parse(raw) as Meta; } catch { return { order: [] }; }
  }

  private async saveMeta(meta: Meta): Promise<void> {
    await this.backend.set(META_KEY, JSON.stringify(meta));
  }

  async get(chain: ChainId, address: Hex): Promise<CacheEntry | null> {
    const key = this.cacheKey(chain, address);
    const raw = await this.backend.get(key);
    if (!raw) return null;
    const entry = deserialize(raw);
    if (Date.now() - entry.fetchedAt > this.opts.ttlMs) {
      await this.backend.delete(key);
      return null;
    }
    const meta = await this.loadMeta();
    meta.order = [key, ...meta.order.filter((k) => k !== key)];
    await this.saveMeta(meta);
    return entry;
  }

  async put(chain: ChainId, address: Hex, entry: CacheEntry): Promise<void> {
    const key = this.cacheKey(chain, address);
    await this.backend.set(key, serialize(entry));
    const meta = await this.loadMeta();
    meta.order = [key, ...meta.order.filter((k) => k !== key)];
    while (meta.order.length > this.opts.capacity) {
      const evict = meta.order.pop();
      if (evict) await this.backend.delete(evict);
    }
    await this.saveMeta(meta);
  }
}

function serialize(e: CacheEntry): string {
  return JSON.stringify({
    version: e.version,
    manifest: e.manifest,
    wasm: Array.from(e.wasm),
    fetchedAt: e.fetchedAt,
  });
}

function deserialize(s: string): CacheEntry {
  const obj = JSON.parse(s);
  return { ...obj, wasm: new Uint8Array(obj.wasm) } as CacheEntry;
}
