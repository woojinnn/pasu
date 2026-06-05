/**
 * registry-api — single-flight (in-flight request coalescing).
 *
 * 세 번째 DoS/비용 완화 layer. 같은 cold object 에 동시 cache-miss 가 N개
 * 들어오면 (다중 사용자 thundering-herd) 그대로 N개 GCS read 가 된다 (위협모델
 * A3). single-flight 는 같은 key 의 진행 중 read 를 하나로 묶어 — 첫 호출만 실제
 * fn() 을 돌리고 나머지는 그 Promise 를 공유 — 1회 read 로 만든다.
 *
 * 키별 Promise 는 settle 시 (성공·실패 모두 `finally`) 즉시 제거한다. 따라서
 * transient 실패가 key 를 wedge 하지 않고, 다음 요청은 새 read 를 시작한다.
 * 캐시(positive/negative)와 독립 — 캐시는 *완료된* 결과를, single-flight 는
 * *진행 중인* read 를 dedupe 한다.
 */
export class SingleFlight<T> {
  private readonly inflight = new Map<string, Promise<T>>();

  /** `key` 에 진행 중 호출이 있으면 그 Promise 를, 없으면 `fn()` 을 1회 실행. */
  run(key: string, fn: () => Promise<T>): Promise<T> {
    const existing = this.inflight.get(key);
    if (existing) return existing;
    const promise = (async () => {
      try {
        return await fn();
      } finally {
        this.inflight.delete(key);
      }
    })();
    this.inflight.set(key, promise);
    return promise;
  }

  size(): number {
    return this.inflight.size;
  }
}
