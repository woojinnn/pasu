import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";

import { LogStore, type RecentCallLog } from "./log-store.js";
import type { SidecarConfig } from "./methods/catalog.js";
import {
  loadPluginEntries,
  type LoadedPluginEntry,
  type PluginLoaderOptions,
} from "./methods/plugin-loader.js";
import {
  loadSidecarEntries,
  type LoadedSidecarEntry,
  type SidecarLoaderOptions,
} from "./methods/sidecar-loader.js";
import {
  createMethodRegistry,
  type MethodRegistry,
  type MethodRegistryOptions,
} from "./methods/registry.js";
import { type NowMs, type PolicyRpcRequest } from "./types.js";
import { parsePolicyRpcRequest, ValidationError } from "./validation.js";

export interface PolicyRpcServerOptions extends MethodRegistryOptions {
  logStore?: LogStore;
  registry?: MethodRegistry;
  maxBodyBytes?: number;
}

/**
 * Async bootstrap helper that wires plugin + sidecar discovery into a
 * fresh `PolicyRpcServer`. Use this in production entry points (e.g.
 * `index.ts`) so the server boots with the full method set discovered
 * from filesystem and configured sidecar URLs.
 *
 * Sync constructor `createPolicyRpcServer` is preserved for callers
 * (notably tests) that already build their own registry or don't need
 * external method discovery.
 */
export async function bootstrapPolicyRpcServer(
  options: PolicyRpcServerOptions & {
    plugins?: PluginLoaderOptions;
    sidecars?: SidecarLoaderOptions;
  } = {},
): Promise<{
  server: Server;
  pluginEntries: readonly LoadedPluginEntry[];
  sidecarEntries: readonly LoadedSidecarEntry[];
}> {
  const pluginEntries =
    options.pluginEntries ??
    (await loadPluginEntries(options.plugins ?? {}));
  const sidecarEntries =
    options.sidecarEntries ??
    (await loadSidecarEntries(options.sidecars ?? {}));
  const server = createPolicyRpcServer({
    ...options,
    pluginEntries,
    sidecarEntries,
  });
  return { server, pluginEntries, sidecarEntries };
}

/**
 * Read a sidecar configuration JSON file from disk. Returns `[]` when
 * the file is absent or malformed (warn-on-error). Used by entrypoint
 * code so operators can declare sidecars via a config file rather than
 * stuffing them into env vars.
 */
export async function readSidecarConfigFile(
  path: string,
  warn: (message: string, ...args: unknown[]) => void = console.warn,
): Promise<SidecarConfig[]> {
  const { readFile } = await import("node:fs/promises");
  let raw: string;
  try {
    raw = await readFile(path, "utf8");
  } catch (error) {
    // ENOENT is a normal case (no sidecars configured) — quiet.
    if ((error as { code?: string }).code !== "ENOENT") {
      warn(
        `[policy-rpc] sidecar config read failed at ${path}: ${
          error instanceof Error ? error.message : String(error)
        }`,
      );
    }
    return [];
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (error) {
    warn(
      `[policy-rpc] sidecar config at ${path} is not valid JSON: ${
        error instanceof Error ? error.message : String(error)
      }`,
    );
    return [];
  }
  if (
    !parsed ||
    typeof parsed !== "object" ||
    !Array.isArray((parsed as { sidecars?: unknown }).sidecars)
  ) {
    warn(
      `[policy-rpc] sidecar config at ${path} must shape { "sidecars": [...] }`,
    );
    return [];
  }
  return (parsed as { sidecars: SidecarConfig[] }).sidecars;
}

const defaultMaxBodyBytes = 1_000_000;

export function createPolicyRpcServer(options: PolicyRpcServerOptions = {}): Server {
  const registry = options.registry ?? createMethodRegistry(options);
  const logStore = options.logStore ?? new LogStore();
  const nowMs = options.nowMs ?? Date.now;
  const maxBodyBytes = options.maxBodyBytes ?? defaultMaxBodyBytes;

  return createServer(async (request, response) => {
    try {
      await routeRequest({
        request,
        response,
        registry,
        logStore,
        nowMs,
        maxBodyBytes,
      });
    } catch (error) {
      writeUnexpectedError(response, error);
    }
  });
}

interface RouteRequestInput {
  request: IncomingMessage;
  response: ServerResponse;
  registry: MethodRegistry;
  logStore: LogStore;
  nowMs: NowMs;
  maxBodyBytes: number;
}

async function routeRequest(input: RouteRequestInput): Promise<void> {
  const method = input.request.method ?? "GET";
  const url = new URL(input.request.url ?? "/", "http://127.0.0.1");

  if (method === "GET" && url.pathname === "/health") {
    writeJson(input.response, 200, { ok: true });
    return;
  }

  if (method === "GET" && url.pathname === "/v1/methods") {
    // Phase 8.5: this endpoint returns the FULL method catalog (params,
    // returns, origin) — the dashboard's manifest editor consumes it to
    // drive its method/param/output dropdowns. The legacy `{ methods:
    // [...] }` shape is preserved alongside the new `catalog` field so
    // older clients that only read the name list keep working.
    writeJson(input.response, 200, {
      methods: input.registry.listMethods(),
      catalog: input.registry.catalog(),
    });
    return;
  }

  if (method === "POST" && url.pathname === "/v1/rpc") {
    await handleRpc(input);
    return;
  }

  if (method === "GET" && url.pathname === "/debug/recent") {
    writeJson(input.response, 200, { entries: input.logStore.recent() });
    return;
  }

  writeJson(input.response, 404, {
    ok: false,
    error: { code: "not_found", message: "Route not found" },
  });
}

async function handleRpc(input: RouteRequestInput): Promise<void> {
  let requestBody: unknown;
  let rpcRequest: PolicyRpcRequest;

  try {
    requestBody = await readJson(input.request, input.maxBodyBytes);
    rpcRequest = parsePolicyRpcRequest(requestBody);
  } catch (error) {
    const message = error instanceof Error ? error.message : "Invalid request body";
    writeJson(input.response, 400, {
      ok: false,
      error: { code: "bad_request", message },
    });
    return;
  }

  const startedAt = new Date(input.nowMs()).toISOString();
  const batchStartMs = input.nowMs();
  const callLogs: RecentCallLog[] = [];
  const results = [];

  for (const call of rpcRequest.calls) {
    const callStartMs = input.nowMs();
    const result = await input.registry.execute(call);
    const durationMs = elapsedMs(input.nowMs, callStartMs);

    callLogs.push({
      id: call.id,
      method: call.method,
      ok: result.ok,
      duration_ms: durationMs,
      ...(result.ok ? {} : { error_code: result.error.code }),
    });
    results.push(result);
  }

  const durationMs = elapsedMs(input.nowMs, batchStartMs);
  input.logStore.add({
    request_id: rpcRequest.request_id,
    started_at: startedAt,
    duration_ms: durationMs,
    calls: callLogs,
  });

  console.log(
    JSON.stringify({
      event: "policy_rpc_batch",
      request_id: rpcRequest.request_id,
      duration_ms: durationMs,
      calls: callLogs,
    }),
  );

  writeJson(input.response, 200, {
    request_id: rpcRequest.request_id,
    results,
  });
}

async function readJson(request: IncomingMessage, maxBodyBytes: number): Promise<unknown> {
  const chunks: Buffer[] = [];
  let totalBytes = 0;

  for await (const chunk of request) {
    const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    totalBytes += buffer.length;

    if (totalBytes > maxBodyBytes) {
      throw new ValidationError("request body is too large");
    }

    chunks.push(buffer);
  }

  const text = Buffer.concat(chunks).toString("utf8");

  if (text.trim() === "") {
    throw new ValidationError("request body must not be empty");
  }

  return JSON.parse(text);
}

function elapsedMs(nowMs: NowMs, startMs: number): number {
  return Math.max(0, nowMs() - startMs);
}

function writeJson(response: ServerResponse, statusCode: number, body: unknown): void {
  response.statusCode = statusCode;
  response.setHeader("content-type", "application/json; charset=utf-8");
  response.end(JSON.stringify(body));
}

function writeUnexpectedError(response: ServerResponse, error: unknown): void {
  const message = error instanceof Error ? error.message : "Unexpected server error";
  writeJson(response, 500, {
    ok: false,
    error: { code: "internal_error", message },
  });
}
