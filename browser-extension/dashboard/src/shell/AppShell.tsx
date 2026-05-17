import { useState } from "react";
import { NavLink, Outlet } from "react-router-dom";
import { OnboardingTour } from "../onboarding/OnboardingTour";
import { useExtension } from "../sdk-context";
import "./AppShell.css";

const NAV_ITEMS: Array<{ to: string; label: string; end?: boolean }> = [
  { to: "/", label: "Home", end: true },
  { to: "/editor", label: "Editor" },
  { to: "/library", label: "Library" },
  { to: "/audit", label: "Audit" },
  { to: "/settings", label: "Settings" },
];

export function AppShell() {
  const [forceHelp, setForceHelp] = useState(false);
  return (
    <div className="shell">
      <header className="shell-header">
        <div className="brand">
          <div className="brand-mark">SC</div>
          <div className="brand-text">
            <div className="brand-name">Scopeball</div>
            <div className="brand-tagline">Cedar policy gate</div>
          </div>
        </div>
        <nav className="shell-nav">
          {NAV_ITEMS.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.end ?? false}
              className={({ isActive }) =>
                "nav-link" + (isActive ? " active" : "")
              }
            >
              {item.label}
            </NavLink>
          ))}
        </nav>
        <div className="shell-right">
          <button
            type="button"
            className="help-btn"
            onClick={() => setForceHelp(true)}
            title="가이드 열기"
            aria-label="가이드 열기"
          >
            ?
          </button>
          <ConnectionPill />
        </div>
      </header>
      <main className="shell-main">
        <Outlet />
      </main>
      <OnboardingTour
        forceOpen={forceHelp}
        onClose={() => setForceHelp(false)}
      />
    </div>
  );
}

function ConnectionPill() {
  const { status, refresh } = useExtension();
  let label: string;
  let cls: string;
  if (status.kind === "connecting") {
    label = "연결 중…";
    cls = "pending";
  } else if (status.kind === "connected") {
    label = `Extension v${status.version}`;
    cls = "ok";
  } else {
    label = "Extension 미연결";
    cls = "err";
  }
  return (
    <button
      type="button"
      className={`conn-pill ${cls}`}
      onClick={() => void refresh()}
      title={status.kind === "error" ? status.message : ""}
    >
      {label}
    </button>
  );
}
