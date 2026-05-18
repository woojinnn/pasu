import { beforeEach, describe, expect, it, vi } from "vitest";

// Reuse the in-memory storage mock from store.test.ts conventions so the
// `store` module reads / writes the same backing map.
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

import { atomicInstall, type WasmInstallFn } from "./atomic-install";
import * as store from "./store";

function manifest(id: string): store.PolicyManifest {
  return { id, schema_version: 1, requires: [], context_extensions: {} };
}

describe("atomicInstall", () => {
  beforeEach(async () => {
    mocks.localStore.clear();
    vi.clearAllMocks();
  });

  it("commits storage + hash only when WASM install succeeds", async () => {
    const wasmInstall: WasmInstallFn = vi.fn(async () => ({
      enrichedSchemaHash: "sha256:new",
      addedCustomFields: { swap: [] },
    }));
    const next = { swap: manifest("user::swap") };

    const result = await atomicInstall(next, { wasmInstall });

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.data.enrichedSchemaHash).toBe("sha256:new");
    }
    expect(await store.getAllManifests()).toEqual(next);
    expect(await store.getHash()).toBe("sha256:new");
    expect(wasmInstall).toHaveBeenCalledTimes(1);
    // The WASM install is called with the *map* shape (Phase 6 contract).
    const arg = (wasmInstall as ReturnType<typeof vi.fn>).mock.calls[0][0];
    expect(arg).toEqual(next);
  });

  it("rolls back: leaves storage untouched when WASM install throws", async () => {
    await store.putManifestRaw("swap", manifest("user::swap-old"));
    await store.setHash("sha256:old");

    const wasmInstall: WasmInstallFn = vi.fn(async () => {
      throw Object.assign(new Error("schema"), { kind: "schema_failed" });
    });

    const next = { swap: manifest("user::swap-new") };
    const result = await atomicInstall(next, { wasmInstall });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.kind).toBe("schema_failed");
    }
    // Storage untouched.
    const after = await store.getAllManifests();
    expect(after.swap.id).toBe("user::swap-old");
    expect(await store.getHash()).toBe("sha256:old");
  });

  it("rolls back when WASM install returns null (legacy envelope) without populating the hash", async () => {
    // A null return means the caller hit the legacy Vec path. Phase 6
    // must reject that — the contract requires an enriched schema hash.
    const wasmInstall: WasmInstallFn = vi.fn(async () => null);
    const next = { swap: manifest("user::swap") };

    const result = await atomicInstall(next, { wasmInstall });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.kind).toBe("install_legacy_envelope");
    }
    expect(await store.getAllManifests()).toEqual({});
    expect(await store.getHash()).toBeNull();
  });

  it("replaces the manifest map wholesale rather than merging", async () => {
    await store.putManifestRaw("swap", manifest("user::swap-old"));
    await store.putManifestRaw("borrow", manifest("user::borrow-old"));

    const wasmInstall: WasmInstallFn = vi.fn(async () => ({
      enrichedSchemaHash: "sha256:new",
      addedCustomFields: {},
    }));
    // Caller wants only `swap` active now — `borrow` must drop.
    const next = { swap: manifest("user::swap-new") };

    const result = await atomicInstall(next, { wasmInstall });

    expect(result.ok).toBe(true);
    const after = await store.getAllManifests();
    expect(Object.keys(after)).toEqual(["swap"]);
    expect(after.swap.id).toBe("user::swap-new");
  });

  it("commits the manifest map and the hash in a single storage.set call", async () => {
    // The map + hash must be persisted atomically — otherwise a thrown
    // second write would leave storage half-installed (manifests with
    // an old/missing hash, or vice versa). We assert this by counting
    // the number of `.set` calls issued AFTER the WASM install returns.
    const wasmInstall: WasmInstallFn = vi.fn(async () => ({
      enrichedSchemaHash: "sha256:atomic",
      addedCustomFields: {},
    }));
    const setSpy = mocks.browser.storage.local.set as ReturnType<typeof vi.fn>;
    setSpy.mockClear();

    const next = { swap: manifest("user::swap") };
    const result = await atomicInstall(next, { wasmInstall });

    expect(result.ok).toBe(true);
    // Exactly one .set call should fire after wasm install — the
    // combined commit that writes KEY_MANIFESTS and KEY_HASH together.
    expect(setSpy).toHaveBeenCalledTimes(1);
    const arg = setSpy.mock.calls[0][0] as Record<string, unknown>;
    expect(Object.keys(arg).sort()).toEqual(
      ["rpc:enrichedSchemaHash", "rpc:manifests"].sort(),
    );
    expect(arg["rpc:manifests"]).toEqual(next);
    expect(arg["rpc:enrichedSchemaHash"]).toBe("sha256:atomic");
  });

  it("leaves both keys unchanged when the storage commit throws", async () => {
    // Pre-existing state we expect to survive a failed install.
    await store.putManifestRaw("swap", manifest("user::swap-old"));
    await store.setHash("sha256:old");

    const wasmInstall: WasmInstallFn = vi.fn(async () => ({
      enrichedSchemaHash: "sha256:new",
      addedCustomFields: {},
    }));

    const setSpy = mocks.browser.storage.local.set as ReturnType<typeof vi.fn>;
    // Force only the *commit* set() (the one with both KEY_MANIFESTS
    // and KEY_HASH) to fail; earlier setup writes already went through
    // so by clearing + re-installing the failing impl we cover the
    // exact "post-wasm commit" window.
    setSpy.mockImplementationOnce(async () => {
      throw Object.assign(new Error("disk full"), { kind: "storage_failed" });
    });

    const next = { swap: manifest("user::swap-new") };
    const result = await atomicInstall(next, { wasmInstall });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.kind).toBe("storage_failed");
    }
    // Neither key was updated — both still hold their pre-install
    // values because the combined .set was atomic.
    const after = await store.getAllManifests();
    expect(after.swap.id).toBe("user::swap-old");
    expect(await store.getHash()).toBe("sha256:old");
  });
});
