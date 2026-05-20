// Dev-build default-manifest seeding.
//
// Per spec D11 the prod build ships zero manifests by default — the
// onboarding flow asks the user to pick a policy-rpc endpoint and to
// install manifests via the dashboard. In dev we want a working
// experience out of the box: the bundled defaults under
// `public/default-manifests/` (copied by `scripts/copy-default-manifests.js`)
// get seeded into storage on the first SW boot.
//
// Seeding rules:
// - Skip entirely when `NODE_ENV === "production"`.
// - Only fill in actions that don't already have a manifest. User edits
//   from a previous SW lifetime stay intact.
// - When at least one new action gets added, run a Map-shape WASM
//   install via `atomicInstall` so storage + engine stay in sync.
// - Also default the policy-rpc endpoint to `http://localhost:8787`
//   when nothing is configured yet (matches the local dev server).

import Browser from "webextension-polyfill";
import { atomicInstall, type WasmInstallFn } from "./atomic-install";
import * as store from "./store";

export interface DevSeedDeps {
  fetchDefaults: () => Promise<Record<string, store.PolicyManifest>>;
  wasmInstall: WasmInstallFn;
}

export const DEFAULT_DEV_ENDPOINT_URL = "http://localhost:8787";

export async function devSeed(deps: DevSeedDeps): Promise<void> {
  if (process.env.NODE_ENV === "production") return;

  const defaults = await deps.fetchDefaults();
  const existing = await store.getAllManifests();
  const next: Record<string, store.PolicyManifest> = { ...existing };
  let added = false;
  for (const [action, manifest] of Object.entries(defaults)) {
    if (!next[action]) {
      next[action] = manifest;
      added = true;
    }
  }
  if (!added) return;

  if (!(await store.getEndpointUrl())) {
    await store.setEndpointUrl(DEFAULT_DEV_ENDPOINT_URL);
  }

  await atomicInstall(next, { wasmInstall: deps.wasmInstall });
}

/**
 * Production helper: fetch the bundled `default-manifests/index.json`
 * and load every listed file. Returns `{}` when the asset bundle is
 * absent (e.g. the `copy-default-manifests.js` script skipped prod).
 */
export async function fetchBundledDefaultManifests(): Promise<
  Record<string, store.PolicyManifest>
> {
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
        `[Scopeball] dev-seed: failed to load default manifest for action=${entry.action}`,
        err,
      );
    }
  }
  return out;
}
