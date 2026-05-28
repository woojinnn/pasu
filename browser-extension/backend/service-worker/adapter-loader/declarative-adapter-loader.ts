/**
 * Phase 1B — declarative adapter loader.
 *
 * Spec: `ADAPTER_LOADER_ARCHITECTURE.md` §5.5 (TS bridge) and §7
 * (3-layer loading). The loader is the single mount point onto the WASM
 * engine for declarative bundles:
 *
 *   1. `mountDeclarativeBundle(bundleJson)` — shape-validates the bundle via
 *      `parseBundle` (Phase 0 hand-written validator), then forwards the
 *      *raw* JSON text to `installDeclarativeBundle`. We deliberately do
 *      NOT re-stringify the parsed bundle: the parser is shape-only and
 *      could drop fields (or alter ordering) that the Rust deserializer
 *      depends on for stable hashing later. Pass-through keeps bytes
 *      identical from disk → engine.
 *
 *   2. `ensureSeedBundlesInstalled()` — fetches every JSON file in
 *      `public/seed-bundles/` and mounts it. The Phase 2 JIT-fetcher will
 *      call the same `mountDeclarativeBundle` entry point with bundles
 *      fetched from the network registry; only the source differs.
 *
 * Idempotency: the engine replaces existing mappers on re-install, so
 * `ensureSeedBundlesInstalled` is safe to call on every SW boot. The
 * `seededOnce` latch just avoids the redundant fetch + WASM round-trip
 * within a single SW lifetime.
 */

import Browser from "webextension-polyfill";
import {
  declarativeInstallV3,
  installDeclarativeBundle,
  type DeclarativeInstallResult,
} from "../wasm-bridge";
import {
  BundleParseError,
  matchEntries,
  matchEntriesV3,
  parseBundle,
  parseBundleV3,
  type AdapterFunctionBundle,
  type V3Bundle,
} from "./bundle-schema";

export interface MountResult {
  decoderId: string;
  bundleId: string;
  /**
   * Phase 6 — parsed bundle kept alongside the mount result so the
   * orchestrator can decode raw calldata against `bundle.abi_fragment` (via
   * viem) before forwarding to the WASM route entry. The bundle is the
   * SAME object the schema parser validated; downstream code MUST treat it
   * as read-only.
   */
  bundle: AdapterFunctionBundle;
}

/**
 * Phase 2B — registry of every bundle currently mounted in the WASM
 * engine, keyed by the canonical callkey
 * (`${chain_id}__${to.toLowerCase()}__${selector.toLowerCase()}`). Used by
 * the JIT fetcher's Layer 1 / Layer 2 lookup before reaching for the
 * network.
 *
 * One bundle can register many callkeys (cartesian product of
 * `match.chain_ids × match.to`), so we eagerly expand the match table at
 * mount time. The same `MountResult` is shared across all expanded keys.
 */
const mountedByCallKey = new Map<string, MountResult>();

/**
 * Index of the seed bundles shipped inside the extension. Mirrors what
 * `scripts/copy-default-policies.js` writes into `public/seed-bundles/`.
 * Kept here as a static list rather than enumerating the directory because
 * the SW can't `readdir` chrome-extension://… URLs at runtime — it must
 * fetch each known path explicitly.
 */
const SEED_BUNDLE_FILENAMES: readonly string[] = [
  "uniswap-v2-swapExactTokensForTokens@1.0.0.json",
];

export class DeclarativeAdapterLoadError extends Error {
  constructor(
    readonly stage:
      | "parse"
      | "install"
      | "fetch"
      | "fetch_status"
      | "fetch_json",
    readonly source: string,
    cause: Error,
  ) {
    super(`declarative-adapter-loader[${stage}] ${source}: ${cause.message}`);
    this.name = "DeclarativeAdapterLoadError";
    this.cause = cause;
  }
}

/**
 * Mount a single declarative bundle into the WASM engine.
 *
 * Validation pipeline (matches §5.5):
 *   1. Parse JSON text.
 *   2. Shape-validate via `parseBundle` so callers get structured
 *      `BundleParseError` instead of opaque Rust serde failures.
 *   3. Forward the *original* `bundleJson` (not the parsed object) to the
 *      engine — pass-through preserves byte ordering for future content
 *      hashing.
 */
export async function mountDeclarativeBundle(
  bundleJson: string,
): Promise<MountResult> {
  // Parse raw text first so a malformed file produces a single,
  // user-comprehensible error instead of cascading into the engine.
  let parsedShape: unknown;
  try {
    parsedShape = JSON.parse(bundleJson);
  } catch (err) {
    throw new DeclarativeAdapterLoadError(
      "parse",
      "<inline>",
      err instanceof Error ? err : new Error(String(err)),
    );
  }
  // Phase 0 validator — catches malformed BNF before we round-trip to WASM.
  let parsedBundle: AdapterFunctionBundle;
  try {
    parsedBundle = parseBundle(parsedShape);
  } catch (err) {
    if (err instanceof BundleParseError) {
      throw new DeclarativeAdapterLoadError("parse", "<inline>", err);
    }
    throw err;
  }

  let installed: DeclarativeInstallResult;
  try {
    installed = await installDeclarativeBundle(bundleJson);
  } catch (err) {
    throw new DeclarativeAdapterLoadError(
      "install",
      "<inline>",
      err instanceof Error ? err : new Error(String(err)),
    );
  }
  const result: MountResult = {
    decoderId: installed.decoder_id,
    bundleId: installed.bundle_id,
    bundle: parsedBundle,
  };
  // Phase 2B — expand the bundle's match table into the callkey lookup so
  // the JIT fetcher can detect Layer 1 hits without a network round-trip.
  registerCallKeys(parsedBundle, result);
  return result;
}

function registerCallKeys(
  bundle: AdapterFunctionBundle,
  result: MountResult,
): void {
  const sel = bundle.match.selector.toLowerCase();
  for (const [chainId, to] of matchEntries(bundle.match)) {
    const key = `${chainId}__${to.toLowerCase()}__${sel}`;
    mountedByCallKey.set(key, result);
  }
}

/**
 * Phase 2B — Layer 1 lookup. Given a callkey, return the already-mounted
 * adapter if one matches, or `null` otherwise. Callkey format mirrors the
 * registry filename convention:
 * `${chain_id}__${to.toLowerCase()}__${selector.toLowerCase()}`.
 */
export function lookupMountedBundle(
  chainId: number,
  to: string,
  selector: string,
): MountResult | null {
  const key = `${chainId}__${to.toLowerCase()}__${selector.toLowerCase()}`;
  return mountedByCallKey.get(key) ?? null;
}

let seededOnce: Promise<void> | null = null;

/**
 * Best-effort idempotent boot hook. Fetches every shipped seed bundle and
 * mounts it. Failures in a single bundle are logged but do not abort the
 * remaining bundles — a corrupt seed must not brick the SW.
 *
 * Returns the cached promise on subsequent calls within a single SW
 * lifetime so the boot path (`index.ts`) and the orchestrator entry can
 * both invoke it freely.
 */
export function ensureSeedBundlesInstalled(): Promise<void> {
  if (seededOnce) return seededOnce;
  seededOnce = (async () => {
    const results = await Promise.allSettled(
      SEED_BUNDLE_FILENAMES.map(async (filename) => {
        const url = Browser.runtime.getURL(`seed-bundles/${filename}`);
        const text = await fetchSeedBundle(url, filename);
        const mount = await mountDeclarativeBundle(text);
        return { filename, mount };
      }),
    );
    for (const r of results) {
      if (r.status === "fulfilled") {
        console.info("[Scopeball] seed bundle mounted", {
          filename: r.value.filename,
          decoderId: r.value.mount.decoderId,
          bundleId: r.value.mount.bundleId,
        });
      } else {
        console.warn("[Scopeball] seed bundle mount failed", {
          reason:
            r.reason instanceof Error ? r.reason.message : String(r.reason),
        });
      }
    }
  })();
  // Surface errors but never block another boot attempt — clear the latch
  // on reject so the next SW boot retries.
  seededOnce.catch(() => {
    seededOnce = null;
  });
  return seededOnce;
}

async function fetchSeedBundle(url: string, filename: string): Promise<string> {
  let response: Response;
  try {
    response = await fetch(url);
  } catch (err) {
    throw new DeclarativeAdapterLoadError(
      "fetch",
      filename,
      err instanceof Error ? err : new Error(String(err)),
    );
  }
  if (!response.ok) {
    throw new DeclarativeAdapterLoadError(
      "fetch_status",
      filename,
      new Error(`HTTP ${response.status} for ${url}`),
    );
  }
  try {
    return await response.text();
  } catch (err) {
    throw new DeclarativeAdapterLoadError(
      "fetch_json",
      filename,
      err instanceof Error ? err : new Error(String(err)),
    );
  }
}

/**
 * Test helper — drop the cached promise so successive vitest cases can
 * re-trigger `ensureSeedBundlesInstalled` from a cold slate. Also wipes
 * the callkey lookup so a Layer 1 hit from a previous case can't bleed
 * into the next.
 */
export function __resetSeedBundlesForTest(): void {
  seededOnce = null;
  mountedByCallKey.clear();
}

// ---------------------------------------------------------------------------
// M3 — v3 bundle hydration (JIT)
// ---------------------------------------------------------------------------
//
// Parallel to the v1 mount path above. The v3 path uses
// `declarative_install_v3_json` (writes to `DECLARATIVE_V3_STATE`) so the
// two install layers coexist while the cutover is in flight. M3 deliberately
// keeps the v1 mount path (`mountDeclarativeBundle`,
// `lookupMountedBundle`) untouched — both registries serve their own
// route entry.

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
 * Stage classification for {@link InstallDeclarativeV3Error}. Mirrors
 * `DeclarativeAdapterLoadError.stage` so dashboards can colour-code v1 / v3
 * faults uniformly.
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
 *      to the v1 path / Tier B static.
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
 * thrown error as a v3 fault vs. a transparent v1 fallback. M3 itself
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
      return {
        decoderId: cached.decoder_id,
        bundleId: cached.bundle_id,
        bundle: parsedBundle,
      };
    }
    // Cache desync — fall through to a full re-hydration.
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
    // back to v1 (`mountDeclarativeBundle`) rather than throwing — this
    // mirrors how the orchestrator handles `no_declarative_mapper` from
    // the v1 path.
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
  for (const [cid, addr] of matchEntriesV3(parsedBundle.match)) {
    const sel = parsedBundle.match.selector.toLowerCase();
    const k = v3CallkeyCacheKey(cid, addr, sel);
    v3InstallCache.set(k, installed);
    v3CachedBundleByCallKey.set(k, parsedBundle);
  }

  return {
    decoderId: installed.decoder_id,
    bundleId: installed.bundle_id,
    bundle: parsedBundle,
  };
}

const v3CachedBundleByCallKey = new Map<string, V3Bundle>();

/**
 * Test helper — drops the v3 install cache so successive vitest cases run
 * with a cold slate. Mirrors `__resetSeedBundlesForTest` for the v1 path.
 */
export function __resetDeclarativeV3CacheForTest(): void {
  v3InstallCache.clear();
  v3CachedBundleByCallKey.clear();
  v3InstalledBundleIds.clear();
}
