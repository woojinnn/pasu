/**
 * Phase 2B — Registry HTTP client.
 *
 * Spec: `ADAPTER_MARKETPLACE_ARCHITECTURE.md` §6.1 (index URL pattern) and
 * §7.3 (JIT fetch flow).
 *
 * Responsibilities:
 *   - Construct the deterministic callkey URL the Phase 2A `build-index.ts`
 *     produces: `<baseUrl>/index/by-callkey/<chainId>__<to>__<selector>.json`
 *     with `to` / `selector` always lowercased.
 *   - Race the fetch against a hard timeout (default 2 s, AbortController).
 *   - Surface uniform `RegistryError` codes so the JIT fetcher can branch
 *     into the four spec-mandated negative-cache reasons without sniffing
 *     error messages.
 *
 * Out of scope (deferred):
 *   - HMAC key transformation (§7.5) — IndexedDB only, not the wire.
 *   - Mirror / OHTTP routing (§7.4 future row).
 *   - Layer 2 prefetch — Phase 4+.
 */
import type { AdapterFunctionBundle } from "../marketplace/bundle-schema";

/** Input to `byCallKey` — mirrors the Rust `CallMatchKey`. */
export interface CallMatchKey {
  chain_id: number;
  to: string;
  /** "0x" + 8 hex chars. Case is normalised before URL construction. */
  selector: string;
}

/**
 * Registry index response (§6.1). On 200 OK with `matched: true` the
 * `bundle` field is the inline Adapter Function Bundle JSON so the client
 * gets a 1-RTT lookup. A 404 surfaces as `RegistryError("not_found", …)`
 * rather than a `matched: false` 200, by spec — see Phase 2A
 * `build-index.ts` (file absence = natural 404).
 */
export interface ByCallKeyOk {
  matched: true;
  bundle_id: string;
  manifest_path: string;
  bundle_sha256: string;
  bundle: AdapterFunctionBundle;
}

export interface ByCallKeyOptions {
  /** Default `http://localhost:8000` for the PoC static server. */
  baseUrl?: string;
  /** Default 2000 ms per §7.3. */
  timeoutMs?: number;
  /** Injected for tests — defaults to global `fetch`. */
  fetchImpl?: typeof fetch;
}

export type RegistryErrorCode =
  | "not_found"
  | "timeout"
  | "network"
  | "malformed_response";

export class RegistryError extends Error {
  constructor(
    readonly code: RegistryErrorCode,
    message: string,
    /** HTTP status when applicable (404 / 5xx / undefined for network/timeout). */
    readonly status?: number,
    options?: { cause?: unknown },
  ) {
    super(`registry[${code}] ${message}`);
    this.name = "RegistryError";
    if (options?.cause !== undefined) {
      // Standard `Error.cause` is supported by V8 / SpiderMonkey; we still
      // store on `this` for older runtimes that ignore the second arg.
      (this as { cause?: unknown }).cause = options.cause;
    }
  }
}

const DEFAULT_BASE_URL = "http://localhost:8000";
const DEFAULT_TIMEOUT_MS = 2000;

/**
 * Build the callkey index URL. `to` and `selector` are lowercased to mirror
 * `build-index.ts`'s `callkeyFilename` — a static file server is
 * case-sensitive on most platforms, so the client must normalise too.
 */
export function callKeyUrl(baseUrl: string, key: CallMatchKey): string {
  const to = key.to.toLowerCase();
  const sel = key.selector.toLowerCase();
  // Strip a trailing slash on the base so we never double-up.
  const base = baseUrl.endsWith("/") ? baseUrl.slice(0, -1) : baseUrl;
  return `${base}/index/by-callkey/${key.chain_id}__${to}__${sel}.json`;
}

/**
 * GET the index entry for `key`. Resolves to the parsed JSON on 200 OK,
 * rejects with `RegistryError` on anything else.
 *
 * Failure mapping (consumed by jit-fetcher to derive negative-cache reasons):
 *   - HTTP 404           → `RegistryError("not_found")`   → `no_publisher` (5 min)
 *   - AbortError/timeout → `RegistryError("timeout")`     → `timeout` (30 s)
 *   - fetch reject       → `RegistryError("network")`     → `timeout` (30 s)
 *   - non-200 / non-404  → `RegistryError("network")`     → `timeout` (30 s)
 *   - bad JSON           → `RegistryError("malformed_response")` → `timeout` (30 s)
 */
export async function byCallKey(
  key: CallMatchKey,
  options: ByCallKeyOptions = {},
): Promise<ByCallKeyOk> {
  const baseUrl = options.baseUrl ?? DEFAULT_BASE_URL;
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const doFetch = options.fetchImpl ?? fetch;

  const url = callKeyUrl(baseUrl, key);

  // AbortController is the only way to cancel an in-flight fetch — Promise
  // racing alone would leak the network request after the timeout fires.
  const controller = new AbortController();
  const timeoutHandle = setTimeout(() => controller.abort(), timeoutMs);

  let response: Response;
  try {
    response = await doFetch(url, { signal: controller.signal });
  } catch (err) {
    clearTimeout(timeoutHandle);
    if (isAbortError(err)) {
      throw new RegistryError("timeout", `${url}: aborted after ${timeoutMs}ms`, undefined, {
        cause: err,
      });
    }
    throw new RegistryError(
      "network",
      `${url}: ${err instanceof Error ? err.message : String(err)}`,
      undefined,
      { cause: err },
    );
  }
  clearTimeout(timeoutHandle);

  if (response.status === 404) {
    throw new RegistryError("not_found", `${url}: 404`, 404);
  }
  if (!response.ok) {
    throw new RegistryError(
      "network",
      `${url}: HTTP ${response.status}`,
      response.status,
    );
  }

  let parsed: unknown;
  try {
    parsed = await response.json();
  } catch (err) {
    throw new RegistryError(
      "malformed_response",
      `${url}: invalid JSON: ${err instanceof Error ? err.message : String(err)}`,
      response.status,
      { cause: err },
    );
  }

  if (!isByCallKeyOk(parsed)) {
    throw new RegistryError(
      "malformed_response",
      `${url}: response does not match ByCallKeyOk shape`,
      response.status,
    );
  }

  return parsed;
}

function isAbortError(err: unknown): boolean {
  if (!err || typeof err !== "object") return false;
  const e = err as { name?: unknown; code?: unknown };
  return e.name === "AbortError" || e.code === "ABORT_ERR";
}

/**
 * Shape-check the wire payload. Mirrors §6.1 — we explicitly require
 * `matched: true` (the spec disallows `matched: false` over the wire; 404 is
 * the only "missing" signal).
 */
function isByCallKeyOk(v: unknown): v is ByCallKeyOk {
  if (!v || typeof v !== "object") return false;
  const o = v as Record<string, unknown>;
  return (
    o.matched === true &&
    typeof o.bundle_id === "string" &&
    typeof o.manifest_path === "string" &&
    typeof o.bundle_sha256 === "string" &&
    typeof o.bundle === "object" &&
    o.bundle !== null
  );
}
