import { useEffect, useState } from "react";
import { Navigate, Outlet } from "react-router-dom";
import { fetchMe, type Me } from "@scopeball/api-client";

/**
 * Phase 0 minimal auth gate. Calls `/auth/me`; on 401 bounces to
 * `/login`. Stores the user in route context via Outlet's default
 * mechanism for now — a proper `AuthContext` lands in Phase 1.
 */
export function RequireAuth() {
  const [state, setState] = useState<
    { kind: "loading" } | { kind: "ok"; me: Me } | { kind: "redirect" }
  >({ kind: "loading" });

  useEffect(() => {
    let cancelled = false;
    fetchMe()
      .then((me) => {
        if (cancelled) return;
        if (me) setState({ kind: "ok", me });
        else setState({ kind: "redirect" });
      })
      .catch(() => {
        if (!cancelled) setState({ kind: "redirect" });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (state.kind === "loading") {
    return <p style={{ padding: 24 }}>Loading…</p>;
  }
  if (state.kind === "redirect") {
    return <Navigate to="/login" replace />;
  }
  return <Outlet />;
}
