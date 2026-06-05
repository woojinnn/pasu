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
  getStoredRefreshToken,
  request,
  setStoredRefreshToken,
  setStoredToken,
} from "./client";
import { sendToExtension, ExtensionBridgeTimeout } from "./extension-bridge";

/**
 * Mirror the dashboard's OAuth token into the SW's `chrome.storage.local`.
 *
 * The dashboard authenticates via the policy-server's `/auth/google` flow,
 * which writes tokens into page `localStorage` (see `client.ts`).
 * The SW reads tokens from `chrome.storage.local` (`scopeball-auth/tokenStore`).
 * Without this bridge the two stores stay out of sync — `recordSimulationOnServer`
 * returns silently at its `hasToken` guard and the HistoryPage's state-diff
 * panel never gets populated.
 *
 * Idempotent: passing the same access/refresh pair is a no-op on the SW side.
 * Fails soft on bridge timeout (the dashboard runs fine without the extension,
 * just no SW-side enforcement). The caller does not await this in a hot path
 * — fire-and-forget at sign-in / refresh time is enough.
 */
async function syncTokensToSw(access: string, refresh: string | null): Promise<void> {
  try {
    await sendToExtension({
      type: "scopeball-auth-sync-tokens",
      access,
      refresh,
    });
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return; // extension not installed
    console.warn("[Scopeball] sync tokens to SW failed:", err);
  }
}

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

  // Mirror the tokens to the SW so its `chrome.storage.local` matches.
  // Fire-and-forget — extension-less dev contexts no-op cleanly inside.
  void syncTokensToSw(access, refresh);

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
  const accessToken = getStoredToken();
  if (!accessToken) return null;
  try {
    const me = await request<Me>("/auth/me");
    // Already-signed-in returning users hit this path on every page load
    // without going through `consumeTokensFromHash`. Re-sync the tokens
    // to the SW so a freshly-installed extension (or one whose
    // `chrome.storage.local` was wiped) catches up to the dashboard's
    // existing session. Idempotent — same tokens just overwrite.
    void syncTokensToSw(accessToken, getStoredRefreshToken());
    return me;
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
