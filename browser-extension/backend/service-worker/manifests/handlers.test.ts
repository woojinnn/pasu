import { afterAll, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();
  return {
    localStore,
    wasmInstall: vi.fn(),
    previewCustomSchema: vi.fn(),
    previewInstalledSchema: vi.fn(),
    getAliasTable: vi.fn(),
    fetch: vi.fn(),
    // Phase 7 codex carry-over H follow-up: handlers must invoke
    // `loadCurrentEnabledPolicySet()` so the Map-install path includes
    // the user-enabled Cedar policies (not just the manifests).
    loadCurrentEnabledPolicySet: vi.fn(async () => [] as { id: string; text: string }[]),
    reinstallAllPolicies: vi.fn(async (_ids: string[]) => undefined),
    browser: {
      runtime: {
        getURL: (p: string) => `chrome-extension://test/${p}`,
      },
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
vi.mock("../policies-loader", () => ({
  loadCurrentEnabledPolicySet: mocks.loadCurrentEnabledPolicySet,
  reinstallAllPolicies: mocks.reinstallAllPolicies,
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
    mocks.localStore.set("dashboard:current-user-id", "test-user");
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

  // Phase 7 codex carry-over H follow-up: `manifest:put` must forward
  // the currently-enabled Cedar policy set to WASM. The earlier
  // implementation passed `policy_set: []`, which silently wiped every
  // installed policy on the next manifest edit — converting the loud
  // `manifest_hash_mismatch` failure into a quiet "every tx passes"
  // failure mode.
  it("manifest:put forwards loadCurrentEnabledPolicySet() into WASM (carry-over H follow-up)", async () => {
    mocks.wasmInstall.mockResolvedValue({
      enrichedSchemaHash: "sha256:fresh",
      addedCustomFields: { swap: [] },
    });
    const enabledPolicies = [
      { id: "dashboard::p1", text: "@id('p1') forbid (principal, action, resource);" },
      { id: "user::p2", text: "@id('p2') forbid (principal, action, resource);" },
    ];
    mocks.loadCurrentEnabledPolicySet.mockResolvedValue(enabledPolicies);

    const r = await handleManifestRequest({
      type: "manifest:put",
      action: "swap",
      manifest: emptyManifest("user::swap"),
    });

    expect(r.ok).toBe(true);
    expect(mocks.wasmInstall).toHaveBeenCalledTimes(1);
    const callArg = mocks.wasmInstall.mock.calls[0][0];
    expect(callArg.policy_set).toEqual(enabledPolicies);
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

  describe("manifest:get-method-catalog (Phase 8.5 hybrid discovery)", () => {
    const originalFetch = globalThis.fetch;
    beforeEach(() => {
      mocks.localStore.clear();
      vi.clearAllMocks();
    });

    afterAll(() => {
      globalThis.fetch = originalFetch;
    });

    it("merges bundled catalog with dynamic catalog from daemon", async () => {
      // Endpoint URL is set → handler will try to fetch from daemon too.
      await store.setEndpointUrl("http://localhost:8787");

      // Mock fetch: bundled catalog has `oracle.usd_value`; daemon
      // adds `risk.score` AND a newer version of `oracle.usd_value`
      // (with `origin: "bundled"` still — daemon's catalog wins).
      globalThis.fetch = vi.fn(async (url: string | URL | Request) => {
        const u = typeof url === "string" ? url : url.toString();
        if (u.includes("method-catalog.json")) {
          return new Response(
            JSON.stringify({
              methods: {
                "oracle.usd_value": {
                  name: "oracle.usd_value",
                  params: {},
                  returns: { kind: "record", type: "UsdValuation" },
                  origin: "bundled",
                  description: "OLD bundled desc",
                },
              },
            }),
            { status: 200 },
          );
        }
        if (u.endsWith("/v1/methods")) {
          return new Response(
            JSON.stringify({
              methods: ["oracle.usd_value", "risk.score"],
              catalog: {
                methods: {
                  "oracle.usd_value": {
                    name: "oracle.usd_value",
                    params: {},
                    returns: { kind: "record", type: "UsdValuation" },
                    origin: "bundled",
                    description: "NEW daemon desc",
                  },
                  "risk.score": {
                    name: "risk.score",
                    params: {},
                    returns: {
                      kind: "scalar",
                      type: "Long",
                      from: "$.result.value",
                    },
                    origin: "plugin",
                  },
                },
              },
            }),
            { status: 200 },
          );
        }
        return new Response("not found", { status: 404 });
      }) as unknown as typeof fetch;

      const r = await handleManifestRequest({
        type: "manifest:get-method-catalog",
      });
      expect(r.ok).toBe(true);
      if (!r.ok) return;
      const cat = r.data as { methods: Record<string, { description?: string; origin: string }> };
      // Both methods present.
      expect(Object.keys(cat.methods).sort()).toEqual([
        "oracle.usd_value",
        "risk.score",
      ]);
      // Daemon overrides bundled — description is the NEW one.
      expect(cat.methods["oracle.usd_value"].description).toBe("NEW daemon desc");
      // Plugin entry passes through with its origin tag intact.
      expect(cat.methods["risk.score"].origin).toBe("plugin");
    });

    it("returns bundled-only when no endpoint URL is configured", async () => {
      // No setEndpointUrl call → endpointUrl is null → daemon fetch skipped.
      globalThis.fetch = vi.fn(async (url: string | URL | Request) => {
        const u = typeof url === "string" ? url : url.toString();
        if (u.includes("method-catalog.json")) {
          return new Response(
            JSON.stringify({
              methods: {
                "oracle.usd_value": {
                  name: "oracle.usd_value",
                  params: {},
                  returns: { kind: "record", type: "UsdValuation" },
                  origin: "bundled",
                },
              },
            }),
            { status: 200 },
          );
        }
        return new Response("not found", { status: 404 });
      }) as unknown as typeof fetch;

      const r = await handleManifestRequest({
        type: "manifest:get-method-catalog",
      });
      expect(r.ok).toBe(true);
      if (!r.ok) return;
      const cat = r.data as { methods: Record<string, unknown> };
      expect(Object.keys(cat.methods)).toEqual(["oracle.usd_value"]);
    });

    it("returns empty catalog when both bundled and dynamic fail", async () => {
      await store.setEndpointUrl("http://localhost:8787");
      globalThis.fetch = vi.fn(async () => {
        throw new Error("network down");
      }) as unknown as typeof fetch;

      const r = await handleManifestRequest({
        type: "manifest:get-method-catalog",
      });
      expect(r.ok).toBe(true);
      if (!r.ok) return;
      const cat = r.data as { methods: Record<string, unknown> };
      expect(cat.methods).toEqual({});
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

  // Fix R: when the detector force-disabled a v0 policy that the user
  // had off pre-upgrade, ack must NOT leave it re-enabled (put-raw
  // unconditionally adds the id to `policy-selection:enabled-ids`).
  // The original-enabled snapshot drives the restore: `false` means the
  // ack handler must strip the id from enabled-ids again and reinstall.
  // `true` means leave put-raw's add intact (the policy stays enabled).
  it("migration:ack restores the user's prior disabled preference (original = false)", async () => {
    // Simulate the dashboard's complete rewrite flow up to ack:
    //   1. Detector saved {v0: false} and stripped v0 from enabled-ids.
    //   2. User clicked Rewrite → put-raw added v0 back to enabled-ids.
    //   3. Now `migration:ack` fires. Because original-enabled[v0] is
    //      false, ack pops v0 off the enabled set and reinstalls.
    mocks.wasmInstall.mockResolvedValue({
      enrichedSchemaHash: "sha256:installed",
      addedCustomFields: {},
    });
    await mocks.browser.storage.local.set({
      "migration:pending": ["dashboard::v0"],
      "migration:original-enabled": { "dashboard::v0": false },
      "policy-selection:enabled-ids:test-user": [
        "dashboard::v0",
        "default::dex/keep",
      ],
    });

    const r = await handleManifestRequest({
      type: "migration:ack",
      id: "dashboard::v0",
    });
    expect(r.ok).toBe(true);
    // pending is empty + snapshot cleared. The helper removes empty
    // keys entirely to keep storage tidy, so we assert on absence.
    expect(mocks.localStore.has("migration:pending")).toBe(false);
    expect(mocks.localStore.has("migration:original-enabled")).toBe(false);
    // enabled-ids no longer contains v0 — and applied-ids tracks it
    // because we route the strip through `applyEnabledIds`, which
    // writes both keys in lockstep.
    expect(mocks.localStore.get("policy-selection:enabled-ids:test-user")).toEqual([
      "default::dex/keep",
    ]);
    expect(mocks.localStore.get("policy-selection:applied-ids:test-user")).toEqual([
      "default::dex/keep",
    ]);
    // Reinstall fired with the new (v0-less) enabled set so WASM mirrors
    // the persisted state. This is the critical bit Fix R closes —
    // without it the rejection-on-every-request mode persists until SW
    // reboot.
    expect(mocks.reinstallAllPolicies).toHaveBeenCalledTimes(1);
    expect(mocks.reinstallAllPolicies.mock.calls[0][0]).toEqual([
      "default::dex/keep",
    ]);
  });

  it("migration:ack leaves the id enabled when original was true", async () => {
    // The detector saved {v0: true} (user had it on). put-raw re-added
    // it. Ack should NOT touch enabled-ids — the user's preference is
    // already restored.
    await mocks.browser.storage.local.set({
      "migration:pending": ["dashboard::v0"],
      "migration:original-enabled": { "dashboard::v0": true },
      "policy-selection:enabled-ids:test-user": [
        "dashboard::v0",
        "default::dex/keep",
      ],
    });

    const r = await handleManifestRequest({
      type: "migration:ack",
      id: "dashboard::v0",
    });
    expect(r.ok).toBe(true);
    expect(mocks.localStore.has("migration:pending")).toBe(false);
    expect(mocks.localStore.has("migration:original-enabled")).toBe(false);
    expect(mocks.localStore.get("policy-selection:enabled-ids:test-user")).toEqual([
      "dashboard::v0",
      "default::dex/keep",
    ]);
    // No reinstall — the persisted set already matches the user's wish.
    expect(mocks.reinstallAllPolicies).not.toHaveBeenCalled();
  });

  it("migration:ack tolerates a missing original-enabled snapshot", async () => {
    // Defensive: if the detector never ran for this id (e.g. ack called
    // out of order), ack still clears pending without crashing.
    await mocks.browser.storage.local.set({
      "migration:pending": ["dashboard::v0"],
      "policy-selection:enabled-ids:test-user": ["dashboard::v0"],
    });
    const r = await handleManifestRequest({
      type: "migration:ack",
      id: "dashboard::v0",
    });
    expect(r.ok).toBe(true);
    expect(mocks.localStore.has("migration:pending")).toBe(false);
    expect(mocks.localStore.get("policy-selection:enabled-ids:test-user")).toEqual([
      "dashboard::v0",
    ]);
  });
});
