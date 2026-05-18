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

  // Phase 7 codex carry-over M: the SW must enforce the http(s) scheme
  // server-side. Dashboard validation is a UX nicety; the SDK is
  // reachable from any content-script context and we can't trust the
  // caller to honour the contract.
  it("manifest:set-endpoint-url accepts http and https URLs", async () => {
    const r1 = await handleManifestRequest({
      type: "manifest:set-endpoint-url",
      url: "http://localhost:8787",
    });
    expect(r1.ok).toBe(true);
    if (r1.ok) {
      expect((r1.data as { url: string | null }).url).toBe(
        "http://localhost:8787",
      );
    }
    const r2 = await handleManifestRequest({
      type: "manifest:set-endpoint-url",
      url: "https://policy-rpc.example.com",
    });
    expect(r2.ok).toBe(true);
    if (r2.ok) {
      expect((r2.data as { url: string | null }).url).toBe(
        "https://policy-rpc.example.com",
      );
    }
  });

  it("manifest:set-endpoint-url rejects URLs with non-http(s) schemes", async () => {
    for (const bad of [
      "javascript:alert(1)",
      "file:///etc/passwd",
      "ftp://example.com",
      "//example.com",
      "example.com",
      "  no-scheme",
    ]) {
      const r = await handleManifestRequest({
        type: "manifest:set-endpoint-url",
        url: bad,
      });
      expect(r.ok).toBe(false);
      if (!r.ok) {
        expect(r.error.kind).toBe("invalid_endpoint_url");
      }
    }
    // Storage was never written.
    expect(await store.getEndpointUrl()).toBeNull();
  });

  it("manifest:set-endpoint-url clears the value when null or empty string is passed", async () => {
    await store.setEndpointUrl("http://localhost:8787");
    const r1 = await handleManifestRequest({
      type: "manifest:set-endpoint-url",
      url: null,
    });
    expect(r1.ok).toBe(true);
    expect(await store.getEndpointUrl()).toBeNull();

    await store.setEndpointUrl("http://localhost:8787");
    const r2 = await handleManifestRequest({
      type: "manifest:set-endpoint-url",
      url: "",
    });
    expect(r2.ok).toBe(true);
    expect(await store.getEndpointUrl()).toBeNull();
  });

  it("migration:list returns the persisted pending ids", async () => {
    await mocks.browser.storage.local.set({
      "migration:pending": ["a", "b"],
    });
    const r = await handleManifestRequest({ type: "migration:list" });
    expect(r.ok).toBe(true);
    if (r.ok) expect((r.data as { ids: string[] }).ids).toEqual(["a", "b"]);
  });

  it("migration:rewrite leaves the id on the pending queue when it produced a real rewrite", async () => {
    // Codex review carry-over: don't pop pending until the dashboard
    // confirms with `migration:ack` after the follow-up put-raw lands.
    // Otherwise a failed put-raw (network drop, WASM reject, tab close)
    // leaves storage with v0 text and no banner to surface it.
    await mocks.browser.storage.local.set({
      "migration:pending": ["dashboard::a"],
    });
    const r = await handleManifestRequest({
      type: "migration:rewrite",
      id: "dashboard::a",
      text: `forbid (principal, action == Action::"swap", resource)
        when { context.totalInputUsd.value > 100 };`,
      knownFields: ["totalInputUsd"],
    });
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect((r.data as { applied: boolean }).applied).toBe(true);
    }
    // Pending unchanged — the dashboard still has to ack.
    expect(mocks.localStore.get("migration:pending")).toEqual(["dashboard::a"]);
  });

  it("migration:rewrite auto-pops when there is nothing to rewrite", async () => {
    // When the text is already on the v1 layout, the rewrite returns
    // the input verbatim and there's no follow-up put-raw to wait for
    // — pop the id immediately so the banner stops showing it.
    await mocks.browser.storage.local.set({
      "migration:pending": ["dashboard::clean"],
    });
    const r = await handleManifestRequest({
      type: "migration:rewrite",
      id: "dashboard::clean",
      text: `forbid (principal, action == Action::"swap", resource);`,
      knownFields: ["totalInputUsd"],
    });
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect((r.data as { applied: boolean }).applied).toBe(false);
    }
    // `setPending([])` removes the key entirely to keep storage tidy.
    expect(mocks.localStore.has("migration:pending")).toBe(false);
  });

  it("migration:ack pops the id off the pending queue", async () => {
    await mocks.browser.storage.local.set({
      "migration:pending": ["dashboard::a", "dashboard::b"],
    });
    const r = await handleManifestRequest({
      type: "migration:ack",
      id: "dashboard::a",
    });
    expect(r.ok).toBe(true);
    expect(mocks.localStore.get("migration:pending")).toEqual(["dashboard::b"]);
    if (r.ok) {
      expect((r.data as { remaining: string[] }).remaining).toEqual([
        "dashboard::b",
      ]);
    }
  });

  it("migration:ack is a no-op when the id isn't on the queue", async () => {
    await mocks.browser.storage.local.set({
      "migration:pending": ["dashboard::a"],
    });
    const r = await handleManifestRequest({
      type: "migration:ack",
      id: "dashboard::missing",
    });
    expect(r.ok).toBe(true);
    expect(mocks.localStore.get("migration:pending")).toEqual(["dashboard::a"]);
  });
});
