/**
 * `useEventStream` — subscribes to the server's `GET /events/stream`
 * SSE feed and exposes the most-recent N events to a component.
 *
 * Lifecycle:
 * - Opens an `EventSource` when the hook mounts and a JWT is available.
 * - Reconnects automatically (browser-managed) using `Last-Event-ID`.
 * - Closes the connection on unmount or logout.
 *
 * The `kinds` argument filters which event types are accumulated; pass
 * `undefined` or `[]` to accept every kind. Filtering happens client-side
 * because the server's SSE endpoint sends every event for the user — the
 * activity feed wants all of them, more focused pages want fewer.
 */

import { useEffect, useRef, useState } from "react";

import { getStoredToken, urlWithTokenQuery } from "../server-api";

/** Generic shape — server sends `{type, ...payload}`. Pages should
 * narrow `kind` and cast `data` to a specific Rust→TS DTO. */
export interface ServerEvent {
  /** Discriminator, e.g. `"tx_confirmed"`. Matches the SSE `event:` field
   * AND the JSON `type` field. */
  kind: string;
  /** Parsed JSON payload from the SSE `data:` line. */
  data: Record<string, unknown>;
  /** Local receive timestamp (ms since epoch). Server doesn't tag
   * events with their wall-clock time today; this is the next best
   * thing for "x seconds ago" UI. */
  receivedAt: number;
}

export interface UseEventStreamOptions {
  /** If provided, only events whose `kind` is in this set are kept. */
  kinds?: string[];
  /** Cap on how many events to retain (FIFO). Default 200. */
  bufferSize?: number;
  /** Pass `false` to keep the connection open but drop new events.
   * Useful when a page is mounted but the user is hidden behind a tab. */
  enabled?: boolean;
}

export type ConnectionState = "idle" | "connecting" | "open" | "error";

export interface UseEventStreamResult {
  events: ServerEvent[];
  status: ConnectionState;
  /** Manual reset — drop accumulated events without bouncing the socket. */
  clear: () => void;
}

export function useEventStream(opts: UseEventStreamOptions = {}): UseEventStreamResult {
  const { kinds, bufferSize = 200, enabled = true } = opts;
  const kindSet = kinds && kinds.length ? new Set(kinds) : null;

  const [events, setEvents] = useState<ServerEvent[]>([]);
  const [status, setStatus] = useState<ConnectionState>("idle");
  const sourceRef = useRef<EventSource | null>(null);

  useEffect(() => {
    if (!enabled) {
      setStatus("idle");
      return;
    }
    const token = getStoredToken();
    if (!token) {
      setStatus("idle");
      return;
    }

    setStatus("connecting");
    const es = new EventSource(urlWithTokenQuery("/events/stream", token));
    sourceRef.current = es;

    es.onopen = () => setStatus("open");
    es.onerror = () => setStatus("error");

    // The server sends `event: <kind>\ndata: {…}\n\n`. EventSource
    // dispatches one DOM event PER kind, so we listen with a single
    // generic handler attached via `addEventListener("message", …)`
    // for the default case AND specific kinds dynamically. Simpler:
    // grab everything via a wildcard pattern — the server's known
    // kinds are bounded, so we install per-kind listeners up front.
    //
    // Trade-off: a new kind on the server needs a frontend
    // listener for the activity feed to surface it. Acceptable
    // since the kind set changes with the schema.
    const handle = (rawEvent: MessageEvent) => {
      const kind = rawEvent.type;
      if (kindSet && !kindSet.has(kind)) return;
      let data: Record<string, unknown>;
      try {
        data = JSON.parse(rawEvent.data) as Record<string, unknown>;
      } catch {
        // Non-JSON keepalive / comment — skip.
        return;
      }
      setEvents((prev) => {
        const next = [...prev, { kind, data, receivedAt: Date.now() }];
        return next.length > bufferSize ? next.slice(-bufferSize) : next;
      });
    };

    const KNOWN_KINDS = [
      "tx_predicted",
      "tx_pending",
      "tx_confirmed",
      "tx_failed",
      "wallet_synced",
      "policy_violated",
    ];
    for (const k of KNOWN_KINDS) es.addEventListener(k, handle);

    return () => {
      for (const k of KNOWN_KINDS) es.removeEventListener(k, handle);
      es.close();
      sourceRef.current = null;
      setStatus("idle");
    };
  }, [enabled, bufferSize, kinds?.join(",")]);

  return {
    events,
    status,
    clear: () => setEvents([]),
  };
}
