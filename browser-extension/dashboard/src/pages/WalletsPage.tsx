/**
 * `WalletsPage` — list, add, and refresh wallets the policy-rpc server
 * is tracking for the authenticated user. Each row links into the
 * `WalletDetailPage` (state / holdings / approvals / block-heights).
 */

import { FormEvent, useCallback, useEffect, useState } from "react";
import { Link } from "react-router-dom";

import { deleteWallet, listWallets, request, type WalletId } from "../server-api";

interface AddWalletResp {
  wallet_id: WalletId;
  synced: boolean;
  error?: string;
}

export function WalletsPage() {
  const [wallets, setWallets] = useState<WalletId[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  // Add-wallet form state. `chains` is intentionally optional — the
  // server defaults to every chain in scopeball-sync.toml when empty.
  // Advanced users can pin a subset; most users don't think in CAIP-2.
  const [addr, setAddr] = useState("");
  const [chains, setChains] = useState("");
  const [advanced, setAdvanced] = useState(false);
  const [addError, setAddError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      const w = await listWallets();
      setWallets(w);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const onAdd = async (e: FormEvent) => {
    e.preventDefault();
    setAddError(null);
    setBusy("add");
    try {
      // Empty chains array → server defaults to every configured chain.
      const chainList = chains
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
      const resp = await request<AddWalletResp>("/wallets", {
        method: "POST",
        body: { address: addr.trim(), chains: chainList },
      });
      if (resp.error) setAddError(resp.error);
      setAddr("");
      await reload();
    } catch (e) {
      setAddError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const onSync = async (address: string) => {
    setBusy(`sync:${address}`);
    try {
      await request<void>(`/wallets/${address}/sync`, { method: "POST" });
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const onArchive = async (address: string) => {
    if (!confirm(`Archive wallet ${address}? (soft delete — DB rows stay)`)) return;
    setBusy(`archive:${address}`);
    try {
      await deleteWallet(address);
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div style={page}>
      <h1 style={{ marginTop: 0 }}>Wallets</h1>

      {error && <p style={{ color: "crimson" }}>Failed to load: {error}</p>}

      {/* Add form */}
      <section style={card}>
        <h2 style={cardTitle}>+ Add wallet</h2>
        <form onSubmit={onAdd} style={{ display: "grid", gap: 8 }}>
          <label style={lbl}>
            Address
            <input
              type="text"
              placeholder="0xd8da6bf26964af9d7eed9e03e53415d37aa96045"
              value={addr}
              onChange={(e) => setAddr(e.target.value)}
              required
              style={input}
            />
          </label>
          <div style={{ fontSize: 11, opacity: 0.6 }}>
            Tracked across every chain the server has RPC for (Ethereum,
            Arbitrum, Base). {" "}
            <button
              type="button"
              onClick={() => setAdvanced((a) => !a)}
              style={inlineLink}
            >
              {advanced ? "Hide advanced" : "Advanced — pin chains"}
            </button>
          </div>
          {advanced && (
            <label style={lbl}>
              Chains (CAIP-2, comma-separated; blank = all)
              <input
                type="text"
                placeholder="eip155:1, eip155:42161"
                value={chains}
                onChange={(e) => setChains(e.target.value)}
                style={input}
              />
            </label>
          )}
          <button type="submit" disabled={busy === "add"} style={primaryBtn}>
            {busy === "add" ? "Adding…" : "POST /wallets"}
          </button>
          {addError && <p style={{ color: "crimson", margin: 0 }}>{addError}</p>}
        </form>
      </section>

      {/* List */}
      <section style={card}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <h2 style={cardTitle}>Tracked wallets</h2>
          <button onClick={reload} style={smallBtn}>Refresh</button>
        </div>
        {wallets === null && !error && <p style={{ opacity: 0.6 }}>Loading…</p>}
        {wallets && wallets.length === 0 && (
          <p style={{ opacity: 0.6 }}>None yet — add one above.</p>
        )}
        {wallets && wallets.length > 0 && (
          <ul style={{ listStyle: "none", padding: 0, margin: 0 }}>
            {wallets.map((w) => (
              <li
                key={w.address}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 12,
                  padding: "10px 0",
                  borderTop: "1px solid #eee",
                }}
              >
                <div style={{ flex: 1, fontSize: 13 }}>
                  <Link
                    to={`/server/wallets/${w.address}`}
                    style={{ fontFamily: "monospace", color: "#0066cc", textDecoration: "none" }}
                  >
                    {w.address}
                  </Link>
                  <div style={{ fontSize: 11, opacity: 0.6, marginTop: 2 }}>
                    Chains: {w.chains.join(", ") || "(none)"}
                  </div>
                </div>
                <button
                  onClick={() => onSync(w.address)}
                  disabled={busy === `sync:${w.address}`}
                  style={smallBtn}
                >
                  {busy === `sync:${w.address}` ? "Syncing…" : "Sync"}
                </button>
                <Link to={`/server/wallets/${w.address}`} style={smallBtnLink}>
                  Detail
                </Link>
                <button
                  onClick={() => onArchive(w.address)}
                  disabled={busy === `archive:${w.address}`}
                  style={{ ...smallBtn, color: "crimson" }}
                >
                  {busy === `archive:${w.address}` ? "…" : "Archive"}
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

const page: React.CSSProperties = { padding: 24, maxWidth: 720, margin: "0 auto" };
const card: React.CSSProperties = {
  background: "white",
  border: "1px solid #e2e2e6",
  borderRadius: 8,
  padding: 16,
  marginBottom: 16,
};
const cardTitle: React.CSSProperties = { margin: "0 0 12px 0", fontSize: 14, color: "#555" };
const lbl: React.CSSProperties = { display: "grid", gap: 4, fontSize: 12, color: "#555" };
const input: React.CSSProperties = {
  padding: "6px 8px",
  border: "1px solid #ccc",
  borderRadius: 4,
  fontFamily: "monospace",
  fontSize: 13,
};
const primaryBtn: React.CSSProperties = {
  fontSize: 13,
  padding: "8px 14px",
  borderRadius: 4,
  border: "1px solid #0066cc",
  background: "#0066cc",
  color: "white",
  cursor: "pointer",
  justifySelf: "start",
};
const smallBtn: React.CSSProperties = {
  fontSize: 12,
  padding: "4px 10px",
  borderRadius: 4,
  border: "1px solid #888",
  background: "white",
  cursor: "pointer",
};
const smallBtnLink: React.CSSProperties = {
  fontSize: 12,
  padding: "4px 10px",
  borderRadius: 4,
  border: "1px solid #0066cc",
  background: "white",
  color: "#0066cc",
  textDecoration: "none",
};
const inlineLink: React.CSSProperties = {
  background: "transparent",
  border: "none",
  color: "#0066cc",
  cursor: "pointer",
  padding: 0,
  fontSize: 11,
  textDecoration: "underline",
};
