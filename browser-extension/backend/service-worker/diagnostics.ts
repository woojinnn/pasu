/**
 * Diagnostics for the `__engine::timeout` (8 s `HARD_TIMEOUT_MS` budget overrun)
 * investigation. The 8 s timeout fires only when an *awaited* operation (a fetch)
 * does not resolve in time — sync WASM cannot fire it (it would block the event
 * loop and the timer with it). So the prime suspects are network calls. This
 * module makes them visible AND durable:
 *
 *  1. **Fetch ring buffer** — every registry / policy-rpc fetch registers a
 *     `start` (`ongoing: true`) and updates to `end` on completion. A *hung*
 *     call stays `ongoing` forever, so it is identifiable even when no response
 *     ever arrives — exactly the timeout case.
 *  2. **Phase timeline** — coarse per-request markers (lifecycle → route →
 *     verdict) so a stall *between* fetches is attributable to a phase.
 *  3. **Durable timeout snapshot** — on the 8 s firing, {@link captureTimeout}
 *     writes the in-flight + recent fetches and the phase timeline to
 *     `chrome.storage.local` so the cause survives SW eviction and is
 *     retrievable later — the bug is intermittent and not reproducible on
 *     demand, so "be watching the console at the exact moment" is not viable.
 *
 * Read the captured snapshots from the SW DevTools console with:
 *   `chrome.storage.local.get("dambi_diag_timeouts").then(console.log)`
 */

export interface FetchEvent {
  seq: number;
  /** "callkey" | "typed-data" | "token" | "dispatch" */
  label: string;
  url: string;
  sentAtMs: number;
  sentAt: string;
  doneAt?: string;
  durationMs?: number;
  status?: number | string;
  /** true until the fetch resolves/rejects — a stuck call never clears this. */
  ongoing: boolean;
}

const RING_CAP = 64;
const ring: FetchEvent[] = [];
let seqCounter = 0;

/** Register a fetch as in-flight; returns the seq to pass to {@link fetchEnded}. */
export function fetchStarted(label: string, url: string): number {
  const seq = ++seqCounter;
  const sentAtMs = Date.now();
  ring.push({
    seq,
    label,
    url,
    sentAtMs,
    sentAt: new Date(sentAtMs).toISOString(),
    ongoing: true,
  });
  while (ring.length > RING_CAP) ring.shift();
  return seq;
}

/** Mark a previously-started fetch as done. Idempotent — only the first end wins. */
export function fetchEnded(
  seq: number,
  status: number | string,
  durationMs: number,
): void {
  const ev = ring.find((e) => e.seq === seq);
  if (!ev || !ev.ongoing) return;
  ev.ongoing = false;
  ev.doneAt = new Date().toISOString();
  ev.durationMs = durationMs;
  ev.status = status;
}

interface PhaseMark {
  phase: string;
  atMs: number;
  at: string;
  extra?: unknown;
}

const PHASE_REQUESTS_CAP = 24;
const phaseTraces = new Map<string, PhaseMark[]>();

/** Record a coarse phase boundary for `requestId` (e.g. "route_done"). */
export function markPhase(
  requestId: string,
  phase: string,
  extra?: unknown,
): void {
  let marks = phaseTraces.get(requestId);
  if (!marks) {
    marks = [];
    phaseTraces.set(requestId, marks);
    while (phaseTraces.size > PHASE_REQUESTS_CAP) {
      const oldest = phaseTraces.keys().next().value;
      if (oldest === undefined) break;
      phaseTraces.delete(oldest);
    }
  }
  const atMs = Date.now();
  marks.push({ phase, atMs, at: new Date(atMs).toISOString(), extra });
}

/** Free the phase trace for a finished request. */
export function clearPhase(requestId: string): void {
  phaseTraces.delete(requestId);
}

const STORAGE_KEY = "dambi_diag_timeouts";
const MAX_SNAPSHOTS = 12;

export interface TimeoutSnapshot {
  capturedAt: string;
  requestId: string;
  tx: Record<string, unknown>;
  note: string;
  /** Sent-but-never-returned fetches — the prime suspect for the 8 s hang. */
  ongoingFetches: FetchEvent[];
  /** All fetches in the ~12 s window before the timeout (completed + ongoing). */
  recentFetches: FetchEvent[];
  phaseTimeline: Array<{
    phase: string;
    at: string;
    deltaMs: number;
    extra?: unknown;
  }>;
}

/**
 * Storage accessor via the SW's `chrome`/`browser` global — deliberately NOT
 * `import`-ing `webextension-polyfill` at module load, so this module stays
 * import-safe in unit tests that don't run inside an extension. Returns null
 * outside an extension (tests), where {@link captureTimeout} then no-ops.
 */
function localStorageArea():
  | {
      get: (key: string) => Promise<Record<string, unknown>>;
      set: (items: Record<string, unknown>) => Promise<void>;
    }
  | null {
  const g = globalThis as unknown as {
    browser?: { storage?: { local?: unknown } };
    chrome?: { storage?: { local?: unknown } };
  };
  const area = g.browser?.storage?.local ?? g.chrome?.storage?.local;
  return (area as ReturnType<typeof localStorageArea>) ?? null;
}

/**
 * Snapshot the in-flight + recent fetches and the phase timeline for a timed-out
 * decision, log it loudly, and persist it durably to `chrome.storage.local`.
 */
export async function captureTimeout(
  requestId: string,
  tx: Record<string, unknown>,
): Promise<void> {
  const nowMs = Date.now();
  const windowStartMs = nowMs - 12_000;
  const recent = ring.filter((e) => e.sentAtMs >= windowStartMs);
  const marks = phaseTraces.get(requestId) ?? [];
  const baseMs = marks.length > 0 ? marks[0].atMs : nowMs;
  const phaseTimeline = marks.map((m) => ({
    phase: m.phase,
    at: m.at,
    deltaMs: m.atMs - baseMs,
    extra: m.extra,
  }));

  const snapshot: TimeoutSnapshot = {
    capturedAt: new Date(nowMs).toISOString(),
    requestId,
    tx,
    note: "8000ms HARD_TIMEOUT_MS fired. ongoingFetches = sent but never returned (prime suspects). A missing late phase (e.g. no route_done) localises the stall.",
    ongoingFetches: recent.filter((e) => e.ongoing),
    recentFetches: recent,
    phaseTimeline,
  };

  console.warn(
    "[Dambi] ⏱️ __engine::timeout DIAGNOSTICS (also saved to storage)",
    snapshot,
  );

  const storage = localStorageArea();
  if (!storage) {
    console.warn(
      "[Dambi] no storage area available — timeout diagnostics logged to console only",
    );
    return;
  }
  try {
    const stored = await storage.get(STORAGE_KEY);
    const prev = stored[STORAGE_KEY];
    const list: TimeoutSnapshot[] = Array.isArray(prev)
      ? (prev as TimeoutSnapshot[])
      : [];
    list.push(snapshot);
    while (list.length > MAX_SNAPSHOTS) list.shift();
    await storage.set({ [STORAGE_KEY]: list });
    console.warn(
      `[Dambi] timeout diagnostics saved → chrome.storage.local["${STORAGE_KEY}"] (${list.length} stored). Read with: chrome.storage.local.get("${STORAGE_KEY}").then(console.log)`,
    );
  } catch (err) {
    console.warn("[Dambi] failed to persist timeout diagnostics", err);
  }
}
