/**
 * `LoginPage` — single "Sign in with Google" button. The router's auth
 * guard redirects unauthenticated users here; on success the OAuth
 * callback lands them on `/auth/callback` which redirects back to the
 * intended page.
 */

import { useEffect } from "react";
import { useNavigate, useLocation } from "react-router-dom";

import { useAuth } from "../hooks/useAuth";

export function LoginPage() {
  const { user, isLoading, login, error } = useAuth();
  const navigate = useNavigate();
  const location = useLocation();

  // Once authenticated, leave /login. The web flow returns via /auth/callback
  // which navigates for us, but the in-extension SW login resolves in place —
  // so the page must react to `user` being set and route to the original
  // destination (or home).
  useEffect(() => {
    if (!user) return;
    const from = (location.state as { from?: string } | null)?.from;
    navigate(from && from !== "/login" ? from : "/", { replace: true });
  }, [user, location.state, navigate]);

  if (isLoading) {
    return (
      <div style={pageStyle}>
        <p>Checking session…</p>
      </div>
    );
  }

  if (user) {
    // Authenticated — the effect above is routing away; brief note meanwhile.
    return (
      <div style={pageStyle}>
        <p>
          Signed in as <strong>{user.email}</strong> — redirecting…
        </p>
      </div>
    );
  }

  return (
    <div style={pageStyle}>
      <h1>Scopeball</h1>
      <p style={{ opacity: 0.7, marginBottom: 24 }}>
        Sign in to view your wallets and activity feed.
      </p>
      <button type="button" onClick={login} style={buttonStyle}>
        Sign in with Google
      </button>
      {error && (
        <p style={{ color: "crimson", marginTop: 16 }}>
          Auth error: {error.message}
        </p>
      )}
    </div>
  );
}

const pageStyle: React.CSSProperties = {
  display: "flex",
  flexDirection: "column",
  alignItems: "center",
  justifyContent: "center",
  minHeight: "60vh",
  textAlign: "center",
  padding: 32,
};

const buttonStyle: React.CSSProperties = {
  fontSize: 16,
  padding: "10px 20px",
  borderRadius: 6,
  border: "1px solid #4285F4",
  background: "#4285F4",
  color: "white",
  cursor: "pointer",
};
