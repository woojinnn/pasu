// SW boot-time manifest hydration (Phase 6 / Task 6.3, with carry-over G
// fix from Phase 7.5).
//
// `install_policies_json` REPLACES engine state on every call
// (`crates/policy-engine-wasm/src/exports.rs` — `*state.borrow_mut() =
// Some(EngineState { policies, ... })`). The naive hydrate path
// — passing `policy_set: []` together with the stored manifest map —
// therefore wipes the Cedar policies that `ensureDefaultPoliciesInstalled`
// installed during the prior bootSequence stage.
//
// The fix here is to re-read the currently-enabled policy set via
// `loadCurrentEnabledPolicySet()` and pass it alongside the manifests,
// so a single install call sets BOTH state slots atomically. The unit
// test in `hydrate.test.ts` covers the regression.
//
// Hydration runs two stages, in order:
//   1. Cold-start restore — if `rpc:manifests` is non-empty, push the
//      stored map back into WASM with the currently-enabled policies so
//      the engine boots with the right schema.
//   2. Dev seeding — `devSeed()` fills in any missing default actions
//      from `public/default-manifests/`. Prod builds short-circuit
//      inside `devSeed`.

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
    // Cold-start restore: re-push the stored manifest map AND the
    // currently-enabled policy set into WASM so the policies installed
    // by `ensureDefaultPoliciesInstalled` aren't wiped by this call.
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

  // Dev seeding still uses `policy_set: []` because in dev the default
  // policies aren't yet relevant — `devSeed` itself only fires when
  // there are missing default actions and `NODE_ENV !== 'production'`.
  // In prod this short-circuits inside `devSeed`.
  await deps.devSeed({
    fetchDefaults: deps.fetchDefaults,
    wasmInstall: async (manifests) => {
      // dev-seed runs AFTER cold-start restore, so the engine already
      // holds the enabled policy set from the call above. Re-read it
      // (cheap; reads storage) so we don't clobber a freshly-enabled
      // set.
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
