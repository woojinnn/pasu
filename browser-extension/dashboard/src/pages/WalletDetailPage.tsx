/**
 * `WalletDetailPage` — per-wallet inspector. Buttons fan out to every
 * read endpoint the server exposes for a wallet:
 *   - GET /wallets/:addr/state
 *   - GET /wallets/:addr/holdings
 *   - GET /wallets/:addr/approvals
 *   - GET /wallets/:addr/block-heights
 * plus POST /wallets/:addr/sync to force a refresh.
 */

import { useCallback, useState } from "react";
import { Link, useParams } from "react-router-dom";

import {
  getWalletApprovals,
  getWalletBlockHeights,
  getWalletHoldings,
  getWalletState,
  request,
} from "../server-api";

type Pane = "state" | "holdings" | "approvals" | "block-heights";

export function WalletDetailPage() {
  const { address = "" } = useParams<{ address: string }>();
  const [pane, setPane] = useState<Pane>("state");
  const [data, setData] = useState<unknown>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);

  const load = useCallback(
    async (which: Pane) => {
      setPane(which);
      setLoading(true);
      setError(null);
      setData(null);
      try {
        let result: unknown;
        switch (which) {
          case "state":
            result = await getWalletState(address);
            break;
          case "holdings":
            result = await getWalletHoldings(address);
            break;
          case "approvals":
            result = await getWalletApprovals(address);
            break;
          case "block-heights":
            result = await getWalletBlockHeights(address);
            break;
        }
        setData(result);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setLoading(false);
      }
    },
    [address],
  );

  const onSync = async () => {
    setSyncing(true);
    setError(null);
    try {
      await request<void>(`/wallets/${address}/sync`, { method: "POST" });
      // Re-load the active pane so the UI shows what the sync wrote.
      await load(pane);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSyncing(false);
    }
  };

  return (
    <div style={page}>
      <Link to="/server/wallets" style={backLink}>
        ← Wallets
      </Link>
      <h1 style={{ marginTop: 8 }}>
        <span style={{ fontFamily: "monospace", fontSize: 14 }}>{address}</span>
      </h1>

      {/* Endpoint buttons */}
      <div style={tabRow}>
        <Tab active={pane === "state"} onClick={() => load("state")}>
          GET /state
        </Tab>
        <Tab active={pane === "holdings"} onClick={() => load("holdings")}>
          GET /holdings
        </Tab>
        <Tab active={pane === "approvals"} onClick={() => load("approvals")}>
          GET /approvals
        </Tab>
        <Tab active={pane === "block-heights"} onClick={() => load("block-heights")}>
          GET /block-heights
        </Tab>
        <button onClick={onSync} disabled={syncing} style={syncBtn}>
          {syncing ? "Syncing…" : "POST /sync"}
        </button>
      </div>

      {/* Response */}
      <section style={card}>
        {!data && !loading && !error && (
          <p style={{ opacity: 0.6 }}>Pick an endpoint above.</p>
        )}
        {loading && <p>Loading…</p>}
        {error && <p style={{ color: "crimson" }}>Error: {error}</p>}
        {data !== null && (
          <pre
            style={{
              margin: 0,
              fontSize: 12,
              fontFamily: "monospace",
              whiteSpace: "pre-wrap",
              wordBreak: "break-all",
            }}
          >
            {JSON.stringify(data, null, 2)}
          </pre>
        )}
      </section>
    </div>
  );
}

function Tab({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      style={{
        fontSize: 12,
        padding: "6px 12px",
        borderRadius: 4,
        border: active ? "1px solid #0066cc" : "1px solid #ccc",
        background: active ? "#0066cc" : "white",
        color: active ? "white" : "#333",
        cursor: "pointer",
        fontFamily: "monospace",
      }}
    >
      {children}
    </button>
  );
}

const page: React.CSSProperties = { padding: 24, maxWidth: 900, margin: "0 auto" };
const backLink: React.CSSProperties = {
  fontSize: 13,
  color: "#0066cc",
  textDecoration: "none",
};
const tabRow: React.CSSProperties = {
  display: "flex",
  gap: 8,
  flexWrap: "wrap",
  marginBottom: 16,
};
const card: React.CSSProperties = {
  background: "white",
  border: "1px solid #e2e2e6",
  borderRadius: 8,
  padding: 16,
  minHeight: 200,
};
const syncBtn: React.CSSProperties = {
  fontSize: 12,
  padding: "6px 12px",
  borderRadius: 4,
  border: "1px solid #28a745",
  background: "#28a745",
  color: "white",
  cursor: "pointer",
  fontFamily: "monospace",
  marginLeft: "auto",
};
