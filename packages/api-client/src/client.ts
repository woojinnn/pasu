/**
 * Thin fetch wrapper for the policy-rpc server (simulation-server).
 *
 * Responsibilities:
 * - Prepend the server base URL (configurable; defaults to localhost).
 * - Attach `Authorization: Bearer <jwt>` automatically when a token is
 *   stored.
 * - Throw structured `ServerError` on non-2xx so callers can match on
 *   status (e.g. trigger re-login on 401).
 * - Stay tiny and dependency-free — no axios, no React.
 */

const DEFAULT_BASE = "http://127.0.0.1:8788";

/** Resolve the server URL — env > localStorage > default. Read once at
 * import time; we don't expect users to swap servers mid-session. */
function resolveBaseUrl(): string {
  if (typeof window !== "undefined") {
    const stored = window.localStorage.getItem("scopeball_server_url");
    if (stored) return stored;
  }
  return DEFAULT_BASE;
}

export const SERVER_BASE_URL = resolveBaseUrl();

const TOKEN_KEY = "scopeball_jwt";
const REFRESH_KEY = "scopeball_jwt_refresh";

/** Persisted access token. Returns `null` when the user is logged out. */
export function getStoredToken(): string | null {
  if (typeof window === "undefined") return null;
  return window.localStorage.getItem(TOKEN_KEY);
}

export function getStoredRefreshToken(): string | null {
  if (typeof window === "undefined") return null;
  return window.localStorage.getItem(REFRESH_KEY);
}

export function setStoredToken(token: string | null): void {
  if (typeof window === "undefined") return;
  if (token === null) window.localStorage.removeItem(TOKEN_KEY);
  else window.localStorage.setItem(TOKEN_KEY, token);
}

export function setStoredRefreshToken(token: string | null): void {
  if (typeof window === "undefined") return;
  if (token === null) window.localStorage.removeItem(REFRESH_KEY);
  else window.localStorage.setItem(REFRESH_KEY, token);
}

/** Error surfaced by every server-api call on non-2xx. */
export class ServerError extends Error {
  public readonly status: number;
  public readonly body: unknown;

  constructor(status: number, message: string, body: unknown) {
    super(message);
    this.name = "ServerError";
    this.status = status;
    this.body = body;
  }

  /** Convenience predicate — most callers re-trigger login on 401. */
  get isUnauthorized(): boolean {
    return this.status === 401;
  }
}

/** Options for `request()`. `body` is JSON-stringified for you. */
export interface RequestOptions {
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  body?: unknown;
  /** Override the stored token (e.g. during login when nothing is persisted yet). */
  token?: string | null;
  /** Skip the `Authorization` header entirely (for `/auth/google` style routes). */
  noAuth?: boolean;
  signal?: AbortSignal;
}

/** Core request primitive. Returns parsed JSON. Throws `ServerError`. */
export async function request<T>(path: string, opts: RequestOptions = {}): Promise<T> {
  const url = path.startsWith("http") ? path : `${SERVER_BASE_URL}${path}`;
  const headers: Record<string, string> = {};
  if (opts.body !== undefined) headers["Content-Type"] = "application/json";

  if (!opts.noAuth) {
    const token = opts.token ?? getStoredToken();
    if (token) headers["Authorization"] = `Bearer ${token}`;
  }

  const res = await fetch(url, {
    method: opts.method ?? "GET",
    headers,
    body: opts.body !== undefined ? JSON.stringify(opts.body) : undefined,
    signal: opts.signal,
  });

  if (!res.ok) {
    let body: unknown = null;
    try {
      body = await res.json();
    } catch {
      // not JSON — fall back to text
      try {
        body = await res.text();
      } catch {
        /* leave body null */
      }
    }
    throw new ServerError(res.status, `${res.status} ${res.statusText}`, body);
  }
  // 204 No Content
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

/** Build a URL with `?token=…` for `EventSource` (which can't set headers). */
export function urlWithTokenQuery(path: string, token: string): string {
  const sep = path.includes("?") ? "&" : "?";
  return `${SERVER_BASE_URL}${path}${sep}token=${encodeURIComponent(token)}`;
}
