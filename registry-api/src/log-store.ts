/**
 * registry-api — recent-request ring buffer (policy-rpc/src/log-store.ts
 * 포팅). /debug/recent 를 뒷받침. 고정 capacity 라 무한 성장 불가. entry 는
 * 복사 in/out 이라 caller 가 저장 history 를 못 바꾼다.
 */
export interface RecentRequestLog {
  ts: string;
  path: string;
  status: number;
  cache: "hit" | "miss" | "n/a";
  duration_ms: number;
}

export class LogStore {
  private readonly entries: RecentRequestLog[] = [];
  constructor(private readonly capacity = 50) {}

  add(entry: RecentRequestLog): void {
    this.entries.push({ ...entry });
    while (this.entries.length > this.capacity) this.entries.shift();
  }

  recent(): RecentRequestLog[] {
    return this.entries.map((e) => ({ ...e }));
  }
}
