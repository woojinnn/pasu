/**
 * Thin fetch wrapper for the Scopeball (Rust) policy-rpc server,
 * service-worker edition.
 *
 * Mirrors `browser-extension/dashboard/src/server-api/client.ts` but:
 * - Reads the JWT from `tokenStore` (chrome.storage) instead of
 *   `localStorage` (which doesn't exist in a service worker).
 * - Doesn't carry the dashboard's `urlWithTokenQuery` SSE helper —
 *   the extension never opens SSE.
 *
 * The server URL is taken from `SCOPEBALL_SERVER_URL` at build time
 * (webpack DefinePlugin in Phase 9) and falls back to the local-dev
 * default.
 */

import { getAccessToken, getRefreshToken, setTokens } from "./tokenStore";

declare const SCOPEBALL_SERVER_URL: string | undefined;

export const SERVER_BASE_URL =
  (typeof SCOPEBALL_SERVER_URL !== "undefined" && SCOPEBALL_SERVER_URL) ||
  "http://127.0.0.1:8788";

export class ServerError extends Error {
  public readonly status: number;
  public readonly body: unknown;
  constructor(status: number, message: string, body: unknown) {
    super(message);
    this.name = "ServerError";
    this.status = status;
    this.body = body;
  }
  get isUnauthorized(): boolean {
    return this.status === 401;
  }
}

export interface RequestOptions {
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  body?: unknown;
  token?: string | null;
  noAuth?: boolean;
  signal?: AbortSignal;
}

interface RefreshResponse {
  access_token: string;
  refresh_token?: string;
}

async function refreshAccessToken(): Promise<string | null> {
  const refresh = await getRefreshToken();
  if (!refresh) return null;
  const res = await fetch(`${SERVER_BASE_URL}/auth/refresh`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ refresh_token: refresh }),
  });
  if (!res.ok) {
    await setTokens(null, null);
    return null;
  }
  const body = (await res.json()) as RefreshResponse;
  await setTokens(body.access_token, body.refresh_token ?? refresh);
  return body.access_token;
}

/** Core request primitive. Returns parsed JSON. Throws `ServerError`. */
export async function request<T>(path: string, opts: RequestOptions = {}): Promise<T> {
  const url = path.startsWith("http") ? path : `${SERVER_BASE_URL}${path}`;
  const headers: Record<string, string> = {};
  if (opts.body !== undefined) headers["Content-Type"] = "application/json";

  if (!opts.noAuth) {
    const token = opts.token ?? (await getAccessToken());
    if (token) headers["Authorization"] = `Bearer ${token}`;
  }

  const init: RequestInit = {
    method: opts.method ?? "GET",
    headers,
  };
  if (opts.body !== undefined) init.body = JSON.stringify(opts.body);
  if (opts.signal !== undefined) init.signal = opts.signal;
  const res = await fetch(url, init);

  if (res.status === 401 && !opts.noAuth) {
    const refreshed = await refreshAccessToken();
    if (refreshed) {
      headers["Authorization"] = `Bearer ${refreshed}`;
      const retry = await fetch(url, { ...init, headers });
      return parseResponse<T>(retry);
    }
  }
  return parseResponse<T>(res);
}

async function parseResponse<T>(res: Response): Promise<T> {
  if (!res.ok) {
    let body: unknown = null;
    try {
      body = await res.json();
    } catch {
      try {
        body = await res.text();
      } catch {
        /* leave body null */
      }
    }
    throw new ServerError(res.status, `${res.status} ${res.statusText}`, body);
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

// ---------- typed helpers ----------

export interface Me {
  user_id: string;
  email: string;
}

export interface WalletId {
  address: string;
  chains: string[];
}

/** `GET /auth/me` — current user (or `null` if no token / 401). */
export async function fetchMe(): Promise<Me | null> {
  if (!(await getAccessToken())) return null;
  try {
    return await request<Me>("/auth/me");
  } catch (e) {
    if (e instanceof ServerError && e.isUnauthorized) return null;
    throw e;
  }
}

/** `GET /wallets` — user's tracked wallets. */
export async function listWallets(): Promise<WalletId[]> {
  return request<WalletId[]>("/wallets");
}

/**
 * `POST /evaluate` — simulate the given action envelopes against the
 * authenticated user's wallet state. The new Rust server persists the
 * resulting delta so the dashboard sees the history; the SW still runs
 * WASM Cedar for the actual verdict.
 *
 * Returns the server's `policyRequest` (state_before / deltas /
 * state_after) and `diagnostics`. Errors are surfaced as `ServerError`.
 *
 * Wire shape mirrors `crates/policy-server/server/src/dto.rs`. Types are
 * kept loose (`Record<string, unknown>`) because the action / context
 * payloads are opaque to the SW — only the server (and WASM Cedar)
 * needs to interpret them.
 */
export interface EvaluateRequestDto {
  wallet_id: WalletId;
  envelopes: ReadonlyArray<Record<string, unknown>>;
  eval_context: Record<string, unknown>;
  call_specs?: ReadonlyArray<Record<string, unknown>>;
}

export interface EvaluateResponseDto {
  policyRequest: Record<string, unknown>;
  diagnostics?: ReadonlyArray<Record<string, unknown>>;
}

export async function evaluate(req: EvaluateRequestDto): Promise<EvaluateResponseDto> {
  return request<EvaluateResponseDto>("/evaluate", {
    method: "POST",
    body: req,
  });
}
