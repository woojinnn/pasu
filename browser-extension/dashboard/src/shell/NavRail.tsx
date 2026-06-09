import { useEffect, useRef, useState } from "react";
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
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    if (!menuOpen) return;
    const onDown = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    };
    window.addEventListener("mousedown", onDown);
    return () => window.removeEventListener("mousedown", onDown);
  }, [menuOpen]);

  const onSignOut = () => {
    setMenuOpen(false);
    logout();
    navigate("/login", { replace: true });
  };
  const onProfile = () => {
    setMenuOpen(false);
    navigate("/profile");
  };

  return (
    <nav className="nav-rail" tabIndex={0} aria-label="Pasu global nav">
      <div className="nav-logo">
        <div className="mark">sb</div>
        <div className="word">pasu</div>
      </div>

      <div className="nav-divider" />

      <div className="nav-group">
        <RailItem to="/" end label="Home" icon={<HomeIcon />} />
        <RailItem to="/editor" label="Editor" icon={<EditorIcon />} />
        <RailItem to="/simulation" label="Simulation" icon={<SimIcon />} />
        <RailItem to="/monitoring" label="Assets" icon={<MonIcon />} />
        <RailItem to="/market" label="Market" icon={<MarketIcon />} />
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
        <RailItem to="/settings" label="Settings" icon={<SettingsIcon />} />
      </div>

      <div className="nav-bottom" ref={menuRef}>
        {menuOpen && (
          <div className="nav-usermenu" role="menu">
            <button type="button" className="nav-usermenu-item" onClick={onProfile} role="menuitem">
              <ProfileIcon />
              프로필
            </button>
            <button type="button" className="nav-usermenu-item danger" onClick={onSignOut} role="menuitem">
              <SignOutIcon />
              로그아웃
            </button>
          </div>
        )}
        <button
          className={`nav-user${menuOpen ? " open" : ""}`}
          onClick={() => setMenuOpen((v) => !v)}
          title="계정"
          aria-haspopup="menu"
          aria-expanded={menuOpen}
        >
          <span className="av">{initials}</span>
          <div className="meta">
            <div className="nm">{meQ.data?.email ?? "—"}</div>
            <div className="em">{meQ.data?.user_id ?? ""}</div>
          </div>
          <span className="nav-user-caret"><CaretUpIcon /></span>
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
const MarketIcon = () => (
  <svg viewBox="0 0 24 24" {...stroke}>
    <path d="M3 8h18l-2 12H5z" />
    <path d="M8 8V5a4 4 0 0 1 8 0v3" />
  </svg>
);
const SettingsIcon = () => (
  <svg viewBox="0 0 24 24" {...stroke}>
    <circle cx="12" cy="12" r="3" />
    <path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9 7 7M17 17l2.1 2.1M19.1 4.9 17 7M7 17l-2.1 2.1" />
  </svg>
);
const ProfileIcon = () => (
  <svg viewBox="0 0 24 24" width="15" height="15" {...stroke}>
    <circle cx="12" cy="8" r="3.5" />
    <path d="M5 20c0-3.5 3-6 7-6s7 2.5 7 6" />
  </svg>
);
const SignOutIcon = () => (
  <svg viewBox="0 0 24 24" width="15" height="15" {...stroke}>
    <path d="M15 4h3a1 1 0 0 1 1 1v14a1 1 0 0 1-1 1h-3" />
    <path d="M10 8 6 12l4 4M6 12h11" />
  </svg>
);
const CaretUpIcon = () => (
  <svg viewBox="0 0 24 24" width="14" height="14" {...stroke}>
    <path d="m6 14 6-6 6 6" />
  </svg>
);
