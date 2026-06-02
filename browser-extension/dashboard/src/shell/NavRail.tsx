import { NavLink, useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";

import { fetchMe, logout, listFindings } from "../server-api";

/**
 * Persistent left nav. Hover/focus expands to 256px (CSS-driven, no JS state).
 * Findings count drives the History badge — refetched every 30s.
 */
export function NavRail() {
  const navigate = useNavigate();
  const meQ = useQuery({ queryKey: ["me"], queryFn: fetchMe, staleTime: Infinity });
  const findingsQ = useQuery({
    queryKey: ["findings", "unresolved-count"],
    queryFn: () => listFindings({ limit: 50 }),
    refetchInterval: 30_000,
  });
  const pendingCount = findingsQ.data?.filter((f) => f.user_decision === null).length ?? 0;

  const initials = (meQ.data?.email ?? "??").slice(0, 2).toUpperCase();
  const onSignOut = () => {
    logout();
    navigate("/login", { replace: true });
  };

  return (
    <nav className="nav-rail" tabIndex={0} aria-label="Scopeball global nav">
      <div className="nav-logo">
        <div className="mark">sb</div>
        <div className="word">scopeball</div>
      </div>

      <div className="nav-divider" />

      <div className="nav-group">
        <RailItem to="/" end label="Home" icon={<HomeIcon />} />
        <RailItem to="/editor" label="Editor" icon={<EditorIcon />} />
        <RailItem to="/simulation" label="Simulation" icon={<SimIcon />} />
        <RailItem to="/monitoring" label="Monitoring" icon={<MonIcon />} />
      </div>

      <div className="nav-divider" />

      <div className="nav-group">
        <RailItem
          to="/history"
          label="History"
          icon={<HistoryIcon />}
          badge={pendingCount > 0 ? String(pendingCount) : undefined}
          showDot={pendingCount > 0}
        />
      </div>

      <div className="nav-bottom">
        <button className="nav-user" onClick={onSignOut} title="Sign out">
          <span className="av">{initials}</span>
          <div className="meta">
            <div className="nm">{meQ.data?.email ?? "—"}</div>
            <div className="em">{meQ.data?.user_id ?? ""}</div>
          </div>
        </button>
      </div>
    </nav>
  );
}

interface RailItemProps {
  to: string;
  label: string;
  icon: React.ReactNode;
  end?: boolean;
  disabled?: boolean;
  badge?: string;
  showDot?: boolean;
}

function RailItem({ to, label, icon, end, disabled, badge, showDot }: RailItemProps) {
  if (disabled) {
    return (
      <span className="nav-item disabled" title="준비 중">
        <span className="icon">{icon}</span>
        <span className="label">{label}</span>
      </span>
    );
  }
  return (
    <NavLink to={to} end={end} className={({ isActive }) => `nav-item${isActive ? " active" : ""}`}>
      <span className="icon">{icon}</span>
      <span className="label">{label}</span>
      {badge && <span className="badge">{badge}</span>}
      {showDot && !badge && <span className="dot-badge" />}
    </NavLink>
  );
}

// ── icons (stroked, 18×18) ──────────────────────────────────────────────
const stroke = { fill: "none", stroke: "currentColor", strokeWidth: 1.8, strokeLinecap: "round" as const, strokeLinejoin: "round" as const };
const HomeIcon = () => (
  <svg viewBox="0 0 24 24" {...stroke}>
    <path d="M3 11.5 12 4l9 7.5" />
    <path d="M5 10v10h14V10" />
  </svg>
);
const EditorIcon = () => (
  <svg viewBox="0 0 24 24" {...stroke}>
    <rect x="3" y="3" width="7" height="7" rx="1.5" />
    <rect x="14" y="3" width="7" height="7" rx="1.5" />
    <rect x="3" y="14" width="7" height="7" rx="1.5" />
    <rect x="14" y="14" width="7" height="7" rx="1.5" />
  </svg>
);
const SimIcon = () => (
  <svg viewBox="0 0 24 24" {...stroke}>
    <circle cx="12" cy="12" r="9" />
    <path d="m10 8.5 5 3.5-5 3.5z" />
  </svg>
);
const MonIcon = () => (
  <svg viewBox="0 0 24 24" {...stroke}>
    <path d="M3 12h4l3 8 4-16 3 8h4" />
  </svg>
);
const HistoryIcon = () => (
  <svg viewBox="0 0 24 24" {...stroke}>
    <path d="M3 3v18h18" />
    <path d="m7 14 4-4 4 3 5-7" />
  </svg>
);
