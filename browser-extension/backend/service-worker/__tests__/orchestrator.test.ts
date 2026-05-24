import { beforeEach, describe, expect, it, vi } from "vitest";
import { RequestType, type Message } from "@lib/types";

const OWNER = "0x1111111111111111111111111111111111111111";
const ROUTER = "0x2222222222222222222222222222222222222222";

const mocks = vi.hoisted(() => {
  class MockEngineError extends Error {
    constructor(
      readonly kind: string,
      message: string,
    ) {
      super(message);
    }
  }

  const sessionStore = new Map<string, unknown>();
  const localStore = new Map<string, unknown>();
  const runtimeMessageListeners: Array<(message: unknown) => void> = [];
  const windowRemovedListeners: Array<(windowId: number) => void> = [];

  const readStore = async (
    store: Map<string, unknown>,
    keys?: string | string[] | Record<string, unknown>,
  ): Promise<Record<string, unknown>> => {
    if (keys === undefined || keys === null)
      return Object.fromEntries(store.entries());
    const out: Record<string, unknown> = {};
    if (typeof keys === "string") {
      out[keys] = store.get(keys);
      return out;
    }
    if (Array.isArray(keys)) {
      for (const key of keys) out[key] = store.get(key);
      return out;
    }
    for (const [key, fallback] of Object.entries(keys)) {
      out[key] = store.has(key) ? store.get(key) : fallback;
    }
    return out;
  };

  return {
    MockEngineError,
    sessionStore,
    localStore,
    runtimeMessageListeners,
    windowRemovedListeners,
    ensureDefaultPoliciesInstalled: vi.fn(async () => undefined),
    getActivePolicyRpcManifests: vi.fn(() => [{ id: "manifest-a" }]),
    pendingPut: vi.fn(async () => undefined),
    pendingDelete: vi.fn(async () => undefined),
    auditAppend: vi.fn(async () => undefined),
    evaluateWithPolicyRpc: vi.fn(),
    evaluateWithEnvelopes: vi.fn<
      (...args: unknown[]) => Promise<unknown>
    >(async () => ({ kind: "pass" })),
    tryDeclarativeRoute: vi.fn<
      (...args: unknown[]) => Promise<unknown>
    >(async () => ({
      kind: "miss",
      reason: "no_selector",
    })),
    browser: {
      storage: {
        session: {
          get: vi.fn((keys?: string | string[] | Record<string, unknown>) =>
            readStore(sessionStore, keys),
          ),
          set: vi.fn(async (entries: Record<string, unknown>) => {
            for (const [key, value] of Object.entries(entries))
              sessionStore.set(key, value);
          }),
        },
        local: {
          get: vi.fn((keys?: string | string[] | Record<string, unknown>) =>
            readStore(localStore, keys),
          ),
          set: vi.fn(async (entries: Record<string, unknown>) => {
            for (const [key, value] of Object.entries(entries))
              localStore.set(key, value);
          }),
        },
      },
      windows: {
        create: vi.fn(async () => ({ id: 99 })),
        remove: vi.fn(async () => undefined),
        onRemoved: {
          addListener: vi.fn((listener: (windowId: number) => void) => {
            windowRemovedListeners.push(listener);
          }),
          removeListener: vi.fn((listener: (windowId: number) => void) => {
            const index = windowRemovedListeners.indexOf(listener);
            if (index >= 0) windowRemovedListeners.splice(index, 1);
          }),
        },
      },
      runtime: {
        getURL: vi.fn((path: string) => `chrome-extension://scopeball/${path}`),
        sendMessage: vi.fn(async () => undefined),
        onMessage: {
          addListener: vi.fn((listener: (message: unknown) => void) => {
            runtimeMessageListeners.push(listener);
          }),
          removeListener: vi.fn((listener: (message: unknown) => void) => {
            const index = runtimeMessageListeners.indexOf(listener);
            if (index >= 0) runtimeMessageListeners.splice(index, 1);
          }),
        },
      },
    },
  };
});

vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));
vi.mock("../policies-loader", () => ({
  ensureDefaultPoliciesInstalled: mocks.ensureDefaultPoliciesInstalled,
  getActivePolicyRpcManifests: mocks.getActivePolicyRpcManifests,
}));
vi.mock("../storage", () => ({
  pendingPut: mocks.pendingPut,
  pendingDelete: mocks.pendingDelete,
  auditAppend: mocks.auditAppend,
}));
vi.mock("../wasm-bridge", () => ({
  EngineError: mocks.MockEngineError,
  evaluateWithEnvelopes: mocks.evaluateWithEnvelopes,
}));
vi.mock("../policy-rpc", () => ({
  evaluateWithPolicyRpc: mocks.evaluateWithPolicyRpc,
  // Pass-through so the orchestrator's audit-log builder behaves like
  // the real `formatAuditMatched`. Mirrors the trivial D9-aware impl.
  formatAuditMatched: (verdict: { matched?: { policy_id: string; severity: string; reason?: string }[] }) =>
    (verdict.matched ?? []).map((m) => {
      const base: { id: string; severity: string; reason?: string } = {
        id: m.policy_id,
        severity: m.severity,
      };
      if (m.policy_id === "__system__" && typeof m.reason === "string") {
        base.reason = m.reason;
      }
      return base;
    }),
}));
// Phase 6 — orchestrator calls `tryDeclarativeRoute` on every transaction.
// We stub it to a fast "no_selector" miss so tests that don't care about
// the declarative path don't have to mock the WASM bridge + JIT fetcher.
vi.mock("../adapter-loader/declarative-route", () => ({
  tryDeclarativeRoute: mocks.tryDeclarativeRoute,
}));

import { decideMessage } from "../orchestrator";

function txMessage(requestId = "req-1"): Message {
  return {
    requestId,
    data: {
      type: RequestType.TRANSACTION,
      chainId: 1,
      hostname: "app.example",
      transaction: {
        from: OWNER,
        to: ROUTER,
        value: "0xde0b6b3a7640000",
        data: "0x",
      },
    },
  } as Message;
}

function untypedMessage(requestId = "sig-1"): Message {
  return {
    requestId,
    data: {
      type: RequestType.UNTYPED_SIGNATURE,
      hostname: "app.example",
      message: "sign this opaque payload",
    },
  };
}

function approve(requestId: string, ok: boolean): void {
  for (const listener of [...mocks.runtimeMessageListeners]) {
    listener({ type: "scopeball:verdict-decision", requestId, ok });
  }
}

describe("orchestrator", () => {
  beforeEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
    mocks.sessionStore.clear();
    mocks.localStore.clear();
    mocks.runtimeMessageListeners.length = 0;
    mocks.windowRemovedListeners.length = 0;
    mocks.evaluateWithPolicyRpc.mockResolvedValue({
      verdict: { kind: "pass" },
      audit: {
        request_id: "stubbed-tx-1",
        manifest_set_hash: "sha256:manifest",
        schema_hash: "sha256:schema",
        call_ids: [],
        methods: [],
      },
    });
    mocks.tryDeclarativeRoute.mockResolvedValue({
      kind: "miss",
      reason: "no_selector",
    });
    mocks.evaluateWithEnvelopes.mockResolvedValue({ kind: "pass" });
  });

  it("evaluates transactions through policy-rpc coordinator", async () => {
    const result = await decideMessage(txMessage("stubbed-tx-1"));

    expect(result.ok).toBe(true);
    expect(result.verdict.kind).toBe("pass");
    expect(mocks.evaluateWithPolicyRpc).toHaveBeenCalledWith(
      txMessage("stubbed-tx-1"),
      { manifests: [{ id: "manifest-a" }] },
    );
    expect(mocks.pendingPut).toHaveBeenCalledOnce();
    expect(mocks.pendingDelete).toHaveBeenCalledWith("stubbed-tx-1");
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({
        policyRpc: expect.objectContaining({
          manifest_set_hash: "sha256:manifest",
          schema_hash: "sha256:schema",
        }),
      }),
    );
  });

  it("records declarative-route hit metadata in the audit trail", async () => {
    mocks.tryDeclarativeRoute.mockResolvedValueOnce({
      kind: "hit",
      value: {
        envelopes: [
          { category: "dex", action: "swap", fields: { swapMode: "exact_in" } },
        ],
        decoderId: "declarative.uniswap/v2/swapExactTokensForTokens",
        bundleId: "uniswap/v2/swapExactTokensForTokens@1.0.0",
        source: "layer1",
      },
    });

    const result = await decideMessage(txMessage("stubbed-tx-1"));
    expect(result.ok).toBe(true);
    expect(mocks.tryDeclarativeRoute).toHaveBeenCalledOnce();
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({
        declarative: expect.objectContaining({
          outcome: "hit",
          source: "layer1",
          decoder_id: "declarative.uniswap/v2/swapExactTokensForTokens",
          bundle_id: "uniswap/v2/swapExactTokensForTokens@1.0.0",
          envelope_count: 1,
        }),
      }),
    );
  });

  it("records declarative-route miss in the audit trail", async () => {
    mocks.tryDeclarativeRoute.mockResolvedValueOnce({
      kind: "miss",
      reason: "no_publisher",
    });

    const result = await decideMessage(txMessage("stubbed-tx-2"));
    expect(result.ok).toBe(true);
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({
        declarative: { outcome: "miss", reason: "no_publisher" },
      }),
    );
  });

  // ── Phase 7F — declarative verdict path ──────────────────────────────
  // When the declarative router returns a hit with ≥1 envelope, the
  // orchestrator now routes the verdict through `evaluate_with_envelopes_json`
  // (mocked here as `evaluateWithEnvelopes`). For miss/fault/empty outcomes
  // it falls back to the static `evaluateWithPolicyRpc` path. The audit
  // log's `verdictSource` field captures which path won.

  const hitOutcome = {
    kind: "hit" as const,
    value: {
      envelopes: [
        {
          category: "dex",
          action: "swap",
          fields: { swapMode: "exact_in" },
        },
      ],
      decoderId: "declarative.uniswap/v2/swapExactTokensForTokens",
      bundleId: "uniswap/v2/swapExactTokensForTokens@1.0.0",
      source: "layer1" as const,
    },
  };

  it("phase7F: declarative hit drives Cedar verdict (verdictSource=declarative)", async () => {
    mocks.tryDeclarativeRoute.mockResolvedValueOnce(hitOutcome);
    mocks.evaluateWithEnvelopes.mockResolvedValueOnce({ kind: "pass" });

    const result = await decideMessage(txMessage("decl-hit-1"));

    expect(result.ok).toBe(true);
    expect(result.verdict.kind).toBe("pass");
    // The declarative path runs evaluateWithEnvelopes…
    expect(mocks.evaluateWithEnvelopes).toHaveBeenCalledOnce();
    const [evalArgs] = mocks.evaluateWithEnvelopes.mock.calls[0] as [
      Record<string, unknown>,
    ];
    expect(evalArgs.envelopes).toEqual(hitOutcome.value.envelopes);
    expect(evalArgs.chain_id).toBe(1);
    expect(evalArgs.manifests).toEqual([{ id: "manifest-a" }]);
    // …and the static path stays out of the way.
    expect(mocks.evaluateWithPolicyRpc).not.toHaveBeenCalled();
    // Audit log records the declarative source.
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({
        verdictSource: "declarative",
        declarative: expect.objectContaining({
          outcome: "hit",
          decoder_id: "declarative.uniswap/v2/swapExactTokensForTokens",
          envelope_count: 1,
        }),
      }),
    );
  });

  it("phase7F: declarative miss falls back to static path (verdictSource=static)", async () => {
    mocks.tryDeclarativeRoute.mockResolvedValueOnce({
      kind: "miss",
      reason: "no_publisher",
    });

    const result = await decideMessage(txMessage("decl-miss-1"));

    expect(result.ok).toBe(true);
    expect(mocks.evaluateWithEnvelopes).not.toHaveBeenCalled();
    expect(mocks.evaluateWithPolicyRpc).toHaveBeenCalledOnce();
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({
        verdictSource: "static",
        declarative: { outcome: "miss", reason: "no_publisher" },
      }),
    );
  });

  it("phase7F: declarative fault falls back to static path (verdictSource=static)", async () => {
    mocks.tryDeclarativeRoute.mockResolvedValueOnce({
      kind: "fault",
      reason: "map_failed",
      cause: new Error("mapper rejected decoded call"),
    });

    const result = await decideMessage(txMessage("decl-fault-1"));

    expect(result.ok).toBe(true);
    expect(mocks.evaluateWithEnvelopes).not.toHaveBeenCalled();
    expect(mocks.evaluateWithPolicyRpc).toHaveBeenCalledOnce();
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({
        verdictSource: "static",
        declarative: { outcome: "fault", reason: "map_failed" },
      }),
    );
  });

  it("phase7F: declarative hit with 0 envelopes falls back to static path", async () => {
    mocks.tryDeclarativeRoute.mockResolvedValueOnce({
      kind: "hit",
      value: {
        envelopes: [],
        decoderId: "declarative.something/empty",
        bundleId: "something/empty@1.0.0",
        source: "layer1",
      },
    });

    const result = await decideMessage(txMessage("decl-empty-1"));

    expect(result.ok).toBe(true);
    expect(mocks.evaluateWithEnvelopes).not.toHaveBeenCalled();
    expect(mocks.evaluateWithPolicyRpc).toHaveBeenCalledOnce();
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({
        verdictSource: "static",
        declarative: expect.objectContaining({
          outcome: "hit",
          envelope_count: 0,
        }),
      }),
    );
  });

  it("phase7F: evaluateWithEnvelopes throw falls through to static path", async () => {
    mocks.tryDeclarativeRoute.mockResolvedValueOnce(hitOutcome);
    mocks.evaluateWithEnvelopes.mockRejectedValueOnce(
      new mocks.MockEngineError(
        "installed_manifest_hash_mismatch",
        "stale manifests",
      ),
    );

    const result = await decideMessage(txMessage("decl-eval-throw-1"));

    expect(result.ok).toBe(true);
    expect(mocks.evaluateWithEnvelopes).toHaveBeenCalledOnce();
    // Fall-through means the static path still runs and produces the
    // final verdict.
    expect(mocks.evaluateWithPolicyRpc).toHaveBeenCalledOnce();
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({
        verdictSource: "static",
      }),
    );
  });

  it("does not call declarative-route for untyped signatures", async () => {
    const result = decideMessage(untypedMessage("sig-skip"), {
      onAwaitingUser: vi.fn(),
    });
    await vi.waitFor(() =>
      expect(mocks.browser.windows.create).toHaveBeenCalledTimes(1),
    );
    approve("sig-skip", true);
    await result;
    expect(mocks.tryDeclarativeRoute).not.toHaveBeenCalled();
  });

  it("lets the user explicitly approve unsupported untyped signatures", async () => {
    const result = decideMessage(untypedMessage(), { onAwaitingUser: vi.fn() });
    await vi.waitFor(() =>
      expect(mocks.browser.windows.create).toHaveBeenCalledTimes(1),
    );

    approve("sig-1", true);

    await expect(result).resolves.toMatchObject({
      ok: true,
      verdict: { kind: "warn" },
    });
  });
});
