/**
 * `TokensPage` — global token catalog (every row in the `tokens`
 * table, including CoinGecko-sourced metadata when available).
 *
 * Read-only. Token rows are created implicitly by `POST /wallets` /
 * sync paths — this page just surfaces what the DB knows.
 */

import { useCallback, useEffect, useState } from "react";

import { listTokens, type TokenCatalogRow } from "../server-api";

export function TokensPage() {
  const [rows, setRows] = useState<TokenCatalogRow[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");

  const reload = useCallback(async () => {
    try {
      setRows(await listTokens());
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const filtered = (rows ?? []).filter((r) => {
    if (!filter.trim()) return true;
    const needle = filter.trim().toLowerCase();
    return (
      (r.symbol ?? "").toLowerCase().includes(needle) ||
      (r.coingecko_id ?? "").toLowerCase().includes(needle) ||
      JSON.stringify(r.key).toLowerCase().includes(needle)
    );
  });

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
        <h1 style={{ margin: 0 }}>Tokens</h1>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <input
            placeholder="filter symbol / id…"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            style={input}
          />
          <button onClick={reload} style={smallBtn}>
            Reload
          </button>
        </div>
      </header>

      {error && <p style={{ color: "crimson" }}>Failed: {error}</p>}
      {rows === null && !error && <p>Loading…</p>}
      {rows && rows.length === 0 && (
        <p style={{ opacity: 0.6 }}>
          Catalog empty. Add a wallet under <strong>Wallets</strong> and the
          server seeds tokens automatically.
        </p>
      )}
      {rows && rows.length > 0 && (
        <ul style={{ listStyle: "none", padding: 0, margin: 0 }}>
          {filtered.map((t) => (
            <li key={t.token_hash} style={card}>
              <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
                {t.logo_url ? (
                  <img
                    src={t.logo_url}
                    alt={t.symbol ?? ""}
                    width={32}
                    height={32}
                    style={{ borderRadius: 16, background: "#eee" }}
                  />
                ) : (
                  <div
                    style={{
                      width: 32,
                      height: 32,
                      borderRadius: 16,
                      background: "#eee",
                      fontSize: 11,
                      color: "#888",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center",
                    }}
                  >
                    ?
                  </div>
                )}
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontWeight: 600 }}>
                    {t.symbol ?? "(no symbol)"} ·{" "}
                    <span style={{ opacity: 0.6, fontWeight: 400, fontSize: 12 }}>
                      decimals={t.decimals ?? "?"}
                    </span>
                  </div>
                  <div
                    style={{
                      fontFamily: "monospace",
                      fontSize: 11,
                      opacity: 0.55,
                      wordBreak: "break-all",
                    }}
                  >
                    {keyToString(t.key)}
                  </div>
                  {t.description && (
                    <p style={{ margin: "6px 0 0 0", fontSize: 12, opacity: 0.75 }}>
                      {t.description}
                    </p>
                  )}
                </div>
                <div style={{ display: "flex", flexDirection: "column", gap: 4, fontSize: 11 }}>
                  {t.website_url && (
                    <a
                      href={t.website_url}
                      target="_blank"
                      rel="noreferrer"
                      style={{ color: "#0066cc", textDecoration: "none" }}
                    >
                      Website ↗
                    </a>
                  )}
                  {t.coingecko_id && (
                    <a
                      href={`https://www.coingecko.com/en/coins/${t.coingecko_id}`}
                      target="_blank"
                      rel="noreferrer"
                      style={{ color: "#0066cc", textDecoration: "none" }}
                    >
                      CoinGecko ↗
                    </a>
                  )}
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function keyToString(key: unknown): string {
  try {
    const obj = key as Record<string, unknown>;
    if (obj && typeof obj === "object") {
      if ("Erc20" in obj) {
        const v = obj.Erc20 as { chain: string; address: string };
        return `erc20 · ${v.chain} · ${v.address}`;
      }
      if ("Native" in obj) {
        const v = obj.Native as { chain: string };
        return `native · ${v.chain}`;
      }
      if ("Erc721" in obj) {
        const v = obj.Erc721 as { chain: string; contract: string };
        return `erc721 · ${v.chain} · ${v.contract}`;
      }
    }
  } catch {
    /* fall through */
  }
  return JSON.stringify(key);
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
const input: React.CSSProperties = {
  fontSize: 12,
  padding: "4px 8px",
  borderRadius: 4,
  border: "1px solid #bbb",
  background: "white",
  width: 180,
};
