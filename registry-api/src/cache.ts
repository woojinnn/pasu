/**
 * registry-api — in-memory LRU + TTL object cache.
 *
 * 핵심 DoS 완화: 동일 callkey 폭주가 전부 RAM 에서 처리돼 hostile dapp 이
 * 익스텐션을 GCS-billing 증폭기로 못 쓴다. 성공 object (positive) + 404
 * (negative) 둘 다 캐시 — 모르는 프로토콜이 흔한 miss 라, 404 캐싱이 probe
 * storm 의 guaranteed-miss read 를 막는다.
 *
 * Map insertion-order LRU (익스텐션 token-client.ts mem 캐시와 같은 패턴):
 * hit 시 재삽입으로 뒤로 이동, capacity 초과 시 keys().next() evict.
 * TTL 은 per-entry lazy expiry.
 *
 * Per-instance only — Cloud Run 은 여러 instance 를 띄울 수 있고 각자 캐시를
 * 가진다. 캐시는 cost/latency 최적화지 correctness 장치가 아니므로 허용 가능.
 * --max-instances 가 fan-out 을 bound 한다.
 */
export interface CachedObject {
  status: 200;
  body: Buffer;
  contentType: string;
}
export interface CachedMiss {
  status: 404;
}
export type CacheValue = CachedObject | CachedMiss;

interface CacheEntry {
  value: CacheValue;
  expiresAt: number;
}

export interface ObjectCacheOptions {
  maxEntries: number;
  ttlMs: number;
  negativeTtlMs: number;
  nowMs?: () => number;
}
export interface CacheStats {
  hits: number;
  misses: number;
  size: number;
}

export class ObjectCache {
  private readonly entries = new Map<string, CacheEntry>();
  private readonly maxEntries: number;
  private readonly ttlMs: number;
  private readonly negativeTtlMs: number;
  private readonly nowMs: () => number;
  private hits = 0;
  private misses = 0;

  constructor(o: ObjectCacheOptions) {
    this.maxEntries = Math.max(1, o.maxEntries);
    this.ttlMs = o.ttlMs;
    this.negativeTtlMs = o.negativeTtlMs;
    this.nowMs = o.nowMs ?? Date.now;
  }

  get(key: string): CacheValue | undefined {
    const e = this.entries.get(key);
    if (!e) {
      this.misses += 1;
      return undefined;
    }
    if (this.nowMs() >= e.expiresAt) {
      this.entries.delete(key);
      this.misses += 1;
      return undefined;
    }
    this.entries.delete(key);
    this.entries.set(key, e); // LRU touch
    this.hits += 1;
    return e.value;
  }

  set(key: string, value: CacheValue): void {
    const ttl = value.status === 404 ? this.negativeTtlMs : this.ttlMs;
    this.entries.delete(key);
    this.entries.set(key, { value, expiresAt: this.nowMs() + ttl });
    while (this.entries.size > this.maxEntries) {
      const oldest = this.entries.keys().next().value;
      if (oldest === undefined) break;
      this.entries.delete(oldest);
    }
  }

  stats(): CacheStats {
    return { hits: this.hits, misses: this.misses, size: this.entries.size };
  }
}
