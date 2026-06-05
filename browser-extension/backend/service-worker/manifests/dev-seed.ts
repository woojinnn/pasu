// Dev-build endpoint seeding.
//
// Phase 8: the manifest auto-seed step was removed. Previously this
// module copied `public/default-manifests/swap.policy-rpc.json` (and
// peers) into the user's storage on the first SW boot in dev, which
// made the builder UI show 9 manifest-driven enrichments out of the
// box but coupled every user's storage to the bundled set in a way
// that silently broke when the bundled list changed. The bundled
// manifest is now a STARTER PACK the user opts into from
// `/manifests/<action>` ("Install starter pack" button); this seeder
// only configures the local-dev endpoint URL so the new-user
// experience still has a working policy-rpc target without any
// manifest claims.
//
// Skip entirely in `NODE_ENV === "production"` — prod has never wanted
// any default endpoint either.

import * as store from "./store";

export interface DevSeedDeps {
  /**
   * Retained for backwards compatibility with `hydrateManifests` (which
   * passes a `fetchDefaults` closure through). Phase 8 doesn't consult
   * it — kept as a parameter so the import graph stays stable for any
   * out-of-tree callers and tests that pre-Phase-8 wired a mock.
   */
  fetchDefaults?: () => Promise<Record<string, store.PolicyManifest>>;
  /** Same as above — accepted but ignored. */
  wasmInstall?: unknown;
}

export const DEFAULT_DEV_ENDPOINT_URL = "http://localhost:8787";

export async function devSeed(_deps: DevSeedDeps): Promise<void> {
  if (process.env.NODE_ENV === "production") return;
  if (!(await store.getEndpointUrl())) {
    await store.setEndpointUrl(DEFAULT_DEV_ENDPOINT_URL);
  }
}

/**
 * Load the bundled "starter pack" manifests shipped under
 * `public/default-manifests/`. Used by:
 *
 *  - The manifest editor's "Install starter pack" button (Phase 8) — an
 *    EXPLICIT opt-in import the user clicks, not the SW boot hook.
 *  - The cold-start hydrate path's compatibility shim — passed in but
 *    no longer consulted by `devSeed` (Phase 8 removed the auto-seed
 *    behaviour).
 *
 * Returns `{}` when the asset bundle is absent (e.g. a release build
 * that skipped `copy-default-manifests.js`).
 */
export async function fetchBundledDefaultManifests(): Promise<
  Record<string, store.PolicyManifest>
> {
  // Import lazily so the SW bundle doesn't pay the webextension-polyfill
  // cost just to register this rarely-called helper. (No measurable
  // perf impact today; keeps the dev-seed module tree shake-friendly.)
  const Browser = (await import("webextension-polyfill")).default;
  const indexUrl = Browser.runtime.getURL("default-manifests/index.json");
  let indexJson: { action: string; file: string }[];
  try {
    const response = await fetch(indexUrl);
    if (!response.ok) return {};
    indexJson = (await response.json()) as { action: string; file: string }[];
  } catch {
    return {};
  }

  const out: Record<string, store.PolicyManifest> = {};
  for (const entry of indexJson) {
    try {
      const url = Browser.runtime.getURL(`default-manifests/${entry.file}`);
      const response = await fetch(url);
      if (!response.ok) continue;
      out[entry.action] = (await response.json()) as store.PolicyManifest;
    } catch (err) {
      console.warn(
        `[Pasu] dev-seed: failed to load starter-pack manifest for action=${entry.action}`,
        err,
      );
    }
  }
  return out;
}
