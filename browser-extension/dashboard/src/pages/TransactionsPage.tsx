/**
 * `TransactionsPage` — recent tx lifecycle log from the server's
 * `state_deltas` table. Auto-refreshes when the user navigates here;
 * a "Reload" button does manual refetch.
 */

import { useCallback, useEffect, useState } from "react";

import { listTransactions, type TxRow } from "../server-api";

export function TransactionsPage() {
  const [rows, setRows] = useState<TxRow[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setRows(await listTransactions({ limit: 100 }));
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  return (
    <div style={page}>
      <header
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          marginBottom: 16,
        }}
      >
        <h1 style={{ margin: 0 }}>Transactions</h1>
        <button onClick={reload} style={smallBtn}>
          Reload
        </button>
      </header>

      {error && <p style={{ color: "crimson" }}>Failed: {error}</p>}
      {rows === null && !error && <p>Loading…</p>}
      {rows && rows.length === 0 && (
        <p style={{ opacity: 0.6 }}>
          No transactions recorded yet. The extension will log here every
          intercepted tx once it's signed in to the server.
        </p>
      )}
      {rows && rows.length > 0 && (
        <ul style={{ listStyle: "none", padding: 0, margin: 0 }}>
          {rows.map((r) => (
            <li key={r.id} style={cardRow}>
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "baseline",
                  marginBottom: 6,
                }}
              >
                <span style={{ fontWeight: 600 }}>
                  {r.action_domain}::{r.action_kind}
                </span>
                <span style={{ fontSize: 12, opacity: 0.6 }}>
                  #{r.id} · {timeAgo(r.created_at)}
                </span>
              </div>
              <div style={{ fontSize: 12, opacity: 0.75, marginBottom: 4 }}>
                <Badge label={r.status} />
                <Badge label={r.source} />
                {r.predicted_verdict && (
                  <Badge label={`verdict: ${r.predicted_verdict}`} />
                )}
              </div>
              <div style={{ fontFamily: "monospace", fontSize: 12, opacity: 0.7 }}>
                from: {r.submitter}
                {r.tx_hash && (
                  <>
                    <br />
                    hash: {r.tx_hash}
                  </>
                )}
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function Badge({ label }: { label: string }) {
  return (
    <span
      style={{
        display: "inline-block",
        padding: "1px 8px",
        marginRight: 6,
        fontSize: 11,
        borderRadius: 10,
        background: "#e8f0ff",
        color: "#0066cc",
      }}
    >
      {label}
    </span>
  );
}

function timeAgo(unixSec: number): string {
  const delta = Math.max(0, Math.round(Date.now() / 1000 - unixSec));
  if (delta < 60) return `${delta}s ago`;
  if (delta < 3600) return `${Math.round(delta / 60)}m ago`;
  if (delta < 86400) return `${Math.round(delta / 3600)}h ago`;
  return `${Math.round(delta / 86400)}d ago`;
}

const page: React.CSSProperties = { padding: 24, maxWidth: 800, margin: "0 auto" };
const cardRow: React.CSSProperties = {
  padding: 14,
  border: "1px solid #e2e2e6",
  background: "white",
  borderRadius: 6,
  marginBottom: 8,
};
const smallBtn: React.CSSProperties = {
  fontSize: 12,
  padding: "4px 10px",
  borderRadius: 4,
  border: "1px solid #888",
  background: "white",
  cursor: "pointer",
};
