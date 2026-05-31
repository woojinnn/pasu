import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { consumeTokensFromHash } from "@scopeball/api-client";

/**
 * Hit by the server's OAuth redirect with `#access_token=…`. Pulls the
 * tokens out of the hash, stores them, and bounces home.
 */
export function AuthCallbackPage() {
  const navigate = useNavigate();
  useEffect(() => {
    const token = consumeTokensFromHash();
    navigate(token ? "/" : "/login", { replace: true });
  }, [navigate]);
  return <p style={{ padding: 24 }}>Signing in…</p>;
}
