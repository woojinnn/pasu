import { useQuery } from "@tanstack/react-query";
import { listWallets, fetchMe } from "@scopeball/api-client";

/**
 * Phase 0 placeholder. Once `/dashboard/summary` lands in Phase 3 this
 * page will render the workspace summary, chain breakdown, and wallet
 * list. For now it just verifies the auth flow + basic API plumbing.
 */
export function HomePage() {
  const meQ = useQuery({
    queryKey: ["me"],
    queryFn: fetchMe,
    staleTime: Infinity,
  });
  const walletsQ = useQuery({
    queryKey: ["wallets"],
    queryFn: listWallets,
  });

  return (
    <div style={pageStyle}>
      <h1 style={{ marginTop: 0 }}>Home</h1>

      <section style={cardStyle}>
        <h2 style={cardTitleStyle}>Account</h2>
        {meQ.isLoading && <p>Loading…</p>}
        {meQ.error && <p style={{ color: "crimson" }}>Failed: {String(meQ.error)}</p>}
        {meQ.data && (
          <p style={{ margin: 0, fontSize: 13 }}>
            Signed in as <code>{meQ.data.email}</code> (<code>{meQ.data.user_id}</code>)
          </p>
        )}
      </section>

      <section style={cardStyle}>
        <h2 style={cardTitleStyle}>Tracked wallets ({walletsQ.data?.length ?? "…"})</h2>
        {walletsQ.isLoading && <p>Loading…</p>}
        {walletsQ.error && <p style={{ color: "crimson" }}>Failed: {String(walletsQ.error)}</p>}
        {walletsQ.data && walletsQ.data.length === 0 && (
          <p style={{ opacity: 0.6, margin: 0 }}>
            No wallets yet. Add wallet UI lands in Phase 3.
          </p>
        )}
        {walletsQ.data && walletsQ.data.length > 0 && (
          <ul style={{ listStyle: "none", padding: 0, margin: 0 }}>
            {walletsQ.data.map((w) => (
              <li
                key={w.address}
                style={{
                  fontFamily: "monospace",
                  fontSize: 12,
                  padding: "6px 0",
                  borderBottom: "1px solid #eee",
                }}
              >
                {w.address} — chains: {w.chains.join(", ")}
              </li>
            ))}
          </ul>
        )}
      </section>

      <p style={{ opacity: 0.5, fontSize: 12, marginTop: 24 }}>
        Phase 0 scaffold. Editor / Simulation / Monitoring / Audit / History pages
        appear as their backend endpoints ship (Phases 1–5).
      </p>
    </div>
  );
}

const pageStyle: React.CSSProperties = {
  padding: 24,
  maxWidth: 900,
  margin: "0 auto",
};
const cardStyle: React.CSSProperties = {
  background: "white",
  border: "1px solid #e2e2e6",
  borderRadius: 8,
  padding: 16,
  marginBottom: 16,
};
const cardTitleStyle: React.CSSProperties = {
  margin: "0 0 12px 0",
  fontSize: 14,
  color: "#555",
};
