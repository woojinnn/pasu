// Cold-start hydrate regression coverage (Phase 7.5 carry-over G).
//
// `install_policies_json` REPLACES engine state — a hydrate call that
// passed `policy_set: []` would therefore wipe the Cedar policies that
// `ensureDefaultPoliciesInstalled` had just installed. This test pins
// the fix: hydrate must re-read the currently-enabled policy set and
// pass it alongside the stored manifest map so a single WASM install
// sets BOTH slots atomically.

import { describe, expect, it, vi } from "vitest";

// The hydrate module's transitive imports (store / policies-loader /
// wasm-bridge) all pull in webextension-polyfill, which throws when
// loaded outside an extension context. Stub it before any other
// import — `vi.hoisted` runs before module evaluation.
const mocks = vi.hoisted(() => ({
  browser: {
    runtime: { getURL: (p: string) => `chrome-extension://x/${p}` },
    storage: {
      local: {
        get: vi.fn(async () => ({})),
        set: vi.fn(async () => {}),
        remove: vi.fn(async () => {}),
      },
    },
  },
}));
vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));

import { hydrateManifests } from "./hydrate";
import type { PolicyManifest } from "./store";

const SAMPLE_POLICY = {
  id: "default::dex/a",
  text: '@id("default::dex/a") @severity("deny") @reason("a") forbid (principal, action, resource);',
};

const SAMPLE_MANIFEST: PolicyManifest = {
  id: "user.swap.v1",
  schema_version: 1,
  requires: [],
};

describe("hydrateManifests (cold-start, carry-over G)", () => {
  it("passes the currently-enabled policy set alongside stored manifests so installed policies aren't wiped", async () => {
    const wasmInstall = vi.fn(
      async (_input: {
        schema_text: string;
        policy_set: typeof SAMPLE_POLICY[];
        manifests: Record<string, PolicyManifest>;
      }) => ({
        enrichedSchemaHash: "sha256:test",
        addedCustomFields: {},
      }),
    );
    const setHash = vi.fn(async () => {});
    const devSeed = vi.fn(async () => {});

    await hydrateManifests({
      getAllManifests: async () => ({ swap: SAMPLE_MANIFEST }),
      loadPolicySet: async () => [SAMPLE_POLICY],
      wasmInstall,
      setHash,
      devSeed,
      fetchDefaults: async () => ({}),
    });

    expect(wasmInstall).toHaveBeenCalled();
    // The policy set the loader returned MUST be in the install payload.
    // If this asserts on `[]`, the regression is back.
    expect(wasmInstall).toHaveBeenCalledWith(
      expect.objectContaining({
        policy_set: [SAMPLE_POLICY],
        manifests: { swap: SAMPLE_MANIFEST },
      }),
    );
    expect(setHash).toHaveBeenCalledWith("sha256:test");
  });

  it("skips the cold-start restore when no manifests are stored", async () => {
    const wasmInstall = vi.fn(async () => ({
      enrichedSchemaHash: "sha256:test",
      addedCustomFields: {},
    }));
    const loadPolicySet = vi.fn(async () => [SAMPLE_POLICY]);
    const devSeed = vi.fn(async () => {});

    await hydrateManifests({
      getAllManifests: async () => ({}),
      loadPolicySet,
      wasmInstall,
      setHash: async () => {},
      devSeed,
      fetchDefaults: async () => ({}),
    });

    // No manifests stored → cold-start restore is a no-op. Only the
    // dev-seed stage might fire (and it's mocked here).
    expect(wasmInstall).not.toHaveBeenCalled();
    expect(loadPolicySet).not.toHaveBeenCalled();
    expect(devSeed).toHaveBeenCalled();
  });

  it("dev-seed install ALSO preserves the enabled policy set", async () => {
    // Even on the dev-seed path (where new actions get added to the
    // manifest map), the WASM install must include the currently-enabled
    // policy set — otherwise dev-seed would clobber the cold-started
    // engine too.
    const wasmInstall = vi.fn(async () => ({
      enrichedSchemaHash: "sha256:test",
      addedCustomFields: {},
    }));
    const loadPolicySet = vi.fn(async () => [SAMPLE_POLICY]);

    // Mock devSeed to invoke the wasmInstall callback we supplied
    // (mimicking real `devSeed → atomicInstall → wasmInstall`). We cast
    // through `unknown` because the test only needs to exercise the
    // callback shape — the rest of `DevSeedDeps` isn't relevant here.
    const devSeed = vi.fn(
      async (deps: { wasmInstall: (m: Record<string, PolicyManifest>) => Promise<unknown> }) => {
        await deps.wasmInstall({ transfer: SAMPLE_MANIFEST });
      },
    ) as unknown as typeof import("./dev-seed").devSeed;

    await hydrateManifests({
      getAllManifests: async () => ({}),
      loadPolicySet,
      wasmInstall,
      setHash: async () => {},
      devSeed,
      fetchDefaults: async () => ({}),
    });

    expect(loadPolicySet).toHaveBeenCalled();
    expect(wasmInstall).toHaveBeenCalledWith(
      expect.objectContaining({
        policy_set: [SAMPLE_POLICY],
        manifests: { transfer: SAMPLE_MANIFEST },
      }),
    );
  });
});
