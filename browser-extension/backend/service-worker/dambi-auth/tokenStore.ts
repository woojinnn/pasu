/**
 * Persistent JWT storage for the Dambi (Rust) policy-rpc server.
 *
 * Stored in `chrome.storage.local` (≈5 MB quota, plenty for a couple of
 * tokens). A NON-null token is memoised after the first lookup so hot-path
 * code (every request adds the `Authorization` header) doesn't pay async
 * cost on each call.
 *
 * We deliberately do NOT cache a `null` (logged-out) read. The dambi-rename
 * storage migration copies legacy access tokens to `dambi_jwt` on SW boot; if a
 * token read raced ahead of that `set` we'd otherwise poison the cache with
 * `null` for the whole SW lifetime and show the user logged out until the SW
 * recycled. Caching only real tokens means a later read (after the migration
 * lands) still hits storage and succeeds.
 *
 * This is intentionally separate from the legacy 8787 policy-rpc client
 * — that path remains unauthenticated. Only the new
 * Rust policy-server at port 8788 needs these tokens.
 */

import Browser from "webextension-polyfill";

const ACCESS_KEY = "dambi_jwt";
const REFRESH_KEY = "dambi_jwt_refresh";

// Only ever holds a real token: a logged-out / not-yet-migrated read leaves
// the cache `null` so it stays a cache MISS and re-reads storage next time.
let accessCache: string | null = null;
let refreshCache: string | null = null;

/** Read the access token. `null` when logged out. */
export async function getAccessToken(): Promise<string | null> {
  if (accessCache !== null) return accessCache;
  const out = (await Browser.storage.local.get(ACCESS_KEY)) as Record<string, unknown>;
  // Cache only a real token; never memoise the empty/logged-out result.
  if (typeof out[ACCESS_KEY] === "string") accessCache = out[ACCESS_KEY] as string;
  return accessCache;
}

/** Read the refresh token. Optional — fall back to access only. */
export async function getRefreshToken(): Promise<string | null> {
  if (refreshCache !== null) return refreshCache;
  const out = (await Browser.storage.local.get(REFRESH_KEY)) as Record<string, unknown>;
  // Cache only a real token; never memoise the empty/logged-out result.
  if (typeof out[REFRESH_KEY] === "string") refreshCache = out[REFRESH_KEY] as string;
  return refreshCache;
}

/** Persist access + refresh tokens. Either can be `null` to drop. */
export async function setTokens(access: string | null, refresh: string | null = null): Promise<void> {
  accessCache = access;
  refreshCache = refresh;
  const toSet: Record<string, string> = {};
  const toRemove: string[] = [];
  if (access === null) toRemove.push(ACCESS_KEY);
  else toSet[ACCESS_KEY] = access;
  if (refresh === null) toRemove.push(REFRESH_KEY);
  else toSet[REFRESH_KEY] = refresh;
  if (Object.keys(toSet).length) await Browser.storage.local.set(toSet);
  if (toRemove.length) await Browser.storage.local.remove(toRemove);
}

/** Drop both tokens. Used by sign-out flow. */
export async function clearTokens(): Promise<void> {
  await setTokens(null, null);
}

/** Test hook — reset the in-memory cache so unit tests see fresh
 * storage reads. */
export function _resetCacheForTests(): void {
  accessCache = null;
  refreshCache = null;
}
