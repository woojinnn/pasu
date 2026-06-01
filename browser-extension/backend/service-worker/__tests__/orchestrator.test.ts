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
    pendingPut: vi.fn(async () => undefined),
    pendingDelete: vi.fn(async () => undefined),
    auditAppend: vi.fn(async () => undefined),
    // Phase 1 / P2 — v2 ActionBody verdict path. The v3 route defaults to a
    // miss so every EXISTING test fails closed; the v2 plan/dispatch/evaluate
    // mocks are only exercised by the dedicated v2-path cases that override
    // the v3 route to a hit.
    planActionRpcV2: vi.fn<(...args: unknown[]) => Promise<unknown>>(
      async () => [],
    ),
    evaluateActionV2: vi.fn<(...args: unknown[]) => Promise<unknown>>(
      async () => ({ kind: "pass" }),
    ),
    dispatchCallsV2: vi.fn<(...args: unknown[]) => Promise<unknown>>(
      async () => ({}),
    ),
    getDefaultPolicyBundlesV2: vi.fn<() => unknown[]>(() => [
      { policy: "forbid(...);", manifest: { id: "high-slippage-warning", schema_version: 2 } },
    ]),
    loadDefaultPolicySetV2: vi.fn<(...args: unknown[]) => Promise<unknown>>(
      async () => [],
    ),
    tryDeclarativeRouteV3: vi.fn<
      (...args: unknown[]) => Promise<unknown>
    >(async () => ({
      kind: "miss",
      reason: "bundle_not_installed",
    })),
    // Typed-data signature router. Default `null` (no published manifest) so
    // existing typed-sig cases fail closed; the routed-hit case overrides it.
    routeTypedSignaturePayload: vi.fn<
      (...args: unknown[]) => Promise<unknown>
    >(async () => null),
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
}));
vi.mock("../storage", () => ({
  pendingPut: mocks.pendingPut,
  pendingDelete: mocks.pendingDelete,
  auditAppend: mocks.auditAppend,
}));
vi.mock("../wasm-bridge", () => ({
  EngineError: mocks.MockEngineError,
  planActionRpcV2: mocks.planActionRpcV2,
  evaluateActionV2: mocks.evaluateActionV2,
}));
vi.mock("../policies-loader-v2", () => ({
  getDefaultPolicyBundlesV2: mocks.getDefaultPolicyBundlesV2,
  loadDefaultPolicySetV2: mocks.loadDefaultPolicySetV2,
}));
vi.mock("../policy-rpc", () => ({
  dispatchCallsV2: mocks.dispatchCallsV2,
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
// Phase 4B / P3 — the orchestrator calls `tryDeclarativeRouteV3` on every
// transaction; we stub it to a fast miss so tests that don't care about the
// v3 decode path don't have to mock the WASM bridge + JIT fetcher. The v1
// `tryDeclarativeRoute` export was removed in the Phase 1/P3 v1 cutover.
vi.mock("../adapter-loader/declarative-route", () => ({
  tryDeclarativeRouteV3: mocks.tryDeclarativeRouteV3,
}));
// Typed-data signature router (`typedSignatureLifecycle` calls this). Mocked so
// tests don't pull in the real WASM `declarative_route_typed_data_v3_json` +
// registry `by-typed-data/` fetch.
vi.mock("../sig-routing", () => ({
  routeTypedSignaturePayload: mocks.routeTypedSignaturePayload,
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

function typedSigMessage(requestId = "typed-1"): Message {
  return {
    requestId,
    data: {
      type: RequestType.TYPED_SIGNATURE,
      chainId: 1,
      hostname: "app.example",
      address: OWNER,
      typedData: {
        primaryType: "Permit",
        domain: { verifyingContract: ROUTER },
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

/**
 * Drive a request that is expected to fail closed (a warn verdict that opens
 * the verdict window and AWAITS the user's choice). We must NOT `await
 * decideMessage` before approving — that would deadlock and (because the
 * per-actor lock is still held) cascade into later same-actor cases.
 */
async function decideAndApprove(
  message: Message,
  ok: boolean,
): Promise<{ ok: boolean; verdict: { kind: string } }> {
  const callsBefore = mocks.browser.windows.create.mock.calls.length;
  const result = decideMessage(message, { onAwaitingUser: vi.fn() });
  await vi.waitFor(() =>
    expect(mocks.browser.windows.create.mock.calls.length).toBe(
      callsBefore + 1,
    ),
  );
  approve(message.requestId, ok);
  return result;
}

describe("orchestrator", () => {
  beforeEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
    mocks.sessionStore.clear();
    mocks.localStore.clear();
    mocks.runtimeMessageListeners.length = 0;
    mocks.windowRemovedListeners.length = 0;
    // v3 route default: miss → fail-closed path. The v2 plan/dispatch/evaluate
    // mocks resolve to inert pass-shaped values; only the v2-path cases below
    // override the v3 route to a hit.
    mocks.tryDeclarativeRouteV3.mockResolvedValue({
      kind: "miss",
      reason: "bundle_not_installed",
    });
    mocks.planActionRpcV2.mockResolvedValue([]);
    mocks.dispatchCallsV2.mockResolvedValue({});
    mocks.evaluateActionV2.mockResolvedValue({ kind: "pass" });
    mocks.getDefaultPolicyBundlesV2.mockReturnValue([
      {
        policy: "forbid(...);",
        manifest: { id: "high-slippage-warning", schema_version: 2 },
      },
    ]);
    // Typed-sig router default: miss → fail-closed warn. The typed-sig hit
    // case overrides it.
    mocks.routeTypedSignaturePayload.mockResolvedValue(null);
  });

  // ── Phase 1 / P2 — v2 ActionBody verdict path ───────────────────────
  // When the v3 route HITS with a real (non-`Unknown`) ActionBody, the
  // stateless v2 pipeline (planActionRpcV2 → dispatchCallsV2 →
  // evaluateActionV2) drives the verdict (verdictSource="declarative-v2").
  // A v3 miss/fault, an all-`Unknown` hit, no v2 bundle, or a plan/dispatch
  // throw fails closed (verdictSource="fail_closed", warn + user-proceed).

  // One real swap Action: `{ meta, body }` where `body.domain !== "unknown"`.
  const v3SwapAction = {
    meta: {
      submitted_at: { unix: 1_738_000_000 },
      submitter: OWNER,
      nature: { kind: "onchain_tx" },
    },
    body: { domain: "amm", swap: { recipient: OWNER } },
  };
  const v3HitOutcome = {
    kind: "hit" as const,
    value: {
      actions: [v3SwapAction],
      decoderId: "registry-v2.uniswap/v3/exactInputSingle",
    },
  };

  it("p2: v3 hit with a real ActionBody drives the v2 verdict (verdictSource=declarative-v2)", async () => {
    mocks.tryDeclarativeRouteV3.mockResolvedValueOnce(v3HitOutcome);
    const planned = [
      {
        manifest_id: "large-swap-usd-warning",
        call_id: "large-swap-usd-warning::total-input-usd",
        method: "oracle.usd_value",
        params: {},
        outputs: [],
        optional: false,
      },
    ];
    mocks.planActionRpcV2.mockResolvedValueOnce(planned);
    mocks.dispatchCallsV2.mockResolvedValueOnce({
      "large-swap-usd-warning::total-input-usd": { usd: "3500.1200" },
    });
    // Warn is the realistic v2 verdict — BOTH shipped default v2 policies are
    // `@severity("warn")`. The user must then approve via the verdict window.
    mocks.evaluateActionV2.mockResolvedValueOnce({
      kind: "warn",
      matched: [
        {
          policy_id: "large-input",
          reason: "large USD input",
          severity: "warn",
          origin: "action",
        },
      ],
    });

    const decided = await decideAndApprove(txMessage("v2-hit-1"), true);

    expect(decided.ok).toBe(true);
    expect(decided.verdict.kind).toBe("warn");
    // v2 pipeline drove the decision.
    expect(mocks.planActionRpcV2).toHaveBeenCalledOnce();
    expect(mocks.dispatchCallsV2).toHaveBeenCalledOnce();
    expect(mocks.evaluateActionV2).toHaveBeenCalledOnce();
    // The plan + evaluate calls split `action=a.body` / `meta=a.meta` and use
    // the CAIP-2 chain id.
    const planArgs = mocks.planActionRpcV2.mock.calls[0][0] as Record<
      string,
      unknown
    >;
    expect(planArgs.action).toEqual(v3SwapAction.body);
    expect(planArgs.meta).toEqual(v3SwapAction.meta);
    expect((planArgs.tx as { chain_id: string }).chain_id).toBe("eip155:1");
    // evaluate receives the per-action results map verbatim + the bundles.
    const evalArgs = mocks.evaluateActionV2.mock.calls[0][0] as Record<
      string,
      unknown
    >;
    expect(evalArgs.results).toEqual({
      "large-swap-usd-warning::total-input-usd": { usd: "3500.1200" },
    });
    expect((evalArgs.bundles as unknown[]).length).toBe(1);
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({ verdictSource: "declarative-v2" }),
    );
  });

  it("p2: a v2 fail verdict is honoured, not treated as a fail-close", async () => {
    mocks.tryDeclarativeRouteV3.mockResolvedValueOnce(v3HitOutcome);
    mocks.planActionRpcV2.mockResolvedValueOnce([]);
    mocks.dispatchCallsV2.mockResolvedValueOnce({});
    mocks.evaluateActionV2.mockResolvedValueOnce({
      kind: "fail",
      matched: [
        {
          policy_id: "__system__",
          reason: "required policy-rpc result missing",
          severity: "deny",
          origin: "tx",
        },
      ],
    });

    const result = await decideMessage(txMessage("v2-fail-1"));

    // Fail verdicts surface as a non-ok decision via the v2 path.
    expect(result.verdict.kind).toBe("fail");
    expect(result.ok).toBe(false);
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({ verdictSource: "declarative-v2" }),
    );
  });

  // ── Phase 1 / P3 — FAIL-CLOSED tail ──────────────────────────────────
  // Every case the deleted legacy declarative/static path used to
  // catch now emits the `__engine::no_decoder` warn verdict, which requires
  // the user to explicitly proceed. The v2 plan/evaluate path never runs.

  it("p3: a v3 hit with only an Unknown ActionBody fails closed", async () => {
    mocks.tryDeclarativeRouteV3.mockResolvedValueOnce({
      kind: "hit",
      value: {
        actions: [
          {
            meta: { submitter: OWNER },
            body: { domain: "unknown", target: ROUTER, calldata: "0x" },
          },
        ],
        decoderId: "",
      },
    });

    const result = await decideAndApprove(txMessage("p3-unknown-1"), true);

    expect(result.ok).toBe(true);
    expect(result.verdict.kind).toBe("warn");
    // Unknown body → v2 skipped entirely.
    expect(mocks.planActionRpcV2).not.toHaveBeenCalled();
    expect(mocks.evaluateActionV2).not.toHaveBeenCalled();
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({
        verdictSource: "fail_closed",
        matchedPolicies: [
          expect.objectContaining({ id: "__engine::no_decoder" }),
        ],
      }),
    );
  });

  it("p3: a v3 miss fails closed (verdictSource=fail_closed)", async () => {
    // v3 route default (beforeEach) is a miss → fail-closed.
    const result = await decideAndApprove(txMessage("p3-miss-1"), true);

    expect(result.ok).toBe(true);
    expect(result.verdict.kind).toBe("warn");
    expect(mocks.planActionRpcV2).not.toHaveBeenCalled();
    expect(mocks.evaluateActionV2).not.toHaveBeenCalled();
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({ verdictSource: "fail_closed" }),
    );
  });

  it("p3: a user-cancelled fail-close yields a non-ok decision", async () => {
    const result = await decideAndApprove(txMessage("p3-cancel-1"), false);
    expect(result.ok).toBe(false);
    expect(result.verdict.kind).toBe("warn");
  });

  it("p3: a v3 fault fails closed (verdictSource=fail_closed)", async () => {
    mocks.tryDeclarativeRouteV3.mockResolvedValueOnce({
      kind: "fault",
      reason: "engine_error",
      cause: new mocks.MockEngineError("engine_error", "v3 decode blew up"),
    });

    const result = await decideAndApprove(txMessage("p3-fault-1"), true);

    expect(result.ok).toBe(true);
    expect(mocks.planActionRpcV2).not.toHaveBeenCalled();
    expect(mocks.evaluateActionV2).not.toHaveBeenCalled();
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({ verdictSource: "fail_closed" }),
    );
  });

  it("p3: a planActionRpcV2 throw fails closed", async () => {
    mocks.tryDeclarativeRouteV3.mockResolvedValueOnce(v3HitOutcome);
    mocks.planActionRpcV2.mockRejectedValueOnce(
      new mocks.MockEngineError("plan_failed", "cannot lower action"),
    );

    const result = await decideAndApprove(txMessage("p3-plan-throw-1"), true);

    expect(result.ok).toBe(true);
    expect(mocks.planActionRpcV2).toHaveBeenCalledOnce();
    // evaluate never ran; the lifecycle fell through to the fail-closed tail.
    expect(mocks.evaluateActionV2).not.toHaveBeenCalled();
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({ verdictSource: "fail_closed" }),
    );
  });

  it("p3: a v3 hit but no v2 bundles loaded fails closed", async () => {
    mocks.tryDeclarativeRouteV3.mockResolvedValueOnce(v3HitOutcome);
    mocks.getDefaultPolicyBundlesV2.mockReturnValueOnce([]);

    const result = await decideAndApprove(txMessage("p3-nobundles-1"), true);

    expect(result.ok).toBe(true);
    expect(mocks.planActionRpcV2).not.toHaveBeenCalled();
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({ verdictSource: "fail_closed" }),
    );
  });

  it("p3: a typed signature with no published manifest fails closed (route miss)", async () => {
    // `typedSignatureLifecycle` routes through the by-typed-data index; a miss
    // (router → null, the default) warn-closes (no decoder, user-proceed).
    const result = await decideAndApprove(typedSigMessage("p3-typed-1"), true);

    expect(result.ok).toBe(true);
    expect(result.verdict.kind).toBe("warn");
    // The typed-sig path uses `routeTypedSignaturePayload`, NOT the tx v3 route;
    // a miss never reaches the v2 plan/dispatch/evaluate loop.
    expect(mocks.tryDeclarativeRouteV3).not.toHaveBeenCalled();
    expect(mocks.planActionRpcV2).not.toHaveBeenCalled();
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({
        verdictSource: "fail_closed",
        matchedPolicies: [
          expect.objectContaining({ id: "__engine::no_decoder" }),
        ],
      }),
    );
  });

  // ── Typed-data signature verdict path (typedSignatureLifecycle) ──────────
  // A routed hit decodes the EIP-712 message into an Action[] and drives the
  // SAME v2 pipeline the tx path uses; the only difference is the `tx` context
  // (`from`=signer, `to`=verifyingContract). warn-closed on miss/fault.

  const sigPermitAction = {
    meta: {
      submitted_at: { unix: 1_700_000_000 },
      submitter: OWNER,
      nature: { kind: "offchain_sig" },
    },
    body: {
      domain: "token",
      token: {
        action: "permit2_sign_allowance",
        permit2_sign_allowance: { spender: ROUTER, amount: "1000" },
      },
    },
  };

  it("typed sig: a routed hit drives the v2 verdict (verdictSource=declarative-v2, tx.to=verifyingContract)", async () => {
    mocks.routeTypedSignaturePayload.mockResolvedValueOnce({
      actions: [sigPermitAction],
      decoderId: "uniswap/permit2/permitSingle@1.0.0",
    });
    // evaluate → pass (beforeEach default) → decideMessage resolves w/o a window.

    const { ok, verdict } = await decideMessage(typedSigMessage("typed-hit-1"), {
      onAwaitingUser: vi.fn(),
    });

    expect(ok).toBe(true);
    expect(verdict.kind).toBe("pass");
    // v2 pipeline drove the verdict from the SIG decode (not the tx v3 route).
    expect(mocks.tryDeclarativeRouteV3).not.toHaveBeenCalled();
    expect(mocks.planActionRpcV2).toHaveBeenCalledOnce();
    expect(mocks.evaluateActionV2).toHaveBeenCalledOnce();
    // typed-sig `tx` context: from=signer, to=verifyingContract (lowercased),
    // CAIP-2 chain id. `action`/`meta` split from the routed Action.
    const planArgs = mocks.planActionRpcV2.mock.calls[0][0] as Record<
      string,
      unknown
    >;
    expect(planArgs.action).toEqual(sigPermitAction.body);
    expect(planArgs.meta).toEqual(sigPermitAction.meta);
    const tx = planArgs.tx as { chain_id: string; from: string; to: string };
    expect(tx.chain_id).toBe("eip155:1");
    expect(tx.from).toBe(OWNER);
    expect(tx.to).toBe(ROUTER.toLowerCase());
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({ verdictSource: "declarative-v2" }),
    );
  });

  it("typed sig: a routed hit logs a readable off-chain signature summary to DevTools", async () => {
    const infoSpy = vi.spyOn(console, "info").mockImplementation(() => {});
    mocks.routeTypedSignaturePayload.mockResolvedValueOnce({
      actions: [sigPermitAction],
      decoderId: "uniswap/permit2/permitSingle@1.0.0",
    });

    await decideMessage(typedSigMessage("typed-log-1"), {
      onAwaitingUser: vi.fn(),
    });

    const summary = infoSpy.mock.calls
      .map((call) => String(call[0]))
      .find((line) => line.startsWith("[Scopeball] off-chain signature parsed"));
    expect(summary).toBeDefined();
    // EIP-712 primaryType + routing decoder + the decoded action tag/fields are
    // all surfaced in one readable line.
    expect(summary).toContain("Permit");
    expect(summary).toContain("uniswap/permit2/permitSingle@1.0.0");
    expect(summary).toContain("permit2_sign_allowance");
    expect(summary).toContain("spender=");
    expect(summary).toContain("amount=1000");
    infoSpy.mockRestore();
  });

  it("typed sig: a routed hit with a warn verdict opens the verdict window", async () => {
    mocks.routeTypedSignaturePayload.mockResolvedValueOnce({
      actions: [sigPermitAction],
      decoderId: "uniswap/permit2/permitSingle@1.0.0",
    });
    mocks.evaluateActionV2.mockResolvedValueOnce({
      kind: "warn",
      matched: [
        {
          policy_id: "permit2-unlimited-approve-warning",
          reason: "unlimited Permit2 approval",
          severity: "warn",
          origin: "action",
        },
      ],
    });

    const decided = await decideAndApprove(typedSigMessage("typed-warn-1"), true);

    expect(decided.ok).toBe(true);
    expect(decided.verdict.kind).toBe("warn");
    expect(mocks.evaluateActionV2).toHaveBeenCalledOnce();
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({ verdictSource: "declarative-v2" }),
    );
  });

  it("does not call the v3 route for untyped signatures", async () => {
    const result = await decideAndApprove(untypedMessage("sig-skip"), true);
    expect(result.ok).toBe(true);
    expect(mocks.tryDeclarativeRouteV3).not.toHaveBeenCalled();
  });

  it("lets the user explicitly approve unsupported untyped signatures", async () => {
    const result = await decideAndApprove(untypedMessage(), true);
    await expect(Promise.resolve(result)).resolves.toMatchObject({
      ok: true,
      verdict: { kind: "warn" },
    });
    expect(mocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({
        verdictSource: "fail_closed",
        matchedPolicies: [
          expect.objectContaining({
            id: "__engine::unsupported_untyped_signature",
          }),
        ],
      }),
    );
  });
});
