/**
 * `<RequireAuth>` — route guard that gates a subtree behind a valid
 * server JWT.
 *
 * Behaviour:
 * - While `useAuth()` is loading (initial `/auth/me` check), renders a
 *   small placeholder so we don't flash the login page for users with
 *   a valid stored token.
 * - When unauthenticated, redirects to `/login`, remembering the
 *   originally-requested path so the post-login handler can return them.
 * - When authenticated, renders the wrapped `<Outlet />`.
 */

import { Navigate, Outlet, useLocation } from "react-router-dom";

import { useAuth } from "./hooks/useAuth";

export function RequireAuth() {
  const { user, isLoading } = useAuth();
  const location = useLocation();

  if (isLoading) {
    return <div style={{ padding: 32 }}>Loading…</div>;
  }
  if (!user) {
    // Stash the originally-requested URL in state so `LoginPage` can
    // bounce the user back after auth.
    return (
      <Navigate to="/login" replace state={{ from: location.pathname }} />
    );
  }
  return <Outlet />;
}
