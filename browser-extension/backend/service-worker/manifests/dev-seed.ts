// Dev-build endpoint seeding.
//
// In dev, ensures the local policy-rpc endpoint URL is set so the new-user
// experience has a working target without requiring any manifest claims.
// The bundled manifests are an explicit opt-in ("Install starter pack") —
// not auto-seeded — so user storage stays decoupled from the bundled set.
//
// Skipped entirely in `NODE_ENV === "production"`.

import * as store from "./store";

export interface DevSeedDeps {
  /**
   * Accepted but ignored by `devSeed` — kept so callers that pass a
   * `fetchDefaults` closure (e.g. `hydrateManifests`) compile without changes.
   */
  fetchDefaults?: () => Promise<Record<string, store.PolicyManifest>>;
  /** Accepted but ignored. */
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
 * `public/default-manifests/`. Used by the manifest editor's "Install
 * starter pack" button — an explicit opt-in, not an auto-seed at SW boot.
 *
 * Returns `{}` when the asset bundle is absent (e.g. a release build
 * that skipped `copy-default-manifests.js`).
 */
export async function fetchBundledDefaultManifests(): Promise<
  Record<string, store.PolicyManifest>
> {
  // Import lazily to keep the dev-seed module tree-shake-friendly.
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
