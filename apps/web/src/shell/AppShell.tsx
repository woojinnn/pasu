import { NavLink, Outlet, useNavigate } from "react-router-dom";
import { logout } from "@scopeball/api-client";

/**
 * Minimal shell — header nav + content. Phase 0 only wires Home;
 * Editor / Simulation / Monitoring / Audit / History links are
 * disabled placeholders so the nav structure is visible from day 1.
 */
export function AppShell() {
  const navigate = useNavigate();
  const onSignOut = () => {
    logout();
    navigate("/login", { replace: true });
  };

  return (
    <div style={{ minHeight: "100vh", background: "#f7f7f8" }}>
      <header style={headerStyle}>
        <nav style={{ display: "flex", gap: 4 }}>
          <span style={brandStyle}>🛡 Scopeball</span>
          <NavBtn to="/" label="Home" end />
          <NavBtnDisabled label="Editor" />
          <NavBtnDisabled label="Simulation" />
          <NavBtnDisabled label="Monitoring" />
          <NavBtnDisabled label="Audit" />
          <NavBtnDisabled label="History" />
        </nav>
        <button onClick={onSignOut} style={signOutStyle}>
          Sign out
        </button>
      </header>
      <main>
        <Outlet />
      </main>
    </div>
  );
}

function NavBtn({ to, label, end }: { to: string; label: string; end?: boolean }) {
  return (
    <NavLink
      to={to}
      end={end}
      style={({ isActive }) => ({
        ...navLinkStyle,
        color: isActive ? "#0066cc" : "#333",
        background: isActive ? "#e8f0ff" : "transparent",
        fontWeight: isActive ? 600 : 400,
      })}
    >
      {label}
    </NavLink>
  );
}

function NavBtnDisabled({ label }: { label: string }) {
  return (
    <span style={{ ...navLinkStyle, color: "#aaa", cursor: "not-allowed" }} title="coming soon">
      {label}
    </span>
  );
}

const headerStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  padding: "10px 20px",
  background: "white",
  borderBottom: "1px solid #e2e2e6",
};
const brandStyle: React.CSSProperties = {
  fontWeight: 700,
  fontSize: 14,
  padding: "6px 12px",
  marginRight: 6,
};
const navLinkStyle: React.CSSProperties = {
  fontSize: 13,
  padding: "6px 12px",
  borderRadius: 6,
  textDecoration: "none",
};
const signOutStyle: React.CSSProperties = {
  fontSize: 12,
  padding: "4px 10px",
  borderRadius: 4,
  border: "1px solid #888",
  background: "white",
  cursor: "pointer",
};
