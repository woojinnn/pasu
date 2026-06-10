// SW boot-time manifest hydration.
//
// `install_policies_json` REPLACES all engine state on each call, so the
// hydrate path must pass BOTH the stored manifest map AND the
// currently-enabled policy set together — otherwise the Cedar policies
// installed earlier in the boot sequence would be wiped.
//
// Two stages, in order:
//   1. Cold-start restore — re-push the stored manifests + enabled policy
//      set into WASM so the engine boots with the correct schema.
//   2. Dev seeding — `devSeed()` configures the local endpoint URL in dev
//      builds; short-circuits in production.

import { devSeed, fetchBundledDefaultManifests } from "./dev-seed";
import {
  getAllManifests,
  setHash,
  type PolicyManifest,
} from "./store";
import { loadCurrentEnabledPolicySet } from "../policies-loader";
import {
  installPolicies as wasmInstallPolicies,
  type InstallPoliciesOutput,
} from "../wasm-bridge";

interface PolicyEntry {
  id: string;
  text: string;
}

export interface HydrateDeps {
  getAllManifests: () => Promise<Record<string, PolicyManifest>>;
  loadPolicySet: () => Promise<PolicyEntry[]>;
  wasmInstall: (input: {
    schema_text: string;
    policy_set: PolicyEntry[];
    manifests: Record<string, PolicyManifest>;
  }) => Promise<InstallPoliciesOutput | null>;
  setHash: (hash: string | null) => Promise<void>;
  devSeed: typeof devSeed;
  fetchDefaults: typeof fetchBundledDefaultManifests;
}

const DEFAULT_DEPS: HydrateDeps = {
  getAllManifests,
  loadPolicySet: loadCurrentEnabledPolicySet,
  wasmInstall: (input) =>
    wasmInstallPolicies({
      schema_text: input.schema_text,
      policy_set: input.policy_set,
      manifests: input.manifests,
    }),
  setHash,
  devSeed,
  fetchDefaults: fetchBundledDefaultManifests,
};

/**
 * Hydrate the WASM engine with stored manifests at SW boot. Optionally
 * inject dependency mocks for testing.
 *
 * Returns the install output (if any) for the cold-start stage; the
 * dev-seed stage's result is not surfaced here.
 */
export async function hydrateManifests(
  overrides: Partial<HydrateDeps> = {},
): Promise<InstallPoliciesOutput | null> {
  const deps: HydrateDeps = { ...DEFAULT_DEPS, ...overrides };
  let installed: InstallPoliciesOutput | null = null;

  const existing = await deps.getAllManifests();
  if (Object.keys(existing).length > 0) {
    // Pass the manifest map AND the enabled policy set together so neither
    // is wiped by the replace-all semantics of `install_policies_json`.
    const policySet = await deps.loadPolicySet();
    installed = await deps.wasmInstall({
      schema_text: "",
      policy_set: policySet,
      manifests: existing,
    });
    if (installed) {
      await deps.setHash(installed.enrichedSchemaHash);
    }
  }

  await deps.devSeed({
    fetchDefaults: deps.fetchDefaults,
    wasmInstall: async (manifests: Record<string, PolicyManifest>) => {
      // Re-read the enabled policy set so a freshly-enabled set is not
      // clobbered — dev-seed runs after cold-start restore, which may
      // have changed the enabled state.
      const policySet = await deps.loadPolicySet();
      return deps.wasmInstall({
        schema_text: "",
        policy_set: policySet,
        manifests,
      });
    },
  });

  return installed;
}
