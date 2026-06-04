/**
 * Google OAuth flow inside the browser extension.
 *
 * Uses `chrome.identity.launchWebAuthFlow` — Chrome opens a popup window
 * pointed at our server's `GET /auth/google`, lets the user complete
 * Google sign-in, and then captures the final redirect URL when the
 * server bounces back. We pull the JWT out of the URL fragment and
 * stash it via `tokenStore`.
 *
 * Why `launchWebAuthFlow` and not just opening a new tab?
 * - It handles closing the popup automatically.
 * - It returns the final redirect URL to us, even if the URL is not
 *   reachable by the browser (we control the redirect target).
 *
 * The redirect target is the dashboard's `/auth/callback` URL by
 * default. The server sends `#access_token=…&refresh_token=…` in the
 * fragment; `launchWebAuthFlow` reports that URL back to us, and we
 * never actually navigate the dashboard tab.
 */

import Browser from "webextension-polyfill";

import { setTokens } from "./tokenStore";
import { getServerBaseUrl } from "./client";

/**
 * Kick off the OAuth flow. Resolves with the user's email on success,
 * rejects with an Error on failure / user cancellation.
 *
 * Note: `chrome.identity.launchWebAuthFlow` requires the manifest to
 * declare the `identity` permission. Firefox supports the same API via
 * `browser.identity.launchWebAuthFlow`; `webextension-polyfill` papers
 * over the difference.
 */
export async function startGoogleLogin(): Promise<{ access: string; refresh: string | null }> {
  const identity = (
    Browser as unknown as {
      identity: {
        getRedirectURL(): string;
        launchWebAuthFlow(opts: { url: string; interactive: boolean }): Promise<string>;
      };
    }
  ).identity;

  // The server must bounce the token back to THIS exact URL — launchWebAuthFlow
  // only resolves on a redirect to `https://<ext-id>.chromiumapp.org/`. That
  // string must be byte-for-byte in the server's OAUTH_ALLOWED_REDIRECT_URIS
  // (trailing slash included); log it so it can be copied into the allowlist.
  const redirectUri = identity.getRedirectURL();
  console.log("[scopeball] OAuth redirect_uri (allowlist this exactly):", redirectUri);

  const url = `${getServerBaseUrl()}/auth/google?redirect_uri=${encodeURIComponent(redirectUri)}`;
  const redirectUrl: string = await identity.launchWebAuthFlow({
    url,
    interactive: true,
  });

  if (!redirectUrl) {
    throw new Error("OAuth flow returned no redirect URL");
  }
  const { access, refresh } = parseTokensFromUrl(redirectUrl);
  if (!access) {
    throw new Error(`OAuth redirect missing access_token: ${redirectUrl}`);
  }
  await setTokens(access, refresh);
  return { access, refresh };
}

/** Pull `access_token` (and optional `refresh_token`) out of a URL
 * fragment. Exported for unit testing. */
export function parseTokensFromUrl(redirectUrl: string): {
  access: string | null;
  refresh: string | null;
} {
  const hashIndex = redirectUrl.indexOf("#");
  if (hashIndex < 0) return { access: null, refresh: null };
  const fragment = redirectUrl.slice(hashIndex + 1);
  const params = new URLSearchParams(fragment);
  return {
    access: params.get("access_token"),
    refresh: params.get("refresh_token"),
  };
}
