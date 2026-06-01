import { useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { Outlet } from "react-router-dom";

import { listPolicies, syncAllPoliciesToExtension } from "../server-api";
import "./shell.css";

import { NavRail } from "./NavRail";

/**
 * Two-column app frame: persistent NavRail + content slot. Pages own
 * their own topbar (crumb/search/dots) because the breadcrumb varies.
 *
 * Side-effect: on every refresh of the policies list (initial mount,
 * post-mutation invalidate, manual refetch) we push the server's view
 * of policies into the extension's chrome.storage.local. Combined with
 * the dual-write in EditorPage mutations, this keeps the popup in step
 * even when policies were created on another device or by a tool that
 * bypassed the UI. Silent if the extension isn't installed.
 */
export function AppShell() {
  // We reuse the same queryKey EditorPage uses so post-save invalidates
  // here too. `enabled` is left at its default — the query runs on mount
  // for every authenticated session.
  const policiesQ = useQuery({ queryKey: ["policies"], queryFn: listPolicies });

  useEffect(() => {
    if (!policiesQ.data) return;
    void syncAllPoliciesToExtension(policiesQ.data);
  }, [policiesQ.data]);

  return (
    <div className="app-frame">
      <NavRail />
      <main className="app-content">
        <Outlet />
      </main>
    </div>
  );
}
