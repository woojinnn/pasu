/**
 * registry-api — runtime configuration. 모든 값이 env-overridable 라
 * Cloud Run deploy 가 rebuild 없이 proxy 를 튜닝한다. 기본값은 Dambi
 * PoC 트래픽 (Uniswap callkey 몇 개 + ~50 token 파일) 기준.
 */
export interface RegistryApiConfig {
  host: string;
  port: number;
  bucketName: string;
  cacheMaxEntries: number;
  cacheTtlMs: number;
  cacheNegativeTtlMs: number;
  cacheControlValue: string;
  /** Cache-Control for content-addressed leaves (bundles/<sha>, signatures/<sha>):
   *  immutable — the sha IS the version, so they can be cached indefinitely. */
  immutableCacheControlValue: string;
  /** Cache-Control for 404 responses — short negative cache blunts probe floods. */
  negativeCacheControlValue: string;
  /** Shared secret gating GET /debug/recent (empty ⇒ the route is 404/disabled). */
  debugToken: string;
  rateLimitBurst: number;
  rateLimitRefillPerSec: number;
  rateLimitMaxIps: number;
  trustedProxyHops: number;
}

function intFromEnv(name: string, fallback: number): number {
  const raw = process.env[name];
  if (raw === undefined || raw.trim() === "") return fallback;
  const parsed = Number.parseInt(raw, 10);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : fallback;
}

function stringFromEnv(name: string, fallback: string): string {
  const raw = process.env[name];
  return raw !== undefined && raw.trim() !== "" ? raw : fallback;
}

export function loadConfig(): RegistryApiConfig {
  const cacheNegativeTtlMs = intFromEnv("CACHE_NEGATIVE_TTL_MS", 60_000);
  return {
    host: stringFromEnv("HOST", "0.0.0.0"),
    port: intFromEnv("PORT", 8080),
    bucketName: stringFromEnv("REGISTRY_BUCKET", "dambi-registry-seoul"),
    cacheMaxEntries: intFromEnv("CACHE_MAX_ENTRIES", 1024),
    cacheTtlMs: intFromEnv("CACHE_TTL_MS", 300_000),
    cacheNegativeTtlMs,
    // SWR removed: the proxy's ObjectCache does hard TTL expiry with no background
    // revalidation, so advertising stale-while-revalidate promised semantics it did
    // not implement (and widened the N1 rollback window). Plain max-age.
    cacheControlValue: stringFromEnv("CACHE_CONTROL", "public, max-age=300"),
    immutableCacheControlValue: stringFromEnv(
      "CACHE_CONTROL_IMMUTABLE",
      "public, max-age=31536000, immutable",
    ),
    negativeCacheControlValue: `public, max-age=${Math.floor(cacheNegativeTtlMs / 1000)}`,
    debugToken: stringFromEnv("DEBUG_TOKEN", ""),
    rateLimitBurst: intFromEnv("RATE_LIMIT_BURST", 60),
    rateLimitRefillPerSec: intFromEnv("RATE_LIMIT_REFILL_PER_SEC", 10),
    rateLimitMaxIps: intFromEnv("RATE_LIMIT_MAX_IPS", 10_000),
    // Trusted proxy hops to skip from the RIGHT of X-Forwarded-For when choosing
    // the rate-limit key. On Cloud Run the rightmost entry is ALWAYS appended by
    // Google's frontend (a request can't reach the container otherwise), so it is
    // never client-spoofable — default 0 (rightmost) FAILS SAFE: it can never be
    // bypassed, worst case it over-throttles. For direct *.run.app the rightmost
    // is the genuine client IP (true per-IP); behind a Google HTTP LB it may be
    // the LB forwarding IP (degrades to a shared/global cap — still cost-safe) so
    // set TRUSTED_PROXY_HOPS to the LB hop count for per-IP there. (Cloud Run's
    // exact ordering for direct run.app is not crisply documented — Google
    // issuetracker 239503543 — hence the fail-safe default.)
    trustedProxyHops: intFromEnv("TRUSTED_PROXY_HOPS", 0),
  };
}
