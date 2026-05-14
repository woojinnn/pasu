export interface RecentCallLog {
  id: string;
  method: string;
  ok: boolean;
  duration_ms: number;
  error_code?: string;
}

export interface RecentBatchLog {
  request_id: string;
  started_at: string;
  duration_ms: number;
  calls: RecentCallLog[];
}

export class LogStore {
  private readonly entries: RecentBatchLog[] = [];

  constructor(private readonly capacity = 50) {}

  add(entry: RecentBatchLog): void {
    this.entries.push(copyEntry(entry));

    while (this.entries.length > this.capacity) {
      this.entries.shift();
    }
  }

  recent(): RecentBatchLog[] {
    return this.entries.map(copyEntry);
  }
}

function copyEntry(entry: RecentBatchLog): RecentBatchLog {
  return {
    request_id: entry.request_id,
    started_at: entry.started_at,
    duration_ms: entry.duration_ms,
    calls: entry.calls.map((call) => ({ ...call })),
  };
}
