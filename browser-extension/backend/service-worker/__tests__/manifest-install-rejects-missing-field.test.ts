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
import { createHash } from "node:crypto";
import { RequestType, type Message } from "@lib/types";
import type { PolicyManifest } from "../manifests/store";

const OWNER = "0x1111111111111111111111111111111111111111";
const ROUTER = "0x2222222222222222222222222222222222222222";

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
        getURL: (p: string) => `chrome-extension://scopeball/${p}`,
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
vi.mock("../marketplace/storage", () => ({
  aggregatedPolicySet: mocks.aggregatedPolicySet,
  listInstalled: mocks.listInstalled,
}));
vi.mock("../dashboard/storage", () => ({
  aggregatedManagedPolicySet: mocks.aggregatedManagedPolicySet,
  listManaged: mocks.listManaged,
}));

// Pure SHA-256 hash mirroring `policy_engine::policy_rpc::manifest_set_hash`
// (sort manifests by `id`, JSON-serialize the canonical Vec, hex-encode).
function manifestSetHashTs(manifests: PolicyManifest[]): string {
  const sorted = [...manifests].sort((a, b) =>
    String(a.id).localeCompare(String(b.id)),
  );
  const json = JSON.stringify(sorted);
  return "sha256:" + createHash("sha256").update(json).digest("hex");
}

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

function swapManifestWithTotalInputUsd(): PolicyManifest {
  return {
    id: "user.swap.v1",
    schema_version: 1,
    requires: [
      {
        id: "swap-total-input-usd",
        when: { action: "swap" },
        method: "oracle.usd_value",
        params: { chain_id: "$.root.chain_id" },
        outputs: [
          {
            kind: "context",
            field: "totalInputUsd",
            type: "UsdValuation",
            from: "$.result",
            required: true,
          },
        ],
        optional: true,
      },
    ],
  } as unknown as PolicyManifest;
}

function txMessage(requestId: string): Message {
  return {
    requestId,
    data: {
      type: RequestType.TRANSACTION,
      chainId: 1,
      hostname: "app.example",
      transaction: { from: OWNER, to: ROUTER, value: "0x0", data: "0x" },
    },
  } as Message;
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

describe("Phase 7 codex carry-over H — evaluate passes installed Map manifests", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.localStore.clear();
    mocks.sessionStore.clear();
    mocks.runtimeMessageListeners.length = 0;
    mocks.windowRemovedListeners.length = 0;
    vi.resetModules();

    // Stale policies-loader Vec (e.g. legacy embedded manifests from
    // default-policies). Different from what the Map atomic-install
    // is about to push into WASM.
    mocks.getActivePolicyRpcManifests.mockReturnValue([
      {
        id: "default.stale.vec",
        schema_version: 1,
        requires: [],
      },
    ]);

    // WASM install accepts the Map and reports a fresh enriched-schema hash.
    mocks.installPolicies.mockImplementation(async () => {
      return {
        enrichedSchemaHash: "sha256:installed-from-map",
        addedCustomFields: { swap: [{ field: "totalInputUsd" }] },
      };
    });

    // The plan + evaluate stubs return success and record their
    // `manifests` payload via `mock.calls` so the test can compare.
    mocks.planPolicyRpc.mockImplementation(async (input: any) => ({
      request_id: input.request_id,
      root: {
        chain_id: 1,
        from: OWNER,
        to: ROUTER,
        value_wei: "0",
        block_timestamp: 0,
      },
      envelopes: [],
      calls: [],
      manifest_set_hash: manifestSetHashTs(input.manifests as PolicyManifest[]),
      schema_hash: "sha256:installed-schema",
      diagnostics: [],
    }));
    mocks.evaluatePolicyRpc.mockResolvedValue({ kind: "pass" });
  });

  it("uses the Map manifest store (not the stale loader Vec) when planning/evaluating", async () => {
    // 1. Install a Map-shape manifest via the dashboard handler path.
    const installed = swapManifestWithTotalInputUsd();
    const { handleManifestRequest } = await import("../manifests/handlers");
    const putResult = await handleManifestRequest({
      type: "manifest:put",
      action: "swap",
      manifest: installed,
    });
    expect(putResult.ok).toBe(true);

    // Sanity: WASM received a Map with exactly that manifest.
    const installArg = mocks.installPolicies.mock.calls[0]?.[0] as {
      manifests?: unknown;
    };
    expect(installArg.manifests).toEqual({ swap: installed });

    // 2. Run a decide-message lifecycle.
    const { decideMessage } = await import("../orchestrator");
    const decision = await decideMessage(txMessage("tx-h-regression"));
    expect(decision.verdict.kind).toBe("pass");

    // 3. Both plan and evaluate must receive the Map values (sorted),
    //    NOT the stale `getActivePolicyRpcManifests()` Vec. Equivalent
    //    way to say it: the manifest-set hash they compute is the same
    //    as the install just used.
    expect(mocks.planPolicyRpc).toHaveBeenCalledTimes(1);
    expect(mocks.evaluatePolicyRpc).toHaveBeenCalledTimes(1);

    const planManifests = (mocks.planPolicyRpc.mock.calls[0]?.[0] as {
      manifests: PolicyManifest[];
    }).manifests;
    const evalManifests = (mocks.evaluatePolicyRpc.mock.calls[0]?.[0] as {
      manifests: PolicyManifest[];
    }).manifests;

    const installedHash = manifestSetHashTs([installed]);
    expect(manifestSetHashTs(planManifests)).toBe(installedHash);
    expect(manifestSetHashTs(evalManifests)).toBe(installedHash);

    // The stale Vec must NOT have leaked through.
    const planIds = planManifests.map((m) => String(m.id));
    expect(planIds).not.toContain("default.stale.vec");
  });
});
