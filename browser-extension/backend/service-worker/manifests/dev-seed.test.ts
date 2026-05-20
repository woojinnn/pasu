import { afterAll, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();
  return {
    localStore,
    browser: {
      storage: {
        local: {
          get: vi.fn(async (key: string | string[]) => {
            const keys = Array.isArray(key) ? key : [key];
            const out: Record<string, unknown> = {};
            for (const k of keys) {
              if (localStore.has(k)) out[k] = localStore.get(k);
            }
            return out;
          }),
          set: vi.fn(async (entries: Record<string, unknown>) => {
            for (const [k, v] of Object.entries(entries)) localStore.set(k, v);
          }),
          remove: vi.fn(async (keys: string | string[]) => {
            const arr = Array.isArray(keys) ? keys : [keys];
            for (const k of arr) localStore.delete(k);
          }),
        },
      },
      runtime: {
        getURL: (p: string) => `chrome-extension://test/${p}`,
      },
    },
  };
});

vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));

import { devSeed } from "./dev-seed";
import * as store from "./store";

function manifest(id: string): store.PolicyManifest {
  return { id, schema_version: 1, requires: [], context_extensions: {} };
}

// Phase 8 contract: `devSeed` only seeds the dev policy-rpc endpoint
// when nothing is configured yet. The previous bundled-manifest auto-
// install was removed because it tied every user's storage to the
// shipped defaults in a way that silently broke on edits. Bundled
// manifests now come in via the manifest editor's explicit "Install
// starter pack" button (see Step 6).
describe("devSeed", () => {
  const originalNodeEnv = process.env.NODE_ENV;

  beforeEach(async () => {
    mocks.localStore.clear();
    vi.clearAllMocks();
    process.env.NODE_ENV = "development";
  });

  afterAll(() => {
    process.env.NODE_ENV = originalNodeEnv;
  });

  it("never installs a bundled manifest into storage", async () => {
    // The `fetchDefaults` and `wasmInstall` arguments are accepted for
    // backwards-compatible call sites but MUST NOT be invoked — this is
    // the regression bait for accidentally re-enabling auto-seed.
    const fetchDefaults = vi.fn(async () => ({
      swap: manifest("default::swap"),
    }));
    const wasmInstall = vi.fn();
    await devSeed({ fetchDefaults, wasmInstall });
    expect(fetchDefaults).not.toHaveBeenCalled();
    expect(wasmInstall).not.toHaveBeenCalled();
    expect(await store.getAllManifests()).toEqual({});
  });

  it("preserves any manifest already in storage", async () => {
    // Pre-existing user content stays untouched.
    await store.putManifestRaw("swap", manifest("user::swap-existing"));
    await devSeed({});
    const all = await store.getAllManifests();
    expect(all.swap.id).toBe("user::swap-existing");
  });

  it("sets the dev endpoint when none is configured", async () => {
    await devSeed({});
    expect(await store.getEndpointUrl()).toBe("http://localhost:8787");
  });

  it("does not overwrite an existing endpoint URL", async () => {
    await store.setEndpointUrl("http://custom.example:9999");
    await devSeed({});
    expect(await store.getEndpointUrl()).toBe("http://custom.example:9999");
  });

  it("is a no-op in prod builds", async () => {
    process.env.NODE_ENV = "production";
    await devSeed({});
    expect(await store.getEndpointUrl()).toBeNull();
  });
});
