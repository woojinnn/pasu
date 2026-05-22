// Sidecar plugin loader (X-1 model).
//
// At daemon startup, query each configured sidecar's `GET /v1/methods`
// to learn what it serves. Every catalog entry whose `name` starts
// with the sidecar's declared prefix becomes a method in this
// daemon's registry, with `origin = "sidecar"` and a `fn` that
// HTTP-forwards the call to the sidecar at evaluation time.
//
// Failure modes are best-effort:
//   - Sidecar unreachable at startup → warn + skip. Daemon still boots.
//   - Sidecar publishes entries outside its prefix → reject those entries
//     (defends against a misconfigured sidecar shadowing bundled methods).
//   - Sidecar errors during RPC forwarding → surface as `upstream_error`.
//
// The forwarder uses `POST <url>/v1/rpc` with a single-call batch
// (mirroring our own RPC protocol) so a sidecar can be ANY policy-rpc
// daemon — including a stripped-down clone of this one.

import {
  RpcMethodError,
  type JsonObject,
} from "../types.js";
import type {
  MethodCatalog,
  MethodCatalogEntry,
  SidecarConfig,
} from "./catalog.js";

export interface LoadedSidecarEntry {
  fn: (params: unknown) => Promise<JsonObject>;
  catalog: MethodCatalogEntry;
  /** Sidecar config that owns this method (for logging). */
  source: SidecarConfig;
}

export interface SidecarLoaderOptions {
  /** Configured sidecars; empty array → no sidecars. */
  sidecars?: readonly SidecarConfig[];
  /**
   * Fetch implementation. Tests stub this to avoid real HTTP.
   * Defaults to global `fetch` (Node 20+ supports it built-in).
   */
  fetchImpl?: typeof fetch;
  /** Warning sink. Defaults to `console.warn`. */
  warn?: (message: string, ...args: unknown[]) => void;
  /**
   * Timeout for the startup `GET /v1/methods` and per-call forwarding,
   * in milliseconds. Default 5_000 — slow sidecars don't get to hang
   * the whole daemon.
   */
  timeoutMs?: number;
}

const DEFAULT_TIMEOUT_MS = 5_000;

/**
 * Query every sidecar, fetch its catalog, and return the union of
 * method entries to add to the registry. Sidecars that fail to
 * respond produce a warning but never block the daemon.
 */
export async function loadSidecarEntries(
  options: SidecarLoaderOptions,
): Promise<LoadedSidecarEntry[]> {
  const sidecars = options.sidecars ?? [];
  const fetchImpl = options.fetchImpl ?? (globalThis.fetch as typeof fetch);
  const warn = options.warn ?? console.warn;
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;

  const all: LoadedSidecarEntry[] = [];
  for (const sidecar of sidecars) {
    const issue = validateSidecarConfig(sidecar);
    if (issue !== null) {
      warn(`[policy-rpc] sidecar config rejected: ${issue} (config: ${JSON.stringify(sidecar)})`);
      continue;
    }
    const entries = await discoverOne(sidecar, fetchImpl, warn, timeoutMs);
    all.push(...entries);
  }
  return all;
}

function validateSidecarConfig(sidecar: SidecarConfig): string | null {
  if (typeof sidecar.name !== "string" || sidecar.name.length === 0) {
    return "name must be a non-empty string";
  }
  if (typeof sidecar.url !== "string" || sidecar.url.length === 0) {
    return "url must be a non-empty string";
  }
  if (!/^https?:\/\//i.test(sidecar.url)) {
    return `url must start with http:// or https:// (got ${sidecar.url})`;
  }
  if (typeof sidecar.methodPrefix !== "string" || sidecar.methodPrefix.length === 0) {
    return "methodPrefix must be a non-empty string";
  }
  return null;
}

async function discoverOne(
  sidecar: SidecarConfig,
  fetchImpl: typeof fetch,
  warn: (message: string, ...args: unknown[]) => void,
  timeoutMs: number,
): Promise<LoadedSidecarEntry[]> {
  const catalogUrl = joinUrl(sidecar.url, "/v1/methods");
  let body: unknown;
  try {
    const response = await withTimeout(
      fetchImpl(catalogUrl, { method: "GET" }),
      timeoutMs,
      () =>
        new RpcMethodError(
          "upstream_error",
          `sidecar ${sidecar.name} catalog request timed out`,
        ),
    );
    if (!response.ok) {
      warn(
        `[policy-rpc] sidecar ${sidecar.name} catalog request failed: HTTP ${response.status}`,
      );
      return [];
    }
    body = await response.json();
  } catch (error) {
    warn(
      `[policy-rpc] sidecar ${sidecar.name} unreachable at ${catalogUrl}: ${asMessage(error)}`,
    );
    return [];
  }

  const catalog = extractCatalog(body);
  if (!catalog) {
    warn(
      `[policy-rpc] sidecar ${sidecar.name} returned no usable catalog (expected {catalog:{methods:{...}}} or {methods:{...}})`,
    );
    return [];
  }

  const out: LoadedSidecarEntry[] = [];
  for (const [name, entry] of Object.entries(catalog.methods)) {
    if (!name.startsWith(sidecar.methodPrefix)) {
      warn(
        `[policy-rpc] sidecar ${sidecar.name} declared method "${name}" outside its prefix "${sidecar.methodPrefix}"; skipping`,
      );
      continue;
    }
    out.push({
      fn: makeForwarder(sidecar, name, fetchImpl, timeoutMs),
      // Force `origin: "sidecar"` so a sidecar can't claim to be a
      // bundled method — the dashboard's UI relies on this badge to
      // surface where each entry came from.
      catalog: { ...entry, name, origin: "sidecar" },
      source: sidecar,
    });
  }
  return out;
}

/**
 * Accept both Phase 8.4 shape (`{methods:[...], catalog:{methods:{...}}}`)
 * and a hypothetical sidecar that only ships the catalog field. Returns
 * `null` when neither path yields a usable `methods` map.
 */
function extractCatalog(body: unknown): MethodCatalog | null {
  if (!body || typeof body !== "object") return null;
  const root = body as Record<string, unknown>;
  const wrapper =
    (root.catalog as Record<string, unknown> | undefined) ?? root;
  const methods = wrapper?.methods;
  if (!methods || typeof methods !== "object" || Array.isArray(methods)) {
    return null;
  }
  return { methods: methods as MethodCatalog["methods"] };
}

/**
 * Build the runtime forwarder for one (sidecar, method) pair. Each
 * call wraps the params into a single-element RPC batch, posts it
 * to the sidecar's `/v1/rpc`, and unwraps the result.
 */
function makeForwarder(
  sidecar: SidecarConfig,
  methodName: string,
  fetchImpl: typeof fetch,
  timeoutMs: number,
): (params: unknown) => Promise<JsonObject> {
  const rpcUrl = joinUrl(sidecar.url, "/v1/rpc");
  return async (params: unknown): Promise<JsonObject> => {
    const requestId = `forward-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    const body = JSON.stringify({
      request_id: requestId,
      calls: [
        {
          id: "call-1",
          method: methodName,
          params: (params ?? {}) as JsonObject,
        },
      ],
    });

    let response: Response;
    try {
      response = await withTimeout(
        fetchImpl(rpcUrl, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body,
        }),
        timeoutMs,
        () =>
          new RpcMethodError(
            "upstream_error",
            `sidecar ${sidecar.name} call ${methodName} timed out`,
          ),
      );
    } catch (error) {
      throw error instanceof RpcMethodError
        ? error
        : new RpcMethodError(
            "upstream_error",
            `sidecar ${sidecar.name} unreachable: ${asMessage(error)}`,
          );
    }

    if (!response.ok) {
      throw new RpcMethodError(
        "upstream_error",
        `sidecar ${sidecar.name} returned HTTP ${response.status}`,
      );
    }

    let payload: unknown;
    try {
      payload = await response.json();
    } catch {
      throw new RpcMethodError(
        "upstream_error",
        `sidecar ${sidecar.name} returned non-JSON body`,
      );
    }

    const result = extractFirstResult(payload, methodName, sidecar);
    return result;
  };
}

function extractFirstResult(
  payload: unknown,
  methodName: string,
  sidecar: SidecarConfig,
): JsonObject {
  if (!payload || typeof payload !== "object") {
    throw new RpcMethodError(
      "upstream_error",
      `sidecar ${sidecar.name}: ${methodName} response missing body`,
    );
  }
  const root = payload as { results?: unknown };
  if (!Array.isArray(root.results) || root.results.length === 0) {
    throw new RpcMethodError(
      "upstream_error",
      `sidecar ${sidecar.name}: ${methodName} response had no results`,
    );
  }
  const first = root.results[0] as
    | { ok: true; result: JsonObject }
    | { ok: false; error: { code: string; message: string } };
  if (first.ok) {
    return first.result;
  }
  // Forward the sidecar's error code verbatim — the engine treats
  // `invalid_params`, `upstream_error`, etc. uniformly, so passing
  // through preserves error semantics.
  throw new RpcMethodError(
    first.error?.code || "upstream_error",
    `${sidecar.name}: ${first.error?.message || "remote error"}`,
  );
}

function joinUrl(base: string, path: string): string {
  return `${base.replace(/\/+$/, "")}${path}`;
}

async function withTimeout<T>(
  promise: Promise<T>,
  ms: number,
  onTimeout: () => Error,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout>;
  const timeout = new Promise<T>((_, reject) => {
    timer = setTimeout(() => reject(onTimeout()), ms);
  });
  try {
    return await Promise.race([promise, timeout]);
  } finally {
    clearTimeout(timer!);
  }
}

function asMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
