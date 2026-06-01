/**
 * registry-api — HTTP server (caching authenticated reverse-proxy).
 *
 * policy-rpc/src/server.ts 미러: Node stdlib http.createServer, 단일
 * routeRequest dispatcher, writeJson helper, top-level try/catch → 500.
 *
 * Routes:
 *   GET /health                       → { ok: true }
 *   GET /debug/recent                 → 최근 요청 log + cache stats
 *   GET /index/by-callkey/<key>.json  → 비공개 버킷 object proxy
 *   GET /index/by-typed-data/<key>.json → 비공개 버킷 object proxy (off-chain sig)
 *   GET /tokens/<chain>/<addr>.json   → 비공개 버킷 object proxy
 *   GET /bundles/<sha>.json           → generated bundle template object
 *   GET /contexts/<source>/<chain>/<addr>.json
 *                                     → generated per-target context object
 *   GET /v1/registry/by-callkey?chain_id&to&selector
 *                                     → spec §6.1 callkey proxy alias (secondary)
 *   OPTIONS <any>                     → 204 CORS preflight
 *
 * Proxy 의미 (핵심 — 익스텐션 negative cache 가 의존):
 *   GCS object found   → 200, body 그대로, Cache-Control + CORS
 *   GCS object 없음    → 404  (REAL status → 익스텐션 no_publisher 5min)
 *   GCS upstream error → 502
 *   rate 초과          → 429
 *   bad path / method  → 404 / 405
 */
import {
  createServer,
  type IncomingMessage,
  type Server,
  type ServerResponse,
} from "node:http";
import type { RegistryApiConfig } from "./config.js";
import type { ObjectReader } from "./gcs-client.js";
import { ObjectCache, type CacheValue } from "./cache.js";
import { TokenBucketRateLimiter } from "./rate-limiter.js";
import { LogStore } from "./log-store.js";
import { parseProxyTarget } from "./validation.js";
import type { NowMs } from "./types.js";

export interface RegistryApiServerOptions {
  config: RegistryApiConfig;
  reader: ObjectReader;
  cache?: ObjectCache;
  rateLimiter?: TokenBucketRateLimiter;
  logStore?: LogStore;
  nowMs?: NowMs;
}

const CORS_HEADERS: Record<string, string> = {
  "access-control-allow-origin": "*",
  "access-control-allow-methods": "GET, OPTIONS",
  "access-control-allow-headers": "*",
  "access-control-max-age": "86400",
};

export function createRegistryApiServer(
  options: RegistryApiServerOptions,
): Server {
  const { config, reader } = options;
  const nowMs = options.nowMs ?? Date.now;
  const cache =
    options.cache ??
    new ObjectCache({
      maxEntries: config.cacheMaxEntries,
      ttlMs: config.cacheTtlMs,
      negativeTtlMs: config.cacheNegativeTtlMs,
      nowMs,
    });
  const rateLimiter =
    options.rateLimiter ??
    new TokenBucketRateLimiter({
      burst: config.rateLimitBurst,
      refillPerSec: config.rateLimitRefillPerSec,
      maxIps: config.rateLimitMaxIps,
      nowMs,
    });
  const logStore = options.logStore ?? new LogStore();

  return createServer(async (request, response) => {
    try {
      await routeRequest({
        request,
        response,
        config,
        reader,
        cache,
        rateLimiter,
        logStore,
        nowMs,
      });
    } catch (error) {
      const message =
        error instanceof Error ? error.message : "Unexpected server error";
      writeJson(response, 500, {
        ok: false,
        error: { code: "internal_error", message },
      });
    }
  });
}

interface RouteInput {
  request: IncomingMessage;
  response: ServerResponse;
  config: RegistryApiConfig;
  reader: ObjectReader;
  cache: ObjectCache;
  rateLimiter: TokenBucketRateLimiter;
  logStore: LogStore;
  nowMs: NowMs;
}

async function routeRequest(input: RouteInput): Promise<void> {
  const method = input.request.method ?? "GET";
  const url = new URL(input.request.url ?? "/", "http://127.0.0.1");

  if (method === "OPTIONS") {
    input.response.writeHead(204, CORS_HEADERS);
    input.response.end();
    return;
  }
  if (method === "GET" && url.pathname === "/health") {
    writeJson(input.response, 200, { ok: true });
    return;
  }
  if (method === "GET" && url.pathname === "/debug/recent") {
    writeJson(input.response, 200, {
      entries: input.logStore.recent(),
      cache: input.cache.stats(),
    });
    return;
  }

  // Secondary spec §6.1 query interface — canonical object path 로 rewrite 후
  // 같은 proxy handler 로 fall through.
  let proxyPath = url.pathname;
  if (method === "GET" && url.pathname === "/v1/registry/by-callkey") {
    const chainId = url.searchParams.get("chain_id") ?? "";
    const to = (url.searchParams.get("to") ?? "").toLowerCase();
    const selector = (url.searchParams.get("selector") ?? "").toLowerCase();
    proxyPath = `/index/by-callkey/${chainId}__${to}__${selector}.json`;
  }

  if (
    method === "GET" &&
    (proxyPath.startsWith("/index/by-callkey/") ||
      proxyPath.startsWith("/index/by-typed-data/") ||
      proxyPath.startsWith("/tokens/") ||
      proxyPath.startsWith("/bundles/") ||
      proxyPath.startsWith("/contexts/"))
  ) {
    await handleProxy(input, proxyPath);
    return;
  }
  if (method !== "GET") {
    writeJson(input.response, 405, {
      ok: false,
      error: { code: "method_not_allowed", message: "Only GET is supported" },
    });
    return;
  }
  writeJson(input.response, 404, {
    ok: false,
    error: { code: "not_found", message: "Route not found" },
  });
}

async function handleProxy(input: RouteInput, proxyPath: string): Promise<void> {
  const startMs = input.nowMs();

  // 1. client IP 별 rate limit — validation 보다 먼저 돌려서 garbage path
  //    폭주도 throttle 한다.
  const ip = clientIp(input.request);
  if (!input.rateLimiter.allow(ip)) {
    input.response.writeHead(429, {
      ...CORS_HEADERS,
      "content-type": "application/json; charset=utf-8",
      "retry-after": "1",
    });
    input.response.end(
      JSON.stringify({
        ok: false,
        error: { code: "rate_limited", message: "Too many requests" },
      }),
    );
    logRequest(input, proxyPath, 429, "n/a", startMs);
    return;
  }

  // 2. path → 버킷 object 이름 검증. 임의 object 를 절대 안 읽음;
  //    invalid path 는 404.
  const target = parseProxyTarget(proxyPath);
  if (!target.ok) {
    writeProxy404(input.response);
    logRequest(input, proxyPath, 404, "n/a", startMs);
    return;
  }
  const cacheKey = target.objectName;

  // 3. cache lookup — hit (positive/negative) 면 GCS 를 건너뜀.
  const cached = input.cache.get(cacheKey);
  if (cached) {
    sendCacheValue(input, cached);
    logRequest(input, proxyPath, cached.status, "hit", startMs);
    return;
  }

  // 4. cache miss → 비공개 버킷 read.
  const result = await input.reader.read(target.objectName);

  if (result.kind === "found") {
    let value: CacheValue;
    try {
      value = await materializeIfRefIndex(input, target.objectName, result);
    } catch (error) {
      input.response.writeHead(502, {
        ...CORS_HEADERS,
        "content-type": "application/json; charset=utf-8",
      });
      input.response.end(
        JSON.stringify({
          ok: false,
          error: {
            code: "ref_materialization_failed",
            message: error instanceof Error ? error.message : String(error),
          },
        }),
      );
      logRequest(input, proxyPath, 502, "miss", startMs);
      return;
    }
    input.cache.set(cacheKey, value);
    sendCacheValue(input, value);
    logRequest(input, proxyPath, 200, "miss", startMs);
    return;
  }
  if (result.kind === "not_found") {
    input.cache.set(cacheKey, { status: 404 });
    writeProxy404(input.response);
    logRequest(input, proxyPath, 404, "miss", startMs);
    return;
  }
  // upstream_error — 캐시 안 함; 502 → 익스텐션이 transient network error
  // (30 s negative cache 후 retry) 로 취급.
  input.response.writeHead(502, {
    ...CORS_HEADERS,
    "content-type": "application/json; charset=utf-8",
  });
  input.response.end(
    JSON.stringify({
      ok: false,
      error: { code: "upstream_error", message: result.message },
    }),
  );
  logRequest(input, proxyPath, 502, "miss", startMs);
}

interface FoundObject {
  kind: "found";
  body: Buffer;
  contentType: string;
}

interface RefRegistryEntry {
  matched: true;
  schema_version: "3-ref";
  bundle_id: string;
  manifest_path: string;
  bundle_sha256: string;
  bundle_ref: string;
  context_ref?: string;
}

interface SourceContextDocument {
  schema_version: "3-source-context";
  chain_id: number;
  address: string;
  context: Record<string, unknown>;
}

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function isRefRegistryEntry(v: unknown): v is RefRegistryEntry {
  if (!isRecord(v)) return false;
  return (
    v.matched === true &&
    v.schema_version === "3-ref" &&
    typeof v.bundle_id === "string" &&
    typeof v.manifest_path === "string" &&
    typeof v.bundle_sha256 === "string" &&
    typeof v.bundle_ref === "string" &&
    (!("context_ref" in v) || typeof v.context_ref === "string")
  );
}

function parseJsonBuffer<T>(body: Buffer, label: string): T {
  try {
    return JSON.parse(body.toString("utf8")) as T;
  } catch (error) {
    throw new Error(
      `${label}: invalid JSON: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

function normalizeObjectRef(ref: string): string {
  const path = ref.startsWith("/") ? ref : `/${ref}`;
  const target = parseProxyTarget(path);
  if (!target.ok) {
    throw new Error(`invalid generated object ref ${JSON.stringify(ref)}`);
  }
  return target.objectName;
}

async function readGeneratedJson<T>(
  input: RouteInput,
  ref: string,
): Promise<T> {
  const objectName = normalizeObjectRef(ref);
  const result = await input.reader.read(objectName);
  if (result.kind !== "found") {
    const kind =
      result.kind === "upstream_error" ? `upstream_error: ${result.message}` : "not_found";
    throw new Error(`${objectName}: ${kind}`);
  }
  return parseJsonBuffer<T>(result.body, objectName);
}

function lookupSourcePath(context: Record<string, unknown>, path: string): unknown {
  let current: unknown = context;
  for (const segment of path.split(".")) {
    if (Array.isArray(current)) {
      const index = Number(segment);
      if (!Number.isInteger(index)) return undefined;
      current = current[index];
    } else if (isRecord(current)) {
      current = current[segment];
    } else {
      return undefined;
    }
  }
  return current;
}

function substituteSourcePlaceholders(
  value: unknown,
  context: Record<string, unknown>,
): unknown {
  if (typeof value === "string") {
    if (!value.startsWith("$source.")) return value;
    const resolved = lookupSourcePath(context, value.slice("$source.".length));
    if (resolved === undefined) {
      throw new Error(`unknown source placeholder ${JSON.stringify(value)}`);
    }
    return resolved;
  }
  if (Array.isArray(value)) {
    return value.map((item) => substituteSourcePlaceholders(item, context));
  }
  if (isRecord(value)) {
    const out: Record<string, unknown> = {};
    for (const [key, nested] of Object.entries(value)) {
      out[key] = substituteSourcePlaceholders(nested, context);
    }
    return out;
  }
  return value;
}

function sanitizeIdSuffix(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9._/-]+/g, "-")
    .replace(/\/+/g, "/")
    .replace(/^-+|-+$/g, "");
}

function appendIdSuffix(id: string, suffix: string): string {
  const clean = sanitizeIdSuffix(suffix);
  if (!clean) throw new Error(`source materialization produced empty id suffix for ${id}`);
  const at = id.lastIndexOf("@");
  if (at === -1) return `${id}/${clean}`;
  return `${id.slice(0, at)}/${clean}${id.slice(at)}`;
}

function materializeSourceBundle(
  template: unknown,
  contextDoc: SourceContextDocument,
): Record<string, unknown> {
  if (!isRecord(template)) throw new Error("bundle template must be an object");
  if (!isRecord(contextDoc) || !isRecord(contextDoc.context)) {
    throw new Error("source context document must have context object");
  }
  const context = contextDoc.context;
  const substituted = substituteSourcePlaceholders(template, context);
  if (!isRecord(substituted)) {
    throw new Error("source-substituted bundle must be an object");
  }
  const match = substituted.match;
  if (!isRecord(match) || typeof match.selector !== "string") {
    throw new Error("source-substituted bundle missing match.selector");
  }
  const id = substituted.id;
  if (typeof id !== "string") {
    throw new Error("source-substituted bundle missing id");
  }
  const idSuffix = context.id_suffix;
  if (typeof idSuffix !== "string") {
    throw new Error("source context missing id_suffix");
  }
  const chainId = contextDoc.chain_id;
  const address = contextDoc.address.toLowerCase();
  if (!Number.isInteger(chainId) || typeof contextDoc.address !== "string") {
    throw new Error("source context has invalid chain_id/address");
  }

  const { match: _match, source_materialize: _sourceMaterialize, ...rest } =
    substituted;
  return {
    ...rest,
    id: appendIdSuffix(id, idSuffix),
    match: {
      selector: match.selector,
      chain_to_addresses: {
        [String(chainId)]: [address],
      },
    },
  };
}

async function materializeIfRefIndex(
  input: RouteInput,
  objectName: string,
  result: FoundObject,
): Promise<CacheValue> {
  if (!objectName.startsWith("index/by-callkey/")) {
    return {
      status: 200,
      body: result.body,
      contentType: result.contentType,
    };
  }

  const entry = parseJsonBuffer<unknown>(result.body, objectName);
  if (!isRefRegistryEntry(entry)) {
    return {
      status: 200,
      body: result.body,
      contentType: result.contentType,
    };
  }

  const template = await readGeneratedJson<unknown>(input, entry.bundle_ref);
  if (entry.context_ref === undefined) {
    const response = {
      matched: true,
      bundle_id: entry.bundle_id,
      manifest_path: entry.manifest_path,
      bundle_sha256: entry.bundle_sha256,
      bundle: template,
    };
    return {
      status: 200,
      body: Buffer.from(JSON.stringify(response, null, 2) + "\n", "utf8"),
      contentType: result.contentType,
    };
  }
  const contextDoc = await readGeneratedJson<SourceContextDocument>(
    input,
    entry.context_ref,
  );
  const bundle = materializeSourceBundle(template, contextDoc);
  const response = {
    matched: true,
    bundle_id: bundle.id,
    manifest_path: entry.manifest_path,
    bundle_sha256: entry.bundle_sha256,
    bundle,
  };

  return {
    status: 200,
    body: Buffer.from(JSON.stringify(response, null, 2) + "\n", "utf8"),
    contentType: result.contentType,
  };
}

function sendCacheValue(input: RouteInput, value: CacheValue): void {
  if (value.status === 404) {
    writeProxy404(input.response);
    return;
  }
  input.response.writeHead(200, {
    ...CORS_HEADERS,
    "content-type": value.contentType,
    "cache-control": input.config.cacheControlValue,
  });
  input.response.end(value.body);
}

function writeProxy404(response: ServerResponse): void {
  // REAL 404 status — 익스텐션 registry client 가 5분 no_publisher negative
  // cache 로 매핑. body 는 informational.
  response.writeHead(404, {
    ...CORS_HEADERS,
    "content-type": "application/json; charset=utf-8",
  });
  response.end(
    JSON.stringify({
      ok: false,
      error: { code: "not_found", message: "Registry object not found" },
    }),
  );
}

/**
 * best-effort client IP. Cloud Run 은 Google front end 뒤에 있어
 * X-Forwarded-For 가 세팅됨; left-most 가 원 client. 로컬 실행은 socket 주소.
 */
function clientIp(request: IncomingMessage): string {
  const xff = request.headers["x-forwarded-for"];
  if (typeof xff === "string" && xff.length > 0) {
    const first = xff.split(",")[0]?.trim();
    if (first) return first;
  }
  return request.socket.remoteAddress ?? "unknown";
}

function logRequest(
  input: RouteInput,
  path: string,
  status: number,
  cache: "hit" | "miss" | "n/a",
  startMs: number,
): void {
  const duration = Math.max(0, input.nowMs() - startMs);
  input.logStore.add({
    ts: new Date(input.nowMs()).toISOString(),
    path,
    status,
    cache,
    duration_ms: duration,
  });
  // 구조화 log line — Cloud Logging 이 stdout JSON 을 자동 ingest.
  console.log(
    JSON.stringify({
      event: "registry_api_request",
      path,
      status,
      cache,
      duration_ms: duration,
    }),
  );
}

function writeJson(
  response: ServerResponse,
  statusCode: number,
  body: unknown,
): void {
  response.writeHead(statusCode, {
    ...CORS_HEADERS,
    "content-type": "application/json; charset=utf-8",
  });
  response.end(JSON.stringify(body));
}
