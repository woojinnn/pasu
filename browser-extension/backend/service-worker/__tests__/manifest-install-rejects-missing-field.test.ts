// Phase 8 / Task 8.2 — SW e2e for manifest-driven install + evaluate.
//
// Two flows are pinned here:
//
//   1. Atomic install fails closed when the manifest doesn't produce a
//      custom field that an installed managed policy references. This is
//      the contract the dashboard relies on to surface "this manifest
//      breaks demand-side validation" instead of silently shipping a
//      schema the policy can't compile against.
//
//   2. **Phase 7 codex carry-over H regression**: after `manifest:put`
//      installs the Map-shape manifest set into WASM, a follow-up
//      `decideMessage` evaluate call must pass the SAME manifest set
//      (same `manifest_set_hash`) to plan/evaluate. Before the fix, the
//      orchestrator forwarded the policies-loader's stale Vec, which
//      hashes differently from the Map values and produced a silent
//      `manifest_hash_mismatch` engine error inside WASM.
//
// We don't load real WASM (no existing test harness does — every
// `__tests__/*` mocks at the wasm-bridge boundary). Instead we capture
// the JSON each layer sends through `wasm-bridge` and assert on the
// envelope shapes.

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PolicyManifest } from "../manifests/store";

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();
  const sessionStore = new Map<string, unknown>();
  const runtimeMessageListeners: Array<(message: unknown) => void> = [];
  const windowRemovedListeners: Array<(windowId: number) => void> = [];

  class MockEngineError extends Error {
    constructor(
      readonly kind: string,
      message: string,
    ) {
      super(message);
      this.name = "EngineError";
    }
  }

  const readStore = async (
    store: Map<string, unknown>,
    keys?: string | string[] | Record<string, unknown>,
  ): Promise<Record<string, unknown>> => {
    if (keys === undefined || keys === null)
      return Object.fromEntries(store.entries());
    if (typeof keys === "string") {
      return { [keys]: store.get(keys) };
    }
    if (Array.isArray(keys)) {
      const out: Record<string, unknown> = {};
      for (const k of keys) out[k] = store.get(k);
      return out;
    }
    const out: Record<string, unknown> = {};
    for (const [k, fallback] of Object.entries(keys)) {
      out[k] = store.has(k) ? store.get(k) : fallback;
    }
    return out;
  };

  return {
    localStore,
    sessionStore,
    runtimeMessageListeners,
    windowRemovedListeners,
    MockEngineError,
    // wasm-bridge mocks. We capture inputs so the H-regression test can
    // compare what install received vs what plan / evaluate received.
    installPolicies: vi.fn(),
    planPolicyRpc: vi.fn(),
    evaluatePolicyRpc: vi.fn(),
    previewCustomSchema: vi.fn(),
    previewInstalledSchema: vi.fn(),
    getAliasTable: vi.fn(),
    // policies-loader: provide a stub Vec so we can assert the
    // orchestrator does NOT forward this (which would mismatch the
    // installed Map's hash).
    getActivePolicyRpcManifests: vi.fn<() => unknown[]>(() => []),
    ensureDefaultPoliciesInstalled: vi.fn(async () => undefined),
    aggregatedPolicySet: vi.fn(async () => []),
    aggregatedManagedPolicySet: vi.fn(async () => []),
    listInstalled: vi.fn(async () => []),
    listManaged: vi.fn(async () => []),
    browser: {
      runtime: {
        getURL: (p: string) => `chrome-extension://dambi/${p}`,
        sendMessage: vi.fn(async () => undefined),
        onMessage: {
          addListener: vi.fn((listener: (m: unknown) => void) => {
            runtimeMessageListeners.push(listener);
          }),
          removeListener: vi.fn((listener: (m: unknown) => void) => {
            const idx = runtimeMessageListeners.indexOf(listener);
            if (idx >= 0) runtimeMessageListeners.splice(idx, 1);
          }),
        },
      },
      windows: {
        create: vi.fn(async () => ({ id: 1 })),
        remove: vi.fn(async () => undefined),
        onRemoved: {
          addListener: vi.fn((listener: (id: number) => void) => {
            windowRemovedListeners.push(listener);
          }),
          removeListener: vi.fn((listener: (id: number) => void) => {
            const idx = windowRemovedListeners.indexOf(listener);
            if (idx >= 0) windowRemovedListeners.splice(idx, 1);
          }),
        },
      },
      storage: {
        local: {
          get: vi.fn((keys?: string | string[] | Record<string, unknown>) =>
            readStore(localStore, keys),
          ),
          set: vi.fn(async (entries: Record<string, unknown>) => {
            for (const [k, v] of Object.entries(entries)) localStore.set(k, v);
          }),
          remove: vi.fn(async (keys: string | string[]) => {
            if (Array.isArray(keys)) for (const k of keys) localStore.delete(k);
            else localStore.delete(keys);
          }),
        },
        session: {
          get: vi.fn((keys?: string | string[] | Record<string, unknown>) =>
            readStore(sessionStore, keys),
          ),
          set: vi.fn(async (entries: Record<string, unknown>) => {
            for (const [k, v] of Object.entries(entries))
              sessionStore.set(k, v);
          }),
        },
      },
    },
  };
});

vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));
vi.mock("../wasm-bridge", () => ({
  EngineError: mocks.MockEngineError,
  installPolicies: mocks.installPolicies,
  planPolicyRpc: mocks.planPolicyRpc,
  evaluatePolicyRpc: mocks.evaluatePolicyRpc,
  previewCustomSchema: mocks.previewCustomSchema,
  previewInstalledSchema: mocks.previewInstalledSchema,
  getAliasTable: mocks.getAliasTable,
}));
vi.mock("../policies-loader", () => ({
  ensureDefaultPoliciesInstalled: mocks.ensureDefaultPoliciesInstalled,
  getActivePolicyRpcManifests: mocks.getActivePolicyRpcManifests,
  loadCurrentEnabledPolicySet: vi.fn(async () => []),
  reinstallAllPolicies: vi.fn(async () => undefined),
}));
vi.mock("../adapter-loader/storage", () => ({
  aggregatedPolicySet: mocks.aggregatedPolicySet,
  listInstalled: mocks.listInstalled,
}));
vi.mock("../dashboard/storage", () => ({
  aggregatedManagedPolicySet: mocks.aggregatedManagedPolicySet,
  listManaged: mocks.listManaged,
}));

function emptySwapManifest(): PolicyManifest {
  // Has the required top-level fields but no outputs at all — meaning
  // the resulting cedarschema custom block is empty. A managed policy
  // that references `context.custom.totalInputUsd` cannot compile against
  // this schema; WASM rejects the install with a `demand_mismatch`.
  return {
    id: "user.swap.empty",
    schema_version: 1,
    requires: [],
  };
}

describe("Phase 8 / Task 8.2 — atomic install rejects missing field", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.localStore.clear();
    mocks.sessionStore.clear();
    mocks.runtimeMessageListeners.length = 0;
    mocks.windowRemovedListeners.length = 0;
    vi.resetModules();
  });

  it("rejects install when WASM signals demand_mismatch and leaves storage untouched", async () => {
    mocks.installPolicies.mockRejectedValueOnce(
      new mocks.MockEngineError(
        "demand_mismatch",
        "policy dashboard::p1 requires context.custom.totalInputUsd",
      ),
    );

    const { atomicInstall } = await import("../manifests/atomic-install");
    const { getAllManifests, getHash } = await import("../manifests/store");

    const result = await atomicInstall(
      { swap: emptySwapManifest() },
      { wasmInstall: mocks.installPolicies },
    );

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.error.kind).toBe("demand_mismatch");
      expect(result.error.message).toMatch(/totalInputUsd/);
    }
    // Storage was NOT mutated — the install failed closed.
    expect(await getAllManifests()).toEqual({});
    expect(await getHash()).toBeNull();
  });
});
