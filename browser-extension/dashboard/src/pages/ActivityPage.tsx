/**
 * `ActivityPage` — live SSE feed.
 *
 * Subscribes to the server's `/events/stream` and renders incoming
 * events newest-first. Empty state + connection indicator included so
 * the page is useful on a fresh user without traffic.
 */

import { useEventStream } from "../hooks/useEventStream";

export function ActivityPage() {
  const { events, status, clear } = useEventStream();

  const reversed = [...events].reverse();

  return (
    <div style={{ padding: 24, maxWidth: 720, margin: "0 auto" }}>
      <header
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: 16,
        }}
      >
        <h1>Activity</h1>
        <span style={{ fontSize: 12, opacity: 0.7 }}>
          {labelForStatus(status)}
          {events.length > 0 && (
            <>
              {" · "}
              <button onClick={clear} style={inlineBtn}>
                Clear
              </button>
            </>
          )}
        </span>
      </header>

      {events.length === 0 ? (
        <p style={{ opacity: 0.6 }}>
          No events yet. Tx confirmations and wallet syncs will appear here
          in real time.
        </p>
      ) : (
        <ul style={{ listStyle: "none", padding: 0 }}>
          {reversed.map((ev, i) => (
            <li
              key={`${ev.receivedAt}-${i}`}
              style={{
                padding: "10px 14px",
                border: "1px solid #ddd",
                borderRadius: 6,
                marginBottom: 8,
                fontFamily: "monospace",
                fontSize: 13,
              }}
            >
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  marginBottom: 4,
                }}
              >
                <strong>{ev.kind}</strong>
                <span style={{ opacity: 0.5, fontSize: 11 }}>
                  {timeAgo(ev.receivedAt)}
                </span>
              </div>
              <pre
                style={{
                  margin: 0,
                  whiteSpace: "pre-wrap",
                  wordBreak: "break-all",
                  fontSize: 12,
                  opacity: 0.8,
                }}
              >
                {JSON.stringify(ev.data, null, 2)}
              </pre>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function labelForStatus(s: "idle" | "connecting" | "open" | "error"): string {
  switch (s) {
    case "idle":
      return "● idle";
    case "connecting":
      return "◐ connecting…";
    case "open":
      return "● live";
    case "error":
      return "○ error";
  }
}

function timeAgo(ms: number): string {
  const delta = Math.max(0, Math.round((Date.now() - ms) / 1000));
  if (delta < 5) return "just now";
  if (delta < 60) return `${delta}s ago`;
  if (delta < 3600) return `${Math.round(delta / 60)}m ago`;
  return `${Math.round(delta / 3600)}h ago`;
}

const inlineBtn: React.CSSProperties = {
  background: "transparent",
  border: "none",
  color: "#0066cc",
  cursor: "pointer",
  fontSize: 12,
  padding: 0,
};
