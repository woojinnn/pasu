import { Outlet } from "react-router-dom";

import "./shell.css";

import { NavRail } from "./NavRail";

/**
 * Two-column app frame: persistent NavRail + content slot. Pages own
 * their own topbar (crumb/search/dots) because the breadcrumb varies.
 */
export function AppShell() {
  return (
    <div className="app-frame">
      <NavRail />
      <main className="app-content">
        <Outlet />
      </main>
    </div>
  );
}
