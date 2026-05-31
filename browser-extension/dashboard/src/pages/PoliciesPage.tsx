/**
 * `PoliciesPage` — Cedar policies installed in the user's
 * `user_policies` table. Read-only at this stage; create / edit will
 * land with the policy CRUD endpoints in a future phase.
 */

import { useCallback, useEffect, useState } from "react";

import {
  createPolicy,
  deletePolicy,
  listPolicies,
  patchPolicy,
  type InstalledPolicy,
} from "../server-api";

export function PoliciesPage() {
  const [rows, setRows] = useState<InstalledPolicy[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);

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

  const onToggle = async (p: InstalledPolicy) => {
    try {
      await patchPolicy(p.id, { enabled: !p.enabled });
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const onDelete = async (p: InstalledPolicy) => {
    if (!confirm(`Delete "${p.name}"?`)) return;
    try {
      await deletePolicy(p.id);
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

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
        <div style={{ display: "flex", gap: 8 }}>
          <button onClick={() => setShowCreate((v) => !v)} style={smallBtn}>
            {showCreate ? "Cancel" : "+ New"}
          </button>
          <button onClick={reload} style={smallBtn}>
            Reload
          </button>
        </div>
      </header>

      {showCreate && (
        <CreatePolicyForm
          onCreated={() => {
            setShowCreate(false);
            void reload();
          }}
          onError={(e) => setError(e)}
        />
      )}

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
                  gap: 8,
                }}
              >
                <span style={{ fontWeight: 600 }}>{p.name}</span>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <span style={{ fontSize: 12, opacity: 0.6 }}>
                    #{p.id} · {p.enabled ? "enabled" : "disabled"} · severity={p.severity}
                  </span>
                  <button onClick={() => onToggle(p)} style={smallBtn}>
                    {p.enabled ? "Disable" : "Enable"}
                  </button>
                  <button
                    onClick={() => onDelete(p)}
                    style={{ ...smallBtn, color: "crimson" }}
                  >
                    Delete
                  </button>
                </div>
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

function CreatePolicyForm({
  onCreated,
  onError,
}: {
  onCreated: () => void;
  onError: (e: string) => void;
}) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [severity, setSeverity] = useState<"deny" | "warn" | "info">("warn");
  const [cedarText, setCedarText] = useState(
    'permit(principal, action, resource);\n// edit this to a real rule',
  );
  const [busy, setBusy] = useState(false);

  const onSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      await createPolicy({
        name: name.trim(),
        description: description.trim() || null,
        cedar_text: cedarText,
        severity,
      });
      setName("");
      setDescription("");
      setCedarText("permit(principal, action, resource);");
      onCreated();
    } catch (err) {
      onError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form onSubmit={onSubmit} style={createForm}>
      <div style={{ display: "flex", gap: 8 }}>
        <input
          placeholder="Policy name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          required
          style={{ ...inputStyle, flex: 2 }}
        />
        <select
          value={severity}
          onChange={(e) => setSeverity(e.target.value as "deny" | "warn" | "info")}
          style={{ ...inputStyle, flex: 1 }}
        >
          <option value="deny">deny</option>
          <option value="warn">warn</option>
          <option value="info">info</option>
        </select>
      </div>
      <input
        placeholder="Description (optional)"
        value={description}
        onChange={(e) => setDescription(e.target.value)}
        style={inputStyle}
      />
      <textarea
        placeholder="Cedar policy text"
        value={cedarText}
        onChange={(e) => setCedarText(e.target.value)}
        required
        rows={6}
        style={{ ...inputStyle, fontFamily: "monospace", fontSize: 12 }}
      />
      <button type="submit" disabled={busy} style={{ ...smallBtn, alignSelf: "flex-start" }}>
        {busy ? "Saving…" : "Create policy"}
      </button>
    </form>
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
const createForm: React.CSSProperties = {
  display: "flex",
  flexDirection: "column",
  gap: 8,
  padding: 14,
  marginBottom: 16,
  border: "1px solid #e2e2e6",
  background: "white",
  borderRadius: 6,
};
const inputStyle: React.CSSProperties = {
  fontSize: 13,
  padding: "6px 10px",
  borderRadius: 4,
  border: "1px solid #bbb",
  background: "white",
};
