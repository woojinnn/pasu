/**
 * Phase M3 — declarative adapter loader (v3-only).
 *
 * Spec: `ADAPTER_LOADER_ARCHITECTURE.md` §5.5 (TS bridge) and §7
 * (3-layer loading). The loader is the single mount point onto the WASM
 * engine for declarative v3 bundles.
 *
 * Plan §M10 + §B4 (2026-05-28) — v1 mount path (`mountDeclarativeBundle`,
 * `ensureSeedBundlesInstalled`, `SEED_BUNDLE_FILENAMES`) removed. v3 JIT
 * fetch via `installDeclarativeBundleV3` is now the only install path.
 */

import {
  declarativeInstallV3,
  type DeclarativeInstallResult,
} from "../wasm-bridge";
import {
  BundleParseError,
  matchEntriesV3,
  parseBundleV3,
  type V3Bundle,
} from "./bundle-schema";
import {
  declarativeV3Cache,
  type DeclarativeV3CacheEntry,
} from "./declarative-v3-cache";

// ---------------------------------------------------------------------------
// M3 — v3 bundle hydration (JIT)
// ---------------------------------------------------------------------------

/**
 * Cached v3 install results keyed by canonical callkey. The cache is in-SW
 * only (no chrome.storage persistence yet) — fast enough for M3's
 * "SW DevTools console outputs the right Action JSON" exit criterion. The
 * `seen` flag lets us also avoid a redundant WASM round-trip when the
 * same bundle is reached via multiple callkeys (cartesian over
 * `chain_to_addresses`).
 */
const v3InstallCache = new Map<string, DeclarativeInstallResult>();

/**
 * Track bundles we already shipped through `declarative_install_v3_json`
 * (keyed by `bundle.id`). Re-installing is idempotent on the WASM side
 * (it overwrites the bridge entry + bundle map), so the latch is a pure
 * latency optimisation.
 */
const v3InstalledBundleIds = new Set<string>();

export interface DeclarativeRegistryV3Response {
  matched: boolean;
  bundle_id?: string;
  manifest_path?: string;
  bundle_sha256?: string;
  bundle?: unknown;
}

/**
 * Stage classification for {@link InstallDeclarativeV3Error}. Surfaced for
 * dashboards / audit so per-stage faults are colour-codeable.
 */
export type InstallV3Stage =
  | "fetch"
  | "fetch_status"
  | "fetch_json"
  | "parse"
  | "install";

export class InstallDeclarativeV3Error extends Error {
  constructor(
    readonly stage: InstallV3Stage,
    readonly source: string,
    cause: Error,
  ) {
    super(`install-declarative-v3[${stage}] ${source}: ${cause.message}`);
    this.name = "InstallDeclarativeV3Error";
    this.cause = cause;
  }
}

export interface InstallDeclarativeBundleV3Args {
  chainId: number;
  /** `to` address, any case — normalised internally. */
  to: string;
  /** `0x` + 8 hex chars, any case — normalised internally. */
  selector: string;
  /**
   * Base URL of the registry. Defaults to the build-time
   * `REGISTRY_BASE_URL`; M3 in-SW callers use the production Cloud Run
   * URL injected via webpack.
   */
  baseUrl?: string;
  /** Injected for tests — defaults to global `fetch`. */
  fetchImpl?: typeof fetch;
}

/**
 * Result returned to the orchestrator after a successful v3 hydration.
 * `bundle` is the parsed v3 manifest (passed through the parse gate); the
 * orchestrator uses `bundle_id` for audit telemetry and `matchEntriesV3`
 * lookups when prefetching child callkeys.
 */
export interface InstallDeclarativeV3Result {
  decoderId: string;
  bundleId: string;
  bundle: V3Bundle;
}

const DEFAULT_REGISTRY_BASE_URL =
  typeof process !== "undefined" && process.env?.REGISTRY_BASE_URL
    ? process.env.REGISTRY_BASE_URL
    : "http://localhost:8000";

function v3CallkeyCacheKey(
  chainId: number,
  to: string,
  selector: string,
): string {
  return `v3:${chainId}__${to.toLowerCase()}__${selector.toLowerCase()}`;
}

function v3CallkeyUrl(
  baseUrl: string,
  chainId: number,
  to: string,
  selector: string,
): string {
  const base = baseUrl.endsWith("/") ? baseUrl.slice(0, -1) : baseUrl;
  return `${base}/index/by-callkey/${chainId}__${to.toLowerCase()}__${selector.toLowerCase()}.json`;
}

/**
 * M3 — hydrate a v3 bundle for `(chainId, to, selector)` from the registry
 * and install it into the WASM engine.
 *
 * Pipeline:
 *   1. Cache lookup — return the prior `DeclarativeInstallResult` when the
 *      same callkey was already hydrated in this SW lifetime.
 *   2. Fetch the callkey index entry. Non-2xx surfaces `null` (treated as a
 *      miss). 404 + 5xx both fall into this — the caller short-circuits
 *      to the Tier B static path.
 *   3. JSON parse + `matched === true` guard.
 *   4. `parseBundleV3` shape gate — rejects v1/v2 payloads and structurally
 *      broken v3 ones. Parse errors are thrown (not nulled) so an
 *      operator misconfiguring the registry surfaces a clear error.
 *   5. Stringify + hand to `declarative_install_v3_json`. The stringified
 *      form mirrors what the registry sent — we do not canonicalise, so
 *      future `bundle_sha256` checks (M4+) can compare byte-stable input.
 *
 * Returns `null` when the registry response is a miss; throws
 * {@link InstallDeclarativeV3Error} on a hard fault that the caller MUST
 * surface (parse failure, install failure, network rejection that the
 * caller asked to treat as fatal).
 *
 * Error semantics — the SW orchestrator (M4) decides whether to treat a
 * thrown error as a v3 fault vs. transparent static fallback. M3 itself
 * only routes; it does not classify.
 */
export async function installDeclarativeBundleV3(
  args: InstallDeclarativeBundleV3Args,
): Promise<InstallDeclarativeV3Result | null> {
  const cacheKey = v3CallkeyCacheKey(args.chainId, args.to, args.selector);
  const cached = v3InstallCache.get(cacheKey);
  if (cached) {
    // Re-derive the parsed bundle from the install side-channel — the
    // cache stored only the install result. The orchestrator only needs
    // `decoderId` / `bundleId` for telemetry, but exposing the parsed
    // bundle keeps the API uniform with the cold path. We re-parse the
    // raw text we still hold via the WASM-side state map; since the SW
    // does not have direct access to that map, M3 carries `bundle`
    // alongside the cache.
    const parsedBundle = v3CachedBundleByCallKey.get(cacheKey);
    if (parsedBundle) {
      // Plan §M5 — 사용자 시각 확인용 console marker. cache hit path =
      // 같은 callkey 두 번째+ 호출. WASM in-memory `DECLARATIVE_V3_STATE`
      // thread_local 에는 이미 등록된 상태 (= chrome.storage 와 별개).
      console.info("[Scopeball] installDeclarativeBundleV3 cache-hit", {
        callkey: cacheKey,
        bundleId: cached.bundle_id,
        decoderId: cached.decoder_id,
      });
      return {
        decoderId: cached.decoder_id,
        bundleId: cached.bundle_id,
        bundle: parsedBundle,
      };
    }
    // Cache desync — fall through to a full re-hydration.
  }

  // Layer 2 — chrome.storage.local mirror (plan §M3 SW restart 영속화).
  // SW cold start 후 in-memory v3InstallCache 는 비어있지만 chrome.storage
  // 에 직전 lifetime 의 bundle 이 남아있을 수 있음. hit 시 WASM 에 다시
  // install 하고 in-memory cache 도 재구성 — registry-api-v3 round-trip 없음.
  try {
    const storageEntry = await declarativeV3Cache.get(cacheKey);
    if (storageEntry) {
      const reinstalled = await declarativeInstallV3(
        JSON.stringify(storageEntry.bundle),
      );
      v3InstallCache.set(cacheKey, reinstalled);
      v3CachedBundleByCallKey.set(cacheKey, storageEntry.bundle);
      v3InstalledBundleIds.add(reinstalled.bundle_id);
      // chain_to_addresses cross-mirror (Layer 1 측). storage 측은 이미
      // 직전 lifetime fresh-install 시 자체 cross-mirror 됨 — 본 hit 도
      // touch 로 LRU 만 갱신.
      const sel = storageEntry.bundle.match.selector.toLowerCase();
      for (const [cid, addr] of matchEntriesV3(storageEntry.bundle.match)) {
        const k = v3CallkeyCacheKey(cid, addr, sel);
        v3InstallCache.set(k, reinstalled);
        v3CachedBundleByCallKey.set(k, storageEntry.bundle);
      }
      console.info("[Scopeball] installDeclarativeBundleV3 storage-hit", {
        callkey: cacheKey,
        bundleId: reinstalled.bundle_id,
        decoderId: reinstalled.decoder_id,
      });
      return {
        decoderId: reinstalled.decoder_id,
        bundleId: reinstalled.bundle_id,
        bundle: storageEntry.bundle,
      };
    }
  } catch (err) {
    // chrome.storage 읽기 실패 / WASM install 실패 — 무음으로 cold fetch 로
    // 떨어뜨림. storage corruption 이 SW 를 brick 시키지 않게 함.
    console.warn(
      "[Scopeball] installDeclarativeBundleV3 storage rehydrate failed",
      err instanceof Error ? err.message : err,
    );
  }

  const baseUrl = args.baseUrl ?? DEFAULT_REGISTRY_BASE_URL;
  const doFetch = args.fetchImpl ?? fetch;
  const url = v3CallkeyUrl(baseUrl, args.chainId, args.to, args.selector);

  let response: Response;
  try {
    response = await doFetch(url);
  } catch (err) {
    throw new InstallDeclarativeV3Error(
      "fetch",
      url,
      err instanceof Error ? err : new Error(String(err)),
    );
  }

  if (response.status === 404) {
    return null;
  }
  if (!response.ok) {
    throw new InstallDeclarativeV3Error(
      "fetch_status",
      url,
      new Error(`HTTP ${response.status}`),
    );
  }

  let parsedResponse: DeclarativeRegistryV3Response;
  try {
    parsedResponse = (await response.json()) as DeclarativeRegistryV3Response;
  } catch (err) {
    throw new InstallDeclarativeV3Error(
      "fetch_json",
      url,
      err instanceof Error ? err : new Error(String(err)),
    );
  }

  if (!parsedResponse || parsedResponse.matched !== true) {
    return null;
  }

  let parsedBundle: V3Bundle | null;
  try {
    parsedBundle = parseBundleV3(parsedResponse.bundle);
  } catch (err) {
    if (err instanceof BundleParseError) {
      throw new InstallDeclarativeV3Error("parse", url, err);
    }
    throw err;
  }
  if (parsedBundle === null) {
    // Registry returned a non-v3 payload for a callkey the caller asked us
    // to hydrate via the v3 path. Treat as a miss so the caller can fall
    // back to the Tier B static path rather than throwing.
    return null;
  }

  const bundleJson = JSON.stringify(parsedResponse.bundle);
  let installed: DeclarativeInstallResult;
  try {
    installed = await declarativeInstallV3(bundleJson);
  } catch (err) {
    throw new InstallDeclarativeV3Error(
      "install",
      url,
      err instanceof Error ? err : new Error(String(err)),
    );
  }

  v3InstallCache.set(cacheKey, installed);
  v3CachedBundleByCallKey.set(cacheKey, parsedBundle);
  v3InstalledBundleIds.add(installed.bundle_id);

  // Expand `chain_to_addresses` so a subsequent callkey on a different
  // chain (same bundle) finds the cached install entry without a second
  // network round-trip. The WASM-side bridge already covers the engine
  // routing side; this is purely a JS cache mirror.
  const sel = parsedBundle.match.selector.toLowerCase();
  for (const [cid, addr] of matchEntriesV3(parsedBundle.match)) {
    const k = v3CallkeyCacheKey(cid, addr, sel);
    v3InstallCache.set(k, installed);
    v3CachedBundleByCallKey.set(k, parsedBundle);
  }

  // Layer 2 persist — mirror the fresh install into chrome.storage.local
  // so the next SW cold-start can rehydrate without another
  // registry-api-v3 fetch. We mirror EVERY callkey the bundle covers
  // (chain_to_addresses cartesian) under the same entry shape; the
  // serialized payload is small (single shared bundle reference per
  // record on disk after JSON serialize-deserialize) and the cap of 256
  // entries gates DoS.
  const fetchedAtMs = Date.now();
  const sha256 = parsedResponse.bundle_sha256 ?? "";
  const persistEntry: DeclarativeV3CacheEntry = {
    bundle: parsedBundle,
    bundleId: installed.bundle_id,
    decoderId: installed.decoder_id,
    bundleSha256: sha256,
    fetchedAtMs,
  };
  try {
    await declarativeV3Cache.put(cacheKey, persistEntry);
    for (const [cid, addr] of matchEntriesV3(parsedBundle.match)) {
      const k = v3CallkeyCacheKey(cid, addr, sel);
      if (k === cacheKey) continue; // already persisted above
      await declarativeV3Cache.put(k, persistEntry);
    }
  } catch (err) {
    // Persisting is best-effort — degrade gracefully so a storage fault
    // (quota exceeded etc.) cannot block the v3 install path itself.
    console.warn(
      "[Scopeball] installDeclarativeBundleV3 storage persist failed",
      err instanceof Error ? err.message : err,
    );
  }

  // Plan §M5 — 사용자 시각 확인용 console marker. fresh install path =
  // registry-api-v3 fetch + parseBundleV3 + WASM declarative_install_v3
  // 까지 통과 + chrome.storage.local mirror 완료. SW restart 후 같은
  // callkey 재진입 시 storage-hit 으로 떨어짐.
  console.info("[Scopeball] installDeclarativeBundleV3 fresh-install", {
    callkey: cacheKey,
    url,
    bundleId: installed.bundle_id,
    decoderId: installed.decoder_id,
    cachedCallkeys: v3InstallCache.size,
    installedBundleIds: v3InstalledBundleIds.size,
  });

  return {
    decoderId: installed.decoder_id,
    bundleId: installed.bundle_id,
    bundle: parsedBundle,
  };
}

const v3CachedBundleByCallKey = new Map<string, V3Bundle>();

/**
 * Test helper — drops the v3 install cache so successive vitest cases run
 * with a cold slate.
 */
export function __resetDeclarativeV3CacheForTest(): void {
  v3InstallCache.clear();
  v3CachedBundleByCallKey.clear();
  v3InstalledBundleIds.clear();
  declarativeV3Cache.reset();
}
