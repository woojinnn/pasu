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
    },
  };
});

vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));

import { devSeed } from "./dev-seed";
import * as store from "./store";

function manifest(id: string): store.PolicyManifest {
  return { id, schema_version: 1, requires: [], context_extensions: {} };
}

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

  it("seeds missing actions in dev build", async () => {
    const fetchDefaults = vi.fn(async () => ({ swap: manifest("default::swap") }));
    const wasmInstall = vi.fn(async () => ({
      enrichedSchemaHash: "sha256:zz",
      addedCustomFields: { swap: [] },
    }));

    await devSeed({ fetchDefaults, wasmInstall });

    expect(fetchDefaults).toHaveBeenCalledTimes(1);
    expect(wasmInstall).toHaveBeenCalledTimes(1);
    expect((await store.getManifest("swap"))!.id).toBe("default::swap");
    expect(await store.getHash()).toBe("sha256:zz");
  });

  it("does not seed in prod build", async () => {
    process.env.NODE_ENV = "production";
    const fetchDefaults = vi.fn();
    const wasmInstall = vi.fn();

    await devSeed({ fetchDefaults, wasmInstall });

    expect(fetchDefaults).not.toHaveBeenCalled();
    expect(wasmInstall).not.toHaveBeenCalled();
    expect(await store.getAllManifests()).toEqual({});
  });

  it("does not overwrite manifests already in storage", async () => {
    await store.putManifestRaw("swap", manifest("user::swap-existing"));

    const fetchDefaults = vi.fn(async () => ({
      swap: manifest("default::swap"),
      borrow: manifest("default::borrow"),
    }));
    const wasmInstall = vi.fn(async () => ({
      enrichedSchemaHash: "sha256:zz",
      addedCustomFields: {},
    }));

    await devSeed({ fetchDefaults, wasmInstall });

    const all = await store.getAllManifests();
    // Existing user `swap` preserved, missing `borrow` filled in.
    expect(all.swap.id).toBe("user::swap-existing");
    expect(all.borrow.id).toBe("default::borrow");
  });

  it("is a no-op when every default action already has a manifest", async () => {
    await store.putManifestRaw("swap", manifest("user::swap"));

    const fetchDefaults = vi.fn(async () => ({ swap: manifest("default::swap") }));
    const wasmInstall = vi.fn();

    await devSeed({ fetchDefaults, wasmInstall });

    expect(wasmInstall).not.toHaveBeenCalled();
    expect((await store.getManifest("swap"))!.id).toBe("user::swap");
  });

  it("sets a default endpoint URL when none is configured", async () => {
    const fetchDefaults = vi.fn(async () => ({ swap: manifest("default::swap") }));
    const wasmInstall = vi.fn(async () => ({
      enrichedSchemaHash: "sha256:zz",
      addedCustomFields: {},
    }));

    await devSeed({ fetchDefaults, wasmInstall });

    expect(await store.getEndpointUrl()).toBe("http://localhost:8787");
  });

  it("does not overwrite an existing endpoint URL", async () => {
    await store.setEndpointUrl("http://custom.example:9999");

    const fetchDefaults = vi.fn(async () => ({ swap: manifest("default::swap") }));
    const wasmInstall = vi.fn(async () => ({
      enrichedSchemaHash: "sha256:zz",
      addedCustomFields: {},
    }));

    await devSeed({ fetchDefaults, wasmInstall });

    expect(await store.getEndpointUrl()).toBe("http://custom.example:9999");
  });
});
