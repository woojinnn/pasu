/**
 * Default v3 decoder-bundle loader.
 *
 * The `declarative_route_request_v3_json` decoder (registered in
 * `crates/policy-engine-wasm/src/declarative_exports.rs`) is a sophisticated
 * routing engine that ALREADY emits typed `ActionBody` trees — BUT only when
 * a matching bundle was previously installed via `declarative_install_v3_json`.
 * Cold-start it has no bundles, so every route call falls through to
 * `ActionBody::Unknown` and the dashboard simulator sees opaque calldata.
 *
 * In production the registry-api JIT path (`/v1/registry/by-callkey`) installs
 * bundles on first hit; in the simulation / probe context that never fires
 * because the dashboard never goes through the real-tx flow. This module
 * ships a curated, build-time-bundled JSON asset (mirrors
 * `policies-loader-v2.ts`'s `policy-set-v2.json` pattern) and installs each
 * entry into the WASM engine at SW boot so the simulator decoder has
 * something to look up.
 *
 * Best-effort throughout — a failed fetch or per-bundle install logs and
 * continues. The decoder's existing Unknown-fallback keeps the route call
 * succeeding even if no bundle gets installed.
 */

import Browser from "webextension-polyfill";

import { declarativeInstallV3 } from "./wasm-bridge";

/** Module-level counter — incremented per successful install. Exported so a
 *  caller (the dashboard SimulationPage probe) can surface "N bundles loaded"
 *  in a status pill. */
let installedCount = 0;
let bootDone = false;

export function getInstalledV3BundleCount(): number {
  return installedCount;
}

/** Has the boot install pass completed (success OR clean failure)? Distinct
 *  from `installedCount > 0` because a fresh SW lifetime starts with the
 *  fetch in-flight; the dashboard probe wants to differentiate "warming up"
 *  from "finished and empty". */
export function v3BundleBootCompleted(): boolean {
  return bootDone;
}

/**
 * Fetch the bundled `bundles-v1.json` asset and pipe each bundle through
 * `declarative_install_v3_json`. Idempotent within a SW lifetime — repeat
 * invocations re-fetch (cheap) but the engine handles re-installs of the
 * same callkey by overwriting.
 *
 * Returns the number of bundles successfully installed. A `0` return is
 * benign (e.g. asset missing on dev builds without `prepare:defaults`),
 * never thrown — the dashboard probe surfaces it as a warning.
 */
export async function ensureDefaultV3BundlesInstalled(): Promise<number> {
  try {
    const url = Browser.runtime.getURL("default-v3-bundles/bundles-v1.json");
    const response = await fetch(url);
    if (!response.ok) {
      console.warn(
        `[Scopeball] v3 default bundles fetch failed: HTTP ${response.status} for ${url}`,
      );
      bootDone = true;
      return 0;
    }
    const text = await response.text();
    const parsed = JSON.parse(text) as unknown;
    if (!Array.isArray(parsed)) {
      console.warn("[Scopeball] v3 default bundles asset is not an array");
      bootDone = true;
      return 0;
    }
    let ok = 0;
    for (const bundle of parsed) {
      // declarative_install_v3_json's caller MUST pass the exact bundle text
      // it received from the registry — see the wasm-bridge docstring. We
      // re-stringify here because the JSON was parsed only to iterate; each
      // re-stringify is canonical-ish (key order may differ from the source
      // file) but byte stability with the registry isn't required for the
      // simulator path, which doesn't sha-check against an upstream pin.
      const bundleJson = JSON.stringify(bundle);
      try {
        const result = await declarativeInstallV3(bundleJson);
        installedCount += 1;
        ok += 1;
        console.debug(
          "[Scopeball] v3 bundle installed:",
          result.decoder_id ?? "<unknown>",
        );
      } catch (err) {
        // One bundle failing to install (e.g. schema drift, missing capability)
        // shouldn't poison the rest. Log + continue.
        console.warn(
          "[Scopeball] v3 bundle install failed:",
          err instanceof Error ? err.message : err,
        );
      }
    }
    bootDone = true;
    return ok;
  } catch (err) {
    console.warn(
      "[Scopeball] v3 default bundles load failed:",
      err instanceof Error ? err.message : err,
    );
    bootDone = true;
    return 0;
  }
}

/** Test helper — drop the module-level state so successive vitest cases
 *  re-fetch from a cold slate. Mirrors `__resetV2BundlesForTest` in the v2
 *  policy loader. */
export function __resetV3BundlesForTest(): void {
  installedCount = 0;
  bootDone = false;
}
