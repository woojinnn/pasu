/**
 * Auth flow for the policy-rpc server (Google OAuth → JWT).
 *
 * The server hosts both endpoints we need (`GET /auth/google` and the
 * `/auth/google/callback` redirect target). The dashboard's job is to
 * (1) kick off the login by sending the browser to the server, and
 * (2) on return, pull the JWT(s) out of the URL fragment.
 *
 * The server redirects back to `${DASHBOARD_URL}/auth/callback#access_token=…&refresh_token=…`.
 * We expose `consumeTokensFromHash()` which the `/auth/callback` route
 * calls on mount: it parses the fragment, persists the tokens, and
 * clears the hash so reloads don't re-trigger.
 */

import {
  SERVER_BASE_URL,
  ServerError,
  getStoredToken,
  request,
  setStoredRefreshToken,
  setStoredToken,
} from "./client";

/** Server's `/auth/me` view of the current user. Mirror of `AuthUser`. */
export interface Me {
  user_id: string;
  email: string;
}

/** Send the browser to the server's Google OAuth start. Full-page nav —
 * we do NOT use `fetch` because OAuth needs cookies / cross-origin
 * redirect handling. */
export function startGoogleLogin(): void {
  window.location.href = `${SERVER_BASE_URL}/auth/google`;
}

/** Pull `access_token` (and optional `refresh_token`) out of the current
 * URL fragment. Persists them and clears the hash. Returns the access
 * token, or `null` when there was nothing to consume. */
export function consumeTokensFromHash(): string | null {
  if (typeof window === "undefined") return null;
  const hash = window.location.hash.startsWith("#")
    ? window.location.hash.slice(1)
    : window.location.hash;
  if (!hash) return null;

  const params = new URLSearchParams(hash);
  const access = params.get("access_token");
  const refresh = params.get("refresh_token");
  if (!access) return null;

  setStoredToken(access);
  if (refresh) setStoredRefreshToken(refresh);

  // Clear the hash without forcing a reload (preserves the rest of the URL).
  window.history.replaceState(
    null,
    "",
    window.location.pathname + window.location.search,
  );
  return access;
}

/** `GET /auth/me` — verifies the stored token. Returns `null` when there
 * is no token; throws `ServerError` (401 if expired). */
export async function fetchMe(): Promise<Me | null> {
  if (!getStoredToken()) return null;
  try {
    return await request<Me>("/auth/me");
  } catch (e) {
    if (e instanceof ServerError && e.isUnauthorized) {
      // Token rejected — drop it so the UI can re-route to login.
      setStoredToken(null);
      setStoredRefreshToken(null);
    }
    throw e;
  }
}

/** Drop tokens locally. (Server-side revocation is future work; HS256
 * stateless tokens expire on their own.) */
export function logout(): void {
  setStoredToken(null);
  setStoredRefreshToken(null);
}
