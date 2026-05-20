/**
 * registry-api — per-IP token-bucket rate limiter.
 *
 * 두 번째 DoS 완화 layer (캐시 다음). token bucket 은 정상 페이지 로드
 * (dapp 이 callkey + token lookup 을 연달아 쏠 수 있음) 에 넉넉한 burst 를
 * 주면서 한 IP 의 지속 rate 를 cap 한다.
 *
 * 정직한 한계 — 이 limiter 는 CLOUD RUN INSTANCE 별 in-process 메모리다.
 * Cloud Run 이 instance 간 load-balance 하므로 공격자가 instance 에 퍼지면
 * burst * instanceCount 유효 capacity 를 보고, instance recycle 시 카운터가
 * 리셋된다. 비용 통제 speed bump 지 진짜 anti-DDoS 가 아니다. 진짜 per-IP
 * edge enforcement 는 Cloud Armor + 외부 LB. PoC baseline 은 instance-local
 * bucket + 캐시 + --max-instances 로 worst-case GCS read 와 비용을 bound 한다.
 *
 * IP map 자체도 LRU-bound — spoofed-source flood 가 limiter 메모리를 무한
 * 키우지 못한다.
 */
interface Bucket {
  tokens: number;
  lastRefillMs: number;
}

export interface RateLimiterOptions {
  burst: number;
  refillPerSec: number;
  maxIps: number;
  nowMs?: () => number;
}

export class TokenBucketRateLimiter {
  private readonly buckets = new Map<string, Bucket>();
  private readonly burst: number;
  private readonly refillPerSec: number;
  private readonly maxIps: number;
  private readonly nowMs: () => number;

  constructor(o: RateLimiterOptions) {
    this.burst = Math.max(1, o.burst);
    this.refillPerSec = Math.max(0, o.refillPerSec);
    this.maxIps = Math.max(1, o.maxIps);
    this.nowMs = o.nowMs ?? Date.now;
  }

  /** `ip` 에 토큰 1개 소비. 버킷이 비면 false. */
  allow(ip: string): boolean {
    const now = this.nowMs();
    let b = this.buckets.get(ip);
    if (!b) {
      b = { tokens: this.burst, lastRefillMs: now };
    } else {
      this.buckets.delete(ip); // LRU touch
      const elapsedSec = (now - b.lastRefillMs) / 1000;
      b.tokens = Math.min(this.burst, b.tokens + elapsedSec * this.refillPerSec);
      b.lastRefillMs = now;
    }
    let allowed = false;
    if (b.tokens >= 1) {
      b.tokens -= 1;
      allowed = true;
    }
    this.buckets.set(ip, b);
    while (this.buckets.size > this.maxIps) {
      const oldest = this.buckets.keys().next().value;
      if (oldest === undefined) break;
      this.buckets.delete(oldest);
    }
    return allowed;
  }

  size(): number {
    return this.buckets.size;
  }
}
