/**
 * Persistent JWT storage for the Scopeball (Rust) policy-rpc server.
 *
 * Stored in `chrome.storage.local` (≈5 MB quota, plenty for a couple of
 * tokens). Reads are memoised after the first lookup so hot-path code
 * (every request adds the `Authorization` header) doesn't pay async
 * cost on each call.
 *
 * This is intentionally separate from the legacy 8787 policy-rpc client
 * — that path stays unauthenticated for now (Phase 8A). Only the new
 * Rust simulation-server at port 8788 needs these tokens.
 */

import Browser from "webextension-polyfill";

const ACCESS_KEY = "scopeball_jwt";
const REFRESH_KEY = "scopeball_jwt_refresh";

let accessCache: string | null | undefined;
let refreshCache: string | null | undefined;

/** Read the access token. `null` when logged out. */
export async function getAccessToken(): Promise<string | null> {
  if (accessCache !== undefined) return accessCache;
  const out = (await Browser.storage.local.get(ACCESS_KEY)) as Record<string, unknown>;
  accessCache = typeof out[ACCESS_KEY] === "string" ? (out[ACCESS_KEY] as string) : null;
  return accessCache;
}

/** Read the refresh token. Optional — fall back to access only. */
export async function getRefreshToken(): Promise<string | null> {
  if (refreshCache !== undefined) return refreshCache;
  const out = (await Browser.storage.local.get(REFRESH_KEY)) as Record<string, unknown>;
  refreshCache = typeof out[REFRESH_KEY] === "string" ? (out[REFRESH_KEY] as string) : null;
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
  accessCache = undefined;
  refreshCache = undefined;
}
