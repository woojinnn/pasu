/**
 * `AuthCallbackPage` — landing page for the OAuth redirect.
 *
 * The server redirects to `${DASHBOARD_URL}/auth/callback#access_token=…`.
 * This page consumes the fragment, persists the token, and forwards the
 * user to the main app.
 */

import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";

import { consumeTokensFromHash } from "../server-api";
import { useAuth } from "../hooks/useAuth";

export function AuthCallbackPage() {
  const { refresh } = useAuth();
  const navigate = useNavigate();
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const token = consumeTokensFromHash();
    if (!token) {
      setError("No token in callback URL fragment.");
      return;
    }
    void refresh().then(() => navigate("/", { replace: true }));
  }, [navigate, refresh]);

  if (error) {
    return (
      <div style={{ padding: 32 }}>
        <h1>Login failed</h1>
        <p>{error}</p>
      </div>
    );
  }
  return (
    <div style={{ padding: 32 }}>
      <p>Completing sign-in…</p>
    </div>
  );
}
