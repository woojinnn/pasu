/**
 * Thin fetch wrapper for the Pasu (Rust) policy-rpc server,
 * service-worker edition.
 *
 * Mirrors `browser-extension/dashboard/src/server-api/client.ts` but:
 * - Reads the JWT from `tokenStore` (chrome.storage) instead of
 *   `localStorage` (which doesn't exist in a service worker).
 * - Doesn't carry the dashboard's `urlWithTokenQuery` SSE helper —
 *   the extension never opens SSE.
 *
 * The server URL comes from `PASU_SERVER_URL` at build time
 * (webpack DefinePlugin) and can be swapped at runtime via
 * `chrome.storage.local["pasu_server_url"]` — the service-worker
 * mirror of the dashboard's `localStorage["pasu_server_url"]`
 * override. Falls back to the local-dev default.
 */

import { getAccessToken, getRefreshToken, setTokens } from "./tokenStore";

declare const PASU_SERVER_URL: string | undefined;

/** Build-time default (webpack DefinePlugin → `PASU_SERVER_URL`). */
const BUILD_BASE_URL =
  (typeof PASU_SERVER_URL !== "undefined" && PASU_SERVER_URL) ||
  "http://127.0.0.1:8788";

/** Runtime override key — mirrors the dashboard's
 * `localStorage["pasu_server_url"]`, in `chrome.storage.local`
 * (service workers have no `localStorage`). */
const SERVER_URL_KEY = "pasu_server_url";

let runtimeBaseUrl: string | null = null;

/** Current server base URL: runtime override (chrome.storage) if set,
 * else the build-time default. Prefer this over `SERVER_BASE_URL` for
 * anything that must respect a live server swap. */
export function getServerBaseUrl(): string {
  return runtimeBaseUrl || BUILD_BASE_URL;
}

/** Back-compat snapshot of the base URL at import time. */
export const SERVER_BASE_URL = BUILD_BASE_URL;

// Load any runtime override once at SW startup, then keep it live.
if (typeof chrome !== "undefined" && chrome.storage?.local) {
  void chrome.storage.local.get(SERVER_URL_KEY).then((r) => {
    const v = r[SERVER_URL_KEY];
    if (typeof v === "string" && v) runtimeBaseUrl = v;
  });
  chrome.storage.onChanged.addListener((changes, area) => {
    if (area === "local" && changes[SERVER_URL_KEY]) {
      const v = changes[SERVER_URL_KEY].newValue;
      runtimeBaseUrl = typeof v === "string" && v ? v : null;
    }
  });
}

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
  const res = await fetch(`${getServerBaseUrl()}/auth/refresh`, {
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
  const url = path.startsWith("http") ? path : `${getServerBaseUrl()}${path}`;
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
    const text = await res.text().catch(() => "");
    let body: unknown = null;
    if (text) {
      try {
        body = JSON.parse(text);
      } catch {
        body = text;
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

/**
 * `POST /wallets/:address/permits` body — a decoded off-chain permit/permit2
 * signature. Mirrors the server's `IngestPermitReq` tagged union
 * (`crates/policy-server/server/src/write_handlers.rs`). Decoded params only;
 * the raw EIP-712 signature is intentionally NOT sent (less sensitive — the
 * reconciler needs nonce/deadline, not the sig).
 */
export type IngestPermitReq =
  | {
      kind: "eip2612";
      token: string;
      spender: string;
      amount: string;
      deadline: number;
      nonce: string;
      chain_id: string;
    }
  | {
      kind: "permit2_allowance";
      token: string;
      spender: string;
      amount: string;
      expires_at: number;
      sig_deadline: number;
      nonce_word: string;
      nonce_bit: number;
      chain_id: string;
    }
  | {
      kind: "permit2_transfer";
      token: string;
      owner: string;
      spender: string;
      amount: string;
      sig_deadline: number;
      nonce_word: string;
      nonce_bit: number;
      witness_type?: string | null;
      chain_id: string;
    };

export interface IngestPermitResp {
  pending_ids: string[];
}

/**
 * `POST /wallets/:address/permits` — record a signed permit/permit2 the
 * extension just observed, so the server tracks it as a `PendingTx` (and the
 * sync reconciler later closes its lifecycle). Best-effort at the call site;
 * errors surface as `ServerError` for the caller to swallow.
 */
export async function ingestPermit(
  address: string,
  req: IngestPermitReq,
): Promise<IngestPermitResp> {
  return request<IngestPermitResp>(`/wallets/${address}/permits`, {
    method: "POST",
    body: req,
  });
}
