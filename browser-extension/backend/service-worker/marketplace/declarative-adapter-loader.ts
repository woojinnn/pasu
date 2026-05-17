/**
 * Phase 1B — declarative adapter loader.
 *
 * Spec: `ADAPTER_MARKETPLACE_ARCHITECTURE.md` §5.5 (TS bridge) and §7
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
  installDeclarativeBundle,
  type DeclarativeInstallResult,
} from "../wasm-bridge";
import {
  BundleParseError,
  parseBundle,
} from "./bundle-schema";

export interface MountResult {
  decoderId: string;
  bundleId: string;
}

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
  try {
    parseBundle(parsedShape);
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
  return {
    decoderId: installed.decoder_id,
    bundleId: installed.bundle_id,
  };
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
 * re-trigger `ensureSeedBundlesInstalled` from a cold slate.
 */
export function __resetSeedBundlesForTest(): void {
  seededOnce = null;
}
