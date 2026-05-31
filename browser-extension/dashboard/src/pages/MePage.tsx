/**
 * `MePage` — minimal profile + diagnostic page.
 *
 * Surfaces what the server thinks of the current session and the raw
 * config the client is wired to. Useful when debugging "is my JWT
 * actually being sent" / "which user does the server see".
 */

import { useEffect, useState } from "react";

import { SERVER_BASE_URL, getStoredToken } from "../server-api";
import { useAuth } from "../hooks/useAuth";

export function MePage() {
  const { user, refresh } = useAuth();
  const [health, setHealth] = useState<string | null>(null);

  useEffect(() => {
    fetch(`${SERVER_BASE_URL}/health`)
      .then((r) => r.text())
      .then(setHealth)
      .catch(() => setHealth("(unreachable)"));
  }, []);

  const token = getStoredToken();
  const tokenPreview = token ? `${token.slice(0, 20)}…${token.slice(-8)}` : "(none)";

  return (
    <div style={page}>
      <h1 style={{ marginTop: 0 }}>Profile</h1>

      <Section title="Authenticated user">
        <Field label="user_id" value={user?.user_id ?? "(not loaded)"} mono />
        <Field label="email" value={user?.email ?? "(not loaded)"} />
      </Section>

      <Section title="Client config">
        <Field label="Server URL" value={SERVER_BASE_URL} mono />
        <Field label="JWT (stored)" value={tokenPreview} mono />
      </Section>

      <Section title="Server diagnostics">
        <Field label="GET /health" value={health ?? "checking…"} mono />
      </Section>

      <button onClick={refresh} style={btn}>
        Re-fetch /auth/me
      </button>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section
      style={{
        background: "white",
        border: "1px solid #e2e2e6",
        borderRadius: 8,
        padding: 16,
        marginBottom: 16,
      }}
    >
      <h2 style={{ margin: "0 0 12px 0", fontSize: 14, color: "#555" }}>{title}</h2>
      {children}
    </section>
  );
}

function Field({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div style={{ display: "flex", gap: 12, padding: "4px 0", fontSize: 13 }}>
      <span style={{ width: 120, color: "#777" }}>{label}</span>
      <span style={{ fontFamily: mono ? "monospace" : undefined, wordBreak: "break-all" }}>
        {value}
      </span>
    </div>
  );
}

const page: React.CSSProperties = {
  padding: 24,
  maxWidth: 720,
  margin: "0 auto",
};

const btn: React.CSSProperties = {
  fontSize: 12,
  padding: "6px 12px",
  borderRadius: 4,
  border: "1px solid #888",
  background: "white",
  cursor: "pointer",
};
