/**
 * `LoginPage` — single "Sign in with Google" button. The router's auth
 * guard redirects unauthenticated users here; on success the OAuth
 * callback lands them on `/auth/callback` which redirects back to the
 * intended page.
 */

import { useAuth } from "../hooks/useAuth";

export function LoginPage() {
  const { user, isLoading, login, error } = useAuth();

  if (isLoading) {
    return (
      <div style={pageStyle}>
        <p>Checking session…</p>
      </div>
    );
  }

  if (user) {
    // Shouldn't happen in normal flow — the router guard sends logged-in
    // users to "/". Render a soft fallback just in case.
    return (
      <div style={pageStyle}>
        <p>
          Already signed in as <strong>{user.email}</strong>.
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
