/**
 * Declarative adapter loader — v3 bundle hydration (JIT).
 *
 * Spec: `ADAPTER_LOADER_ARCHITECTURE.md` §5.5 (TS bridge) and §7
 * (3-layer loading). The loader is the single install point onto the WASM
 * engine for v3 declarative bundles. The v3 path uses
 * `declarative_install_v3_json` (writes to `DECLARATIVE_V3_STATE`):
 *
 *   `installDeclarativeBundleV3({ chainId, to, selector })` — fetches the
 *   matching v3 manifest from the registry, shape-validates it via
 *   `parseBundleV3`, then forwards the
 *   *raw* JSON text to `declarativeInstallV3`. We deliberately do NOT
 *   re-stringify the parsed bundle: the parser is shape-only and could drop
 *   fields (or alter ordering) that the Rust deserializer depends on for
 *   stable hashing later. Pass-through keeps bytes identical.
 *
 * Idempotency: the engine overwrites existing mappers on re-install, so the
 * install is safe to call repeatedly; the in-SW + chrome.storage caches just
 * avoid the redundant fetch + WASM round-trip.
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
import { fetchStarted, fetchEnded } from "../diagnostics";

/**
 * Cached v3 install results keyed by canonical callkey. The process-local map
 * avoids redundant fetch + WASM install work within one service-worker
 * lifetime; chrome.storage provides the cross-restart cache below.
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
 * Stage classification for {@link InstallDeclarativeV3Error}. The orchestrator
 * logs this as v3 registry/install telemetry before failing closed.
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
   * `REGISTRY_BASE_URL`; in-SW callers use the URL injected via webpack.
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

/**
 * EIP-712 typed-data routing key. The 3-tuple
 * `(chainId, verifyingContract, primaryType)` selects a manifest in the
 * `by-typed-data/` index; the optional `witnessType` is the 4th routing-key
 * segment that de-collides Permit2 `permitWitnessTransferFrom` orders
 * (UniswapX intent orders etc. all share `(chain, Permit2, primaryType)`).
 */
export interface TypedDataMatchKey {
  chainId: number;
  /** EIP-712 `verifyingContract`, any case — normalised internally. */
  verifyingContract: string;
  /** EIP-712 `primaryType` discriminator. */
  primaryType: string;
  /** Optional 4th routing-key segment — the EIP-712 `witness` struct's type. */
  witnessType?: string;
}

/**
 * Result of {@link installDeclarativeBundleV3ByTypedData}. Flattened to a
 * boolean + reason so the SW sig-router maps a miss to a transparent `null`
 * fall-through (the orchestrator preserves the observability-only audit row).
 * `bundleId` is present iff `ok`.
 */
export interface InstallDeclarativeV3ByTypedDataResult {
  ok: boolean;
  bundleId?: string;
  reason?: string;
}

const DEFAULT_REGISTRY_BASE_URL =
  typeof process !== "undefined" && process.env && process.env.REGISTRY_BASE_URL
    ? process.env.REGISTRY_BASE_URL
    : "http://localhost:8000";

/**
 * Wrap a registry `fetch` with sent/received/duration + URL console logging so a
 * slow registry round-trip — the suspected cause of the `__engine::timeout` 8s
 * budget overrun — is visible in the SW DevTools: exactly which URL, when it was
 * sent, when it answered, and how long it took (wall-clock + ms).
 *
 * On a hang the "→ sent" line prints but the "← recv" line never does, so the
 * stuck URL is still identifiable even when no response ever arrives.
 */
async function timedRegistryFetch(
  doFetch: (url: string) => Promise<Response>,
  url: string,
  label: string,
): Promise<Response> {
  const sentAtMs = Date.now();
  const startedAt = performance.now();
  const traceSeq = fetchStarted(label, url);
  console.info("[Scopeball] registry-fetch → sent", {
    label,
    url,
    sentAt: new Date(sentAtMs).toISOString(),
  });
  try {
    const response = await doFetch(url);
    const durationMs = Math.round(performance.now() - startedAt);
    fetchEnded(traceSeq, response.status, durationMs);
    console.info("[Scopeball] registry-fetch ← recv", {
      label,
      url,
      sentAt: new Date(sentAtMs).toISOString(),
      receivedAt: new Date().toISOString(),
      durationMs,
      status: response.status,
    });
    return response;
  } catch (err) {
    const durationMs = Math.round(performance.now() - startedAt);
    fetchEnded(
      traceSeq,
      `error:${err instanceof Error ? err.message : String(err)}`,
      durationMs,
    );
    console.warn("[Scopeball] registry-fetch ✗ error", {
      label,
      url,
      sentAt: new Date(sentAtMs).toISOString(),
      durationMs,
      error: err instanceof Error ? err.message : String(err),
    });
    throw err;
  }
}

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

function v3TypedDataCacheKey(key: TypedDataMatchKey): string {
  const witnessSuffix =
    key.witnessType !== undefined ? `__${key.witnessType}` : "";
  return `td:${key.chainId}__${key.verifyingContract.toLowerCase()}__${key.primaryType}${witnessSuffix}`;
}

/**
 * Build the `by-typed-data/` index URL. MUST mirror build-index's
 * `typedDataFilename` byte-for-byte — `primaryType` / `witnessType` colons
 * escaped to `__` (EIP-712 types like HyperLiquid's
 * `HyperliquidTransaction:UsdSend`), `verifyingContract` lowercased, and the
 * `witnessType` 4th segment omitted when absent — or the live SW 404s against
 * the generated index file.
 */
function v3TypedDataUrl(baseUrl: string, key: TypedDataMatchKey): string {
  const base = baseUrl.endsWith("/") ? baseUrl.slice(0, -1) : baseUrl;
  const ptEscaped = key.primaryType.replace(/:/g, "__");
  let file = `${key.chainId}__${key.verifyingContract.toLowerCase()}__${ptEscaped}`;
  if (key.witnessType !== undefined) {
    file += `__${key.witnessType.replace(/:/g, "__")}`;
  }
  return `${base}/index/by-typed-data/${file}.json`;
}

function v3SelectorCacheKey(chainId: number, selector: string): string {
  return `sel:${chainId}__${selector.toLowerCase()}`;
}

/**
 * Build the `by-selector/` index URL for an address-agnostic adapter. MUST
 * mirror build-index's `selectorFilename` (`<chainId>__<selector.lower>.json`)
 * — no address segment.
 */
function v3SelectorUrl(
  baseUrl: string,
  chainId: number,
  selector: string,
): string {
  const base = baseUrl.endsWith("/") ? baseUrl.slice(0, -1) : baseUrl;
  return `${base}/index/by-selector/${chainId}__${selector.toLowerCase()}.json`;
}

/**
 * Hydrate a v3 bundle for `(chainId, to, selector)` from the registry and
 * install it into the WASM engine.
 *
 * Pipeline:
 *   1. Cache lookup — return the prior `DeclarativeInstallResult` when the
 *      same callkey was already hydrated in this SW lifetime.
 *   2. Fetch the callkey index entry. Non-2xx surfaces `null` (treated as a
 *      miss). 404 + 5xx both fall into this — the caller fails closed with
 *      a warn verdict rather than using a legacy fallback.
 *   3. JSON parse + `matched === true` guard.
 *   4. `parseBundleV3` shape gate — rejects v1/v2 payloads and structurally
 *      broken v3 ones. Parse errors are thrown (not nulled) so an
 *      operator misconfiguring the registry surfaces a clear error.
 *   5. Stringify + hand to `declarative_install_v3_json`. The stringified
 *      form mirrors what the registry sent — we do not canonicalise, so
 *      future `bundle_sha256` checks can compare byte-stable input.
 *
 * Returns `null` when the registry response is a miss; throws
 * {@link InstallDeclarativeV3Error} on a hard fault that the caller MUST
 * surface (parse failure, install failure, network rejection that the
 * caller asked to treat as fatal).
 *
 * Error semantics — the SW orchestrator decides whether to treat a thrown
 * error as a v3 fault. This loader only fetches, validates, and installs.
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
    // does not have direct access to that map, this loader carries `bundle`
    // alongside the cache.
    const parsedBundle = v3CachedBundleByCallKey.get(cacheKey);
    if (parsedBundle) {
      // Cache-hit marker for repeat calls in the same service-worker lifetime;
      // WASM already has this bundle installed in its in-memory v3 state.
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

  // chrome.storage.local mirror. After a service-worker cold start the
  // process-local cache is empty, but a prior lifetime may have persisted the
  // bundle. On hit, reinstall into WASM and rebuild the in-memory cache without
  // a registry round-trip.
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
    response = await timedRegistryFetch(doFetch, url, "callkey");
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
    // to hydrate via the v3 path. Treat as a miss; the orchestrator owns the
    // resulting fail-closed verdict/audit behavior.
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

  // Layer 2 persist — mirror the fresh install into chrome.storage.local so
  // the next SW cold-start can rehydrate THIS callkey without another
  // registry-api-v3 fetch. We persist ONLY the requested callkey, NOT the
  // bundle's full `chain_to_addresses` cartesian. Rationale: every
  // `declarativeV3Cache.put` re-serializes the ENTIRE growing record and
  // does a full `storage.local.set` (see declarative-v3-cache.ts), so the
  // old fan-out — one awaited put per address — was O(N²) writes. A
  // large-match bundle (e.g. `standard/erc20/approve`, ~3891 token
  // addresses) spent ~7.7s in this loop on first install, blowing the
  // `HARD_TIMEOUT_MS = 8000` decision budget (the `__engine::timeout`
  // 8s overrun). Coverage is unaffected: siblings stay routable for THIS SW
  // lifetime via the in-memory mirror above + the WASM bridge (one install
  // bridges every address in the served match); on a cold SW restart an
  // as-yet-unseen sibling token simply re-fetches its own tiny by-callkey
  // file on demand — a cache-warmth trade, never a route miss. (The old
  // `MAX_CALLKEYS = 256` cap already evicted all but the last 256 fanned-out
  // entries anyway, so almost no durable warmth is lost.)
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
  } catch (err) {
    // Persisting is best-effort — degrade gracefully so a storage fault
    // (quota exceeded etc.) cannot block the v3 install path itself.
    console.warn(
      "[Scopeball] installDeclarativeBundleV3 storage persist failed",
      err instanceof Error ? err.message : err,
    );
  }

  // Fresh-install marker: registry fetch, parseBundleV3, WASM install, and
  // best-effort chrome.storage mirroring all completed for this callkey.
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
 * Hydrate and install the v3 bundle for an EIP-712 typed-data key
 * `(chainId, verifyingContract, primaryType[, witnessType])` so a subsequent
 * `declarative_route_typed_data_v3_json` finds it. The install populates the
 * WASM typed_data bridge the same `declarative_install_v3_json` call backs for
 * the callkey path — there is no separate typed-data install primitive.
 *
 * Mirrors {@link installDeclarativeBundleV3} but fetches the `by-typed-data/`
 * index (URL via {@link v3TypedDataUrl}) instead of `by-callkey/`, and returns
 * a flattened `{ ok, bundleId?, reason? }` so the sig router treats ANY
 * miss/fault as a `null` fall-through. A single bad publisher must not brick
 * the sign path, so fetch / parse / install faults all return
 * `{ ok: false, reason }` rather than throw. Memory cache only — a typed-data
 * install is cheap to re-fetch on SW cold start and the WASM install is
 * idempotent; the shared caches are reused under the `td:`-prefixed key so
 * they cannot collide with the `v3:` callkey entries.
 */
export async function installDeclarativeBundleV3ByTypedData(
  key: TypedDataMatchKey,
  options: { baseUrl?: string; fetchImpl?: typeof fetch } = {},
): Promise<InstallDeclarativeV3ByTypedDataResult> {
  const cacheKey = v3TypedDataCacheKey(key);
  const cached = v3InstallCache.get(cacheKey);
  if (cached) {
    console.info(
      "[Scopeball] installDeclarativeBundleV3ByTypedData cache-hit",
      {
        typedDataKey: cacheKey,
        bundleId: cached.bundle_id,
        decoderId: cached.decoder_id,
      },
    );
    return { ok: true, bundleId: cached.bundle_id };
  }

  const baseUrl = options.baseUrl ?? DEFAULT_REGISTRY_BASE_URL;
  const doFetch = options.fetchImpl ?? fetch;
  const url = v3TypedDataUrl(baseUrl, key);

  let response: Response;
  try {
    response = await timedRegistryFetch(doFetch, url, "typed-data");
  } catch (err) {
    console.warn(
      "[Scopeball] installDeclarativeBundleV3ByTypedData fetch failed",
      {
        typedDataKey: cacheKey,
        message: err instanceof Error ? err.message : err,
      },
    );
    return { ok: false, reason: "fetch_failed" };
  }

  if (response.status === 404) {
    return { ok: false, reason: "manifest_not_found" };
  }
  if (!response.ok) {
    return { ok: false, reason: `fetch_status_${response.status}` };
  }

  let parsedResponse: DeclarativeRegistryV3Response;
  try {
    parsedResponse = (await response.json()) as DeclarativeRegistryV3Response;
  } catch (err) {
    console.warn(
      "[Scopeball] installDeclarativeBundleV3ByTypedData json parse failed",
      {
        typedDataKey: cacheKey,
        message: err instanceof Error ? err.message : err,
      },
    );
    return { ok: false, reason: "fetch_json_failed" };
  }

  if (!parsedResponse || parsedResponse.matched !== true) {
    return { ok: false, reason: "manifest_not_found" };
  }

  let parsedBundle: V3Bundle | null;
  try {
    parsedBundle = parseBundleV3(parsedResponse.bundle);
  } catch (err) {
    if (err instanceof BundleParseError) {
      console.warn(
        "[Scopeball] installDeclarativeBundleV3ByTypedData parse failed",
        { typedDataKey: cacheKey, message: err.message },
      );
      return { ok: false, reason: "parse_failed" };
    }
    throw err;
  }
  if (parsedBundle === null) {
    return { ok: false, reason: "not_v3_bundle" };
  }

  const bundleJson = JSON.stringify(parsedResponse.bundle);
  let installed: DeclarativeInstallResult;
  try {
    installed = await declarativeInstallV3(bundleJson);
  } catch (err) {
    console.warn(
      "[Scopeball] installDeclarativeBundleV3ByTypedData install failed",
      {
        typedDataKey: cacheKey,
        message: err instanceof Error ? err.message : err,
      },
    );
    return { ok: false, reason: "install_failed" };
  }

  v3InstallCache.set(cacheKey, installed);
  v3CachedBundleByCallKey.set(cacheKey, parsedBundle);
  v3InstalledBundleIds.add(installed.bundle_id);

  console.info(
    "[Scopeball] installDeclarativeBundleV3ByTypedData fresh-install",
    {
      typedDataKey: cacheKey,
      url,
      bundleId: installed.bundle_id,
      decoderId: installed.decoder_id,
    },
  );

  return { ok: true, bundleId: installed.bundle_id };
}

/**
 * Address-agnostic (selector-only) hydrate + install — standard NFT
 * `setApprovalForAll`. Fetches the `by-selector/` index entry for
 * `(chainId, selector)` and installs it; the WASM route then reaches it via the
 * `selector_bridge` after a per-address callkey miss.
 *
 * Mirrors {@link installDeclarativeBundleV3} but fetches the `by-selector/`
 * index (URL via {@link v3SelectorUrl}). Returns the install result, or `null`
 * on ANY miss/fault (404 / parse / install). It NEVER throws: the on-chain tx
 * flow is warn-closed, so a fault here degrades to the same fail-closed warn as
 * a plain miss. Memory cache only (re-fetch on SW cold start is cheap; the WASM
 * install is idempotent) under the `sel:`-prefixed key so it cannot collide
 * with the `v3:` callkey or `td:` typed-data entries.
 */
export async function installDeclarativeBundleV3BySelector(args: {
  chainId: number;
  selector: string;
  baseUrl?: string;
  fetchImpl?: typeof fetch;
}): Promise<InstallDeclarativeV3Result | null> {
  const cacheKey = v3SelectorCacheKey(args.chainId, args.selector);
  const cached = v3InstallCache.get(cacheKey);
  const cachedBundle = v3CachedBundleByCallKey.get(cacheKey);
  if (cached && cachedBundle) {
    return {
      decoderId: cached.decoder_id,
      bundleId: cached.bundle_id,
      bundle: cachedBundle,
    };
  }

  const baseUrl = args.baseUrl ?? DEFAULT_REGISTRY_BASE_URL;
  const doFetch = args.fetchImpl ?? fetch;
  const url = v3SelectorUrl(baseUrl, args.chainId, args.selector);

  let response: Response;
  try {
    response = await doFetch(url);
  } catch (err) {
    console.warn("[Scopeball] installDeclarativeBundleV3BySelector fetch failed", {
      selectorKey: cacheKey,
      message: err instanceof Error ? err.message : err,
    });
    return null;
  }

  if (response.status === 404) return null;
  if (!response.ok) {
    console.warn("[Scopeball] installDeclarativeBundleV3BySelector fetch_status", {
      selectorKey: cacheKey,
      status: response.status,
    });
    return null;
  }

  let parsedResponse: DeclarativeRegistryV3Response;
  try {
    parsedResponse = (await response.json()) as DeclarativeRegistryV3Response;
  } catch (err) {
    console.warn("[Scopeball] installDeclarativeBundleV3BySelector json parse failed", {
      selectorKey: cacheKey,
      message: err instanceof Error ? err.message : err,
    });
    return null;
  }

  if (!parsedResponse || parsedResponse.matched !== true) return null;

  let parsedBundle: V3Bundle | null;
  try {
    parsedBundle = parseBundleV3(parsedResponse.bundle);
  } catch (err) {
    if (err instanceof BundleParseError) {
      console.warn("[Scopeball] installDeclarativeBundleV3BySelector parse failed", {
        selectorKey: cacheKey,
        message: err.message,
      });
      return null;
    }
    throw err;
  }
  if (parsedBundle === null) return null;

  const bundleJson = JSON.stringify(parsedResponse.bundle);
  let installed: DeclarativeInstallResult;
  try {
    installed = await declarativeInstallV3(bundleJson);
  } catch (err) {
    console.warn("[Scopeball] installDeclarativeBundleV3BySelector install failed", {
      selectorKey: cacheKey,
      message: err instanceof Error ? err.message : err,
    });
    return null;
  }

  v3InstallCache.set(cacheKey, installed);
  v3CachedBundleByCallKey.set(cacheKey, parsedBundle);
  v3InstalledBundleIds.add(installed.bundle_id);

  console.info("[Scopeball] installDeclarativeBundleV3BySelector fresh-install", {
    selectorKey: cacheKey,
    url,
    bundleId: installed.bundle_id,
    decoderId: installed.decoder_id,
  });

  return {
    decoderId: installed.decoder_id,
    bundleId: installed.bundle_id,
    bundle: parsedBundle,
  };
}

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
