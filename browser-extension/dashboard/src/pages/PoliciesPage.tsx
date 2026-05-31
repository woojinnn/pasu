/**
 * `PoliciesPage` — Cedar policies installed in the user's
 * `user_policies` table. Read-only at this stage; create / edit will
 * land with the policy CRUD endpoints in a future phase.
 */

import { useCallback, useEffect, useState } from "react";

import { listPolicies, type InstalledPolicy } from "../server-api";

export function PoliciesPage() {
  const [rows, setRows] = useState<InstalledPolicy[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setRows(await listPolicies());
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
        <h1 style={{ margin: 0 }}>Policies</h1>
        <button onClick={reload} style={smallBtn}>
          Reload
        </button>
      </header>

      {error && <p style={{ color: "crimson" }}>Failed: {error}</p>}
      {rows === null && !error && <p>Loading…</p>}
      {rows && rows.length === 0 && (
        <p style={{ opacity: 0.6 }}>
          No Cedar policies installed yet. POST /policies (not yet wired
          on the dashboard) will create one.
        </p>
      )}
      {rows && rows.length > 0 && (
        <ul style={{ listStyle: "none", padding: 0, margin: 0 }}>
          {rows.map((p) => (
            <li key={p.id} style={card}>
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "baseline",
                  marginBottom: 6,
                }}
              >
                <span style={{ fontWeight: 600 }}>{p.name}</span>
                <span style={{ fontSize: 12, opacity: 0.6 }}>
                  #{p.id} · {p.enabled ? "enabled" : "disabled"} · severity={p.severity}
                </span>
              </div>
              {p.description && (
                <p style={{ margin: "0 0 8px 0", fontSize: 13, opacity: 0.75 }}>
                  {p.description}
                </p>
              )}
              <pre
                style={{
                  margin: 0,
                  fontSize: 12,
                  fontFamily: "monospace",
                  whiteSpace: "pre-wrap",
                  wordBreak: "break-all",
                  background: "#f7f7f8",
                  padding: 10,
                  borderRadius: 4,
                }}
              >
                {p.cedar_text}
              </pre>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

const page: React.CSSProperties = { padding: 24, maxWidth: 900, margin: "0 auto" };
const card: React.CSSProperties = {
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
