/**
 * `<ServerLayout>` — shared shell for the `/server/*` subtree.
 *
 * Renders a tiny top nav (Wallets / Activity / Me) plus a sign-out
 * shortcut, then the matched route via `<Outlet />`. Keeps the dashboard
 * navigation explicit so a fresh user can see every endpoint that the
 * Phase 7-9 work surfaced without hand-typing URLs.
 */

import { NavLink, Outlet, useNavigate } from "react-router-dom";

import { useAuth } from "./hooks/useAuth";
import { SERVER_BASE_URL, getStoredToken } from "./server-api";

export function ServerLayout() {
  const { user, logout } = useAuth();
  const navigate = useNavigate();

  const onSignOut = () => {
    logout();
    navigate("/login", { replace: true });
  };

  // Pass the JWT via URL fragment so Swagger UI can pre-fill the
  // Authorize button — saves a manual copy-paste before "Try it out".
  const docsUrl = (() => {
    const t = getStoredToken();
    const base = `${SERVER_BASE_URL}/docs`;
    return t ? `${base}#token=${encodeURIComponent(t)}` : base;
  })();

  return (
    <div style={{ minHeight: "100vh", background: "#f7f7f8" }}>
      <header
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "10px 20px",
          background: "white",
          borderBottom: "1px solid #e2e2e6",
          boxShadow: "0 1px 2px rgba(0,0,0,0.03)",
        }}
      >
        <nav style={{ display: "flex", gap: 4 }}>
          <Brand />
          <NavBtn to="/server/wallets" label="Wallets" />
          <NavBtn to="/server/transactions" label="Transactions" />
          <NavBtn to="/server/policies" label="Policies" />
          <NavBtn to="/server/tokens" label="Tokens" />
          <NavBtn to="/server/activity" label="Activity" />
          <NavBtn to="/server/me" label="Me" />
        </nav>
        <div style={{ display: "flex", alignItems: "center", gap: 12, fontSize: 13 }}>
          <a
            href={docsUrl}
            target="_blank"
            rel="noreferrer"
            style={{ fontSize: 12, color: "#0066cc", textDecoration: "none" }}
            title="OpenAPI / Swagger UI (with auto-auth)"
          >
            API docs ↗
          </a>
          <span style={{ opacity: 0.7 }}>{user?.email}</span>
          <button onClick={onSignOut} style={signOutStyle}>
            Sign out
          </button>
        </div>
      </header>
      <main>
        <Outlet />
      </main>
    </div>
  );
}

function Brand() {
  return (
    <span
      style={{
        fontWeight: 700,
        fontSize: 14,
        padding: "6px 12px",
        color: "#111",
        marginRight: 6,
      }}
    >
      🛡 Scopeball
    </span>
  );
}

function NavBtn({ to, label }: { to: string; label: string }) {
  return (
    <NavLink
      to={to}
      style={({ isActive }) => ({
        fontSize: 13,
        padding: "6px 12px",
        borderRadius: 6,
        textDecoration: "none",
        color: isActive ? "#0066cc" : "#333",
        background: isActive ? "#e8f0ff" : "transparent",
        fontWeight: isActive ? 600 : 400,
      })}
    >
      {label}
    </NavLink>
  );
}

const signOutStyle: React.CSSProperties = {
  fontSize: 12,
  padding: "4px 10px",
  borderRadius: 4,
  border: "1px solid #888",
  background: "white",
  cursor: "pointer",
};
