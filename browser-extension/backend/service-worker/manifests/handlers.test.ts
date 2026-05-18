import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();
  return {
    localStore,
    wasmInstall: vi.fn(),
    previewCustomSchema: vi.fn(),
    previewInstalledSchema: vi.fn(),
    getAliasTable: vi.fn(),
    fetch: vi.fn(),
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
vi.mock("../wasm-bridge", () => ({
  installPolicies: mocks.wasmInstall,
  previewCustomSchema: mocks.previewCustomSchema,
  previewInstalledSchema: mocks.previewInstalledSchema,
  getAliasTable: mocks.getAliasTable,
}));

import { handleManifestRequest, isManifestRequest } from "./handlers";
import * as store from "./store";

function emptyManifest(id: string): store.PolicyManifest {
  return { id, schema_version: 1, requires: [], context_extensions: {} };
}

describe("isManifestRequest", () => {
  it("returns true for any manifest: or migration: prefix", () => {
    expect(isManifestRequest({ type: "manifest:put" })).toBe(true);
    expect(isManifestRequest({ type: "manifest:get" })).toBe(true);
    expect(isManifestRequest({ type: "migration:list" })).toBe(true);
  });
  it("returns false for everything else", () => {
    expect(isManifestRequest({ type: "dashboard:put-raw" })).toBe(false);
    expect(isManifestRequest({ type: "unrelated" })).toBe(false);
    expect(isManifestRequest(null)).toBe(false);
  });
});

describe("handleManifestRequest", () => {
  beforeEach(() => {
    mocks.localStore.clear();
    vi.clearAllMocks();
    vi.stubGlobal("fetch", mocks.fetch);
  });

  it("manifest:get returns null for an absent action", async () => {
    const r = await handleManifestRequest({
      type: "manifest:get",
      action: "swap",
    });
    expect(r).toEqual({ ok: true, data: { manifest: null } });
  });

  it("manifest:put installs atomically through WASM and writes storage", async () => {
    mocks.wasmInstall.mockResolvedValue({
      enrichedSchemaHash: "sha256:new",
      addedCustomFields: { swap: [] },
    });

    const manifest = emptyManifest("user::swap");
    const r = await handleManifestRequest({
      type: "manifest:put",
      action: "swap",
      manifest,
    });

    expect(r.ok).toBe(true);
    expect(mocks.wasmInstall).toHaveBeenCalledTimes(1);
    // Map-shape contract — exactly one entry keyed on the action.
    const callArg = mocks.wasmInstall.mock.calls[0][0];
    expect(callArg.manifests).toEqual({ swap: manifest });
    expect((await store.getManifest("swap"))!.id).toBe("user::swap");
    expect(await store.getHash()).toBe("sha256:new");
  });

  it("manifest:put rolls back when WASM rejects", async () => {
    await store.putManifestRaw("swap", emptyManifest("user::swap-old"));
    mocks.wasmInstall.mockRejectedValue(
      Object.assign(new Error("schema validation"), { kind: "install_failed" }),
    );

    const r = await handleManifestRequest({
      type: "manifest:put",
      action: "swap",
      manifest: emptyManifest("user::swap-new"),
    });

    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error.kind).toBe("install_failed");
    expect((await store.getManifest("swap"))!.id).toBe("user::swap-old");
  });

  it("manifest:get returns the stored manifest", async () => {
    await store.putManifestRaw("swap", emptyManifest("user::swap"));
    const r = await handleManifestRequest({
      type: "manifest:get",
      action: "swap",
    });
    expect(r.ok).toBe(true);
    if (r.ok)
      expect((r.data as { manifest: { id: string } }).manifest.id).toBe(
        "user::swap",
      );
  });

  it("manifest:preview delegates to the WASM bridge", async () => {
    mocks.previewCustomSchema.mockResolvedValue({
      customTypes: [],
      enrichedSchemaText: "type SwapCustomContext = {};",
      diff: { added: [], removed: [], changed: [] },
      schemaHash: "sha256:zz",
    });

    const r = await handleManifestRequest({
      type: "manifest:preview",
      action: "swap",
      manifest: emptyManifest("user::swap"),
    });

    expect(r.ok).toBe(true);
    if (r.ok) {
      expect((r.data as { schemaHash: string }).schemaHash).toBe("sha256:zz");
    }
    expect(mocks.previewCustomSchema).toHaveBeenCalledWith({
      action: "swap",
      manifest: emptyManifest("user::swap"),
    });
  });

  it("manifest:get-enriched-schema delegates to the WASM bridge", async () => {
    mocks.previewInstalledSchema.mockResolvedValue({
      schema_text: "",
      schema_hash: "sha256:installed",
      added_fields: [],
      customContexts: {},
      schemaHash: "sha256:installed",
    });

    const r = await handleManifestRequest({
      type: "manifest:get-enriched-schema",
    });
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect((r.data as { schemaHash: string }).schemaHash).toBe(
        "sha256:installed",
      );
    }
  });

  it("manifest:alias-table delegates to the WASM bridge", async () => {
    mocks.getAliasTable.mockResolvedValue({
      entries: [{ name: "String", kind: "scalar", cedarSpelling: "String" }],
    });
    const r = await handleManifestRequest({ type: "manifest:alias-table" });
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(
        (r.data as { entries: unknown[] }).entries[0],
      ).toEqual({
        name: "String",
        kind: "scalar",
        cedarSpelling: "String",
      });
    }
  });

  it("manifest:ping returns reachable=true on HTTP 200", async () => {
    await store.setEndpointUrl("http://localhost:8787");
    mocks.fetch.mockResolvedValue({ ok: true, status: 200 } as Response);
    const r = await handleManifestRequest({ type: "manifest:ping" });
    expect(r.ok).toBe(true);
    if (r.ok) {
      const d = r.data as { reachable: boolean; url: string };
      expect(d.reachable).toBe(true);
      expect(d.url).toBe("http://localhost:8787");
    }
  });

  it("manifest:ping returns reachable=false when no endpoint is configured", async () => {
    const r = await handleManifestRequest({ type: "manifest:ping" });
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect((r.data as { reachable: boolean }).reachable).toBe(false);
    }
  });

  it("migration:list returns the persisted pending ids", async () => {
    await mocks.browser.storage.local.set({
      "migration:pending": ["a", "b"],
    });
    const r = await handleManifestRequest({ type: "migration:list" });
    expect(r.ok).toBe(true);
    if (r.ok) expect((r.data as { ids: string[] }).ids).toEqual(["a", "b"]);
  });
});
