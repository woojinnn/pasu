/**
 * T-TEST-ENVELOPE — Cedar verdict source audit edge cases (Groups B + C).
 *
 * Plan §B4 (2026-05-28) — Group A (`enrichEnvelopeAssets edge cases`)
 * removed alongside the v1 source. Groups B + C remain under `describe.skip`
 * pending Cedar v3 verdict re-integration (별 plan `cedar-verdict-v3-integration.md`).
 *
 * Coverage matrix (Groups B + C, currently skipped):
 *
 *   4. verdictSource_audit_declarative_path_logged
 *      — Orchestrator integration: declarative-route hit + envelope ≥ 1 →
 *        `evaluateWithEnvelopes` runs → audit log records
 *        `verdictSource: "declarative"`.
 *
 *   5. verdictSource_audit_static_path_fallback_logged
 *      — Orchestrator integration: declarative-route miss → static
 *        `evaluateWithPolicyRpc` runs → audit log records
 *        `verdictSource: "static"`.
 *
 *   6. default_policy_evaluates_v2_swap_with_pass_verdict
 *      — V2 swap envelope with known WETH + USDC + non-zero recipient
 *        passes the 10 default policies. We assert the orchestrator emits a
 *        `verdict: "pass"` audit log for this happy path. evaluateWithEnvelopes
 *        is mocked to return `pass` (mirroring Cedar engine semantics).
 *
 *   7. default_policy_evaluates_v2_swap_with_zero_recipient_denies
 *      — V2 swap envelope with `recipient = 0x0..0`. The default
 *        `forbid-zero-recipient` policy triggers. evaluateWithEnvelopes is
 *        mocked to return `fail` with the matching policy id — we assert the
 *        orchestrator surfaces `verdict.kind === "fail"`.
 *
 *   8. default_policy_evaluates_max_input_usd_3_denies_if_oracle_resolves
 *      — Documents the PoC limitation: `max-input-usd-3` policy declares a
 *        host:oracle requirement that the declarative verdict path
 *        intentionally does NOT yet wire (`rpc_response.results: []`). The
 *        WASM falls closed via `__engine::projection_failed`. We assert that
 *        evaluateWithEnvelopes returning this engine error → orchestrator
 *        records `verdict.kind === "fail"` with the synthetic policy id.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { RequestType, type Message } from "@lib/types";

const WETH_MAINNET = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const USDC_MAINNET = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";

/**
 * Make a V2 swap envelope with the given input asset (WETH default), the
 * given output asset (USDC default), and the given recipient. Plan §B4
 * (2026-05-28) — kept module-level even after Group A removal because
 * Group C orchestrator tests share the same envelope shape; cedar verdict
 * v3 migration (별 plan `cedar-verdict-v3-integration.md`) will repoint
 * these fixtures at the v3 Action shape.
 */
function makeSwapEnvelope(opts: {
  inputAddress?: string;
  outputAddress?: string;
  recipient?: string;
  inputAmount?: string;
  inputSymbol?: string;
  outputSymbol?: string;
}): Record<string, unknown> {
  return {
    category: "dex",
    action: "swap",
    fields: {
      swapMode: "exact_in",
      inputToken: {
        asset: {
          kind: "erc20",
          address: opts.inputAddress ?? WETH_MAINNET,
          ...(opts.inputSymbol !== undefined
            ? { symbol: opts.inputSymbol }
            : {}),
        },
        amount: { kind: "exact", value: opts.inputAmount ?? "1000" },
      },
      outputToken: {
        asset: {
          kind: "erc20",
          address: opts.outputAddress ?? USDC_MAINNET,
          ...(opts.outputSymbol !== undefined
            ? { symbol: opts.outputSymbol }
            : {}),
        },
        amount: { kind: "min", value: "1" },
      },
      recipient: opts.recipient ?? "0x2222222222222222222222222222222222222222",
    },
  };
}

// ─── Groups B + C: orchestrator-driven verdictSource + Cedar verdict ──────
//
// These tests need the orchestrator wiring — separate `describe` block so
// the `vi.mock` graph reaches `../orchestrator` instead of the adapter-loader
// router boundaries.

const orchestratorMocks = vi.hoisted(() => {
  const sessionStore = new Map<string, unknown>();
  const localStore = new Map<string, unknown>();
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
            for (const [k, v] of Object.entries(entries))
              sessionStore.set(k, v);
          }),
        },
        local: {
          get: vi.fn((keys?: string | string[] | Record<string, unknown>) =>
            readStore(localStore, keys),
          ),
          set: vi.fn(async (entries: Record<string, unknown>) => {
            for (const [k, v] of Object.entries(entries)) localStore.set(k, v);
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
            const idx = windowRemovedListeners.indexOf(listener);
            if (idx >= 0) windowRemovedListeners.splice(idx, 1);
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
            const idx = runtimeMessageListeners.indexOf(listener);
            if (idx >= 0) runtimeMessageListeners.splice(idx, 1);
          }),
        },
      },
    },
  };
});

vi.mock("../../policies-loader", () => ({
  ensureDefaultPoliciesInstalled: orchestratorMocks.ensureDefaultPoliciesInstalled,
  getActivePolicyRpcManifests: orchestratorMocks.getActivePolicyRpcManifests,
}));
vi.mock("../../storage", () => ({
  pendingPut: orchestratorMocks.pendingPut,
  pendingDelete: orchestratorMocks.pendingDelete,
  auditAppend: orchestratorMocks.auditAppend,
}));
// `../../wasm-bridge` is already mocked above for Group A; vitest dedupes
// the mock factory by module specifier so we override here to give the
// orchestrator a richer mock surface.
vi.doMock("../../wasm-bridge", () => ({
  EngineError: orchestratorMocks.MockEngineError,
  evaluateWithEnvelopes: orchestratorMocks.evaluateWithEnvelopes,
}));
// `../../policy-rpc` exposes both `evaluateWithPolicyRpc` (static verdict
// path) and `formatAuditMatched` (D9 — projects a VerdictDto into the
// audit-log matched-policies list). The orchestrator imports BOTH, so the
// mock must provide both. `formatAuditMatched` is a pure projection with no
// side effects — we mirror the real implementation (policy-rpc.ts:156) so
// the audit-row assertions (`matchedPolicies` shaped `{ id, severity }`)
// observe identical behaviour to production.
const SYSTEM_POLICY_ID = "__system__";
vi.mock("../../policy-rpc", () => ({
  evaluateWithPolicyRpc: orchestratorMocks.evaluateWithPolicyRpc,
  SYSTEM_POLICY_ID,
  formatAuditMatched: (verdict: {
    matched?: Array<{
      policy_id: string;
      severity: string;
      reason?: string;
    }>;
  }) => {
    if (!verdict.matched) return [];
    return verdict.matched.map((m) => {
      const base: { id: string; severity: string; reason?: string } = {
        id: m.policy_id,
        severity: m.severity,
      };
      if (m.policy_id === SYSTEM_POLICY_ID && typeof m.reason === "string") {
        base.reason = m.reason;
      }
      return base;
    });
  },
}));
// `../../adapter-loader/declarative-route` is imported directly in Group A to
// exercise `enrichEnvelopeAssets`, but the orchestrator (loaded inside
// Group B+C) needs `tryDeclarativeRoute` mocked. We use `importOriginal` to
// preserve the real `enrichEnvelopeAssets` while overriding the route entry.
vi.mock("../../adapter-loader/declarative-route", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("../../adapter-loader/declarative-route")>();
  return {
    ...actual,
    tryDeclarativeRoute: orchestratorMocks.tryDeclarativeRoute,
  };
});
// `webextension-polyfill` was mocked above with the enrichment-tier stub.
// Re-stub with the richer orchestrator surface (storage + windows +
// runtime) via `vi.doMock` so the orchestrator module sees the full API.
vi.doMock("webextension-polyfill", () => ({
  default: orchestratorMocks.browser,
}));

const OWNER = "0x1111111111111111111111111111111111111111";
const ROUTER = "0x7a250d5630b4cf539739df2c5dacb4c659f2488d";

function txMessage(requestId = "edge-tx-1"): Message {
  return {
    requestId,
    data: {
      type: RequestType.TRANSACTION,
      chainId: 1,
      hostname: "app.example",
      transaction: {
        from: OWNER,
        to: ROUTER,
        value: "0x0",
        data: "0x38ed1739",
      },
    },
  } as Message;
}

// Plan §M10 (2026-05-28) — v1 declarative-route + Cedar verdict 분기가
// orchestrator 에서 cutover. 본 describe 의 5 test 는 v1 envelope → Cedar
// verdict path 를 검증하던 것. v3 ActionBody → Cedar schema 매핑은 별 plan.
describe.skip("verdictSource audit + default policy Cedar verdict", () => {
  let decideMessage: typeof import("../../orchestrator").decideMessage;

  beforeEach(async () => {
    vi.useRealTimers();
    vi.clearAllMocks();
    orchestratorMocks.sessionStore.clear();
    orchestratorMocks.localStore.clear();
    orchestratorMocks.runtimeMessageListeners.length = 0;
    orchestratorMocks.windowRemovedListeners.length = 0;

    orchestratorMocks.evaluateWithPolicyRpc.mockResolvedValue({
      verdict: { kind: "pass" },
      audit: {
        request_id: "edge-tx-1",
        manifest_set_hash: "sha256:manifest",
        schema_hash: "sha256:schema",
        call_ids: [],
        methods: [],
      },
    });
    orchestratorMocks.tryDeclarativeRoute.mockResolvedValue({
      kind: "miss",
      reason: "no_selector",
    });
    orchestratorMocks.evaluateWithEnvelopes.mockResolvedValue({
      kind: "pass",
    });
    // Re-import via dynamic require so the freshly applied `vi.doMock`
    // graph for `webextension-polyfill` / `wasm-bridge` is honoured. (Tests
    // in Group A use the eagerly-applied `vi.mock` factories; Groups B+C
    // need the orchestrator-shaped surface.)
    vi.resetModules();
    ({ decideMessage } = await import("../../orchestrator"));
  });

  it("verdictSource_audit_declarative_path_logged", async () => {
    // Declarative-route hit with one envelope → orchestrator runs
    // `evaluateWithEnvelopes` and tags the audit row `verdictSource: "declarative"`.
    orchestratorMocks.tryDeclarativeRoute.mockResolvedValueOnce({
      kind: "hit",
      value: {
        envelopes: [makeSwapEnvelope({})],
        decoderId: "declarative.uniswap/v2/swapExactTokensForTokens",
        bundleId: "uniswap/v2/swapExactTokensForTokens@1.0.0",
        source: "layer1",
      },
    });
    orchestratorMocks.evaluateWithEnvelopes.mockResolvedValueOnce({
      kind: "pass",
    });

    const result = await decideMessage(txMessage("verdict-decl-1"));
    expect(result.ok).toBe(true);
    expect(orchestratorMocks.evaluateWithEnvelopes).toHaveBeenCalledOnce();
    expect(orchestratorMocks.evaluateWithPolicyRpc).not.toHaveBeenCalled();

    expect(orchestratorMocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({
        verdict: "pass",
        verdictSource: "declarative",
        declarative: expect.objectContaining({
          outcome: "hit",
          source: "layer1",
          decoder_id: "declarative.uniswap/v2/swapExactTokensForTokens",
          envelope_count: 1,
        }),
      }),
    );
  });

  it("verdictSource_audit_static_path_fallback_logged", async () => {
    // Declarative miss → static path produces the verdict. Audit log MUST
    // tag the row `verdictSource: "static"` AND retain the miss reason for
    // post-hoc analysis.
    orchestratorMocks.tryDeclarativeRoute.mockResolvedValueOnce({
      kind: "miss",
      reason: "no_publisher",
    });

    const result = await decideMessage(txMessage("verdict-static-1"));
    expect(result.ok).toBe(true);
    expect(orchestratorMocks.evaluateWithEnvelopes).not.toHaveBeenCalled();
    expect(orchestratorMocks.evaluateWithPolicyRpc).toHaveBeenCalledOnce();

    expect(orchestratorMocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({
        verdict: "pass",
        verdictSource: "static",
        declarative: { outcome: "miss", reason: "no_publisher" },
      }),
    );
  });

  it("default_policy_evaluates_v2_swap_with_pass_verdict", async () => {
    // Happy path: V2 swap envelope with both tokens enriched (WETH, USDC)
    // and a non-zero recipient. The default 10-policy bundle should let
    // this pass — we simulate Cedar's behaviour by having
    // `evaluateWithEnvelopes` return `{ kind: "pass" }`. The assertion is
    // that the orchestrator threads the verdict through unchanged and
    // tags `verdictSource: "declarative"`.
    const enrichedSwap = {
      category: "dex",
      action: "swap",
      fields: {
        swapMode: "exact_in",
        inputToken: {
          asset: {
            kind: "erc20",
            address: WETH_MAINNET,
            symbol: "WETH",
            decimals: 18,
          },
          amount: { kind: "exact", value: "1000000000000000000" },
        },
        outputToken: {
          asset: {
            kind: "erc20",
            address: USDC_MAINNET,
            symbol: "USDC",
            decimals: 6,
          },
          amount: { kind: "min", value: "1900000" },
        },
        recipient: "0x2222222222222222222222222222222222222222",
      },
    };
    orchestratorMocks.tryDeclarativeRoute.mockResolvedValueOnce({
      kind: "hit",
      value: {
        envelopes: [enrichedSwap],
        decoderId: "declarative.uniswap/v2/swapExactTokensForTokens",
        bundleId: "uniswap/v2/swapExactTokensForTokens@1.0.0",
        source: "layer1",
      },
    });
    orchestratorMocks.evaluateWithEnvelopes.mockResolvedValueOnce({
      kind: "pass",
    });

    const result = await decideMessage(txMessage("verdict-v2-pass"));
    expect(result.verdict.kind).toBe("pass");
    expect(result.ok).toBe(true);
    // The envelope reached evaluateWithEnvelopes intact.
    const evalArgs = orchestratorMocks.evaluateWithEnvelopes.mock
      .calls[0][0] as { envelopes: Record<string, unknown>[] };
    expect(evalArgs.envelopes).toEqual([enrichedSwap]);
    expect(orchestratorMocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({
        verdict: "pass",
        verdictSource: "declarative",
      }),
    );
  });

  it("default_policy_evaluates_v2_swap_with_zero_recipient_denies", async () => {
    // V2 swap envelope with `recipient = 0x0..0`. The default
    // `forbid-zero-recipient` policy fires deny. We simulate Cedar's
    // verdict by returning `{ kind: "fail", matched: [...] }` from the
    // mock; the assertion verifies the orchestrator surfaces it AND tags
    // the audit row `verdictSource: "declarative"` (the verdict came from
    // the declarative path, NOT static fallback).
    const swap = {
      category: "dex",
      action: "swap",
      fields: {
        swapMode: "exact_in",
        inputToken: {
          asset: {
            kind: "erc20",
            address: WETH_MAINNET,
            symbol: "WETH",
            decimals: 18,
          },
          amount: { kind: "exact", value: "1000000000000000000" },
        },
        outputToken: {
          asset: {
            kind: "erc20",
            address: USDC_MAINNET,
            symbol: "USDC",
            decimals: 6,
          },
          amount: { kind: "min", value: "1900000" },
        },
        recipient: "0x0000000000000000000000000000000000000000",
      },
    };
    orchestratorMocks.tryDeclarativeRoute.mockResolvedValueOnce({
      kind: "hit",
      value: {
        envelopes: [swap],
        decoderId: "declarative.uniswap/v2/swapExactTokensForTokens",
        bundleId: "uniswap/v2/swapExactTokensForTokens@1.0.0",
        source: "layer1",
      },
    });
    orchestratorMocks.evaluateWithEnvelopes.mockResolvedValueOnce({
      kind: "fail",
      matched: [
        {
          policy_id: "default::swap/forbid-zero-recipient",
          reason: "Swap recipient must not be the zero address",
          severity: "deny",
          origin: "action",
        },
      ],
    });

    const result = await decideMessage(txMessage("verdict-v2-deny"));
    expect(result.verdict.kind).toBe("fail");
    expect(result.ok).toBe(false);
    expect(result.verdict.matched?.[0].policy_id).toBe(
      "default::swap/forbid-zero-recipient",
    );
    expect(orchestratorMocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({
        verdict: "fail",
        verdictSource: "declarative",
        matchedPolicies: expect.arrayContaining([
          expect.objectContaining({
            id: "default::swap/forbid-zero-recipient",
            severity: "deny",
          }),
        ]),
      }),
    );
  });

  it("default_policy_evaluates_max_input_usd_3_denies_if_oracle_resolves", async () => {
    // PoC limitation: `max-input-usd-3` policy declares a host:oracle
    // requirement that the declarative verdict path (Phase 7F MVP) does
    // NOT wire — `rpc_response.results: []` per orchestrator.ts:370-373.
    // The WASM falls closed via `__engine::projection_failed`. We simulate
    // that semantics by having `evaluateWithEnvelopes` return a synthetic
    // engine-error verdict (the same shape `parseVerdict` produces for the
    // closed-loop projection failure path).
    //
    // Acceptance: orchestrator surfaces `verdict.kind === "fail"`, audit
    // row records `verdictSource: "declarative"` (the failure happened on
    // the declarative branch — the policy did fire, but on a synthetic
    // engine-error policy id rather than the user's `max-input-usd-3`).
    const swap = {
      category: "dex",
      action: "swap",
      fields: {
        swapMode: "exact_in",
        inputToken: {
          asset: {
            kind: "erc20",
            address: WETH_MAINNET,
            symbol: "WETH",
            decimals: 18,
          },
          // 10 WETH ≈ way more than $3 → would trigger max-input-usd-3
          // if the oracle resolved. The PoC's intentional limitation is
          // that the oracle does NOT resolve on the declarative path.
          amount: { kind: "exact", value: "10000000000000000000" },
        },
        outputToken: {
          asset: {
            kind: "erc20",
            address: USDC_MAINNET,
            symbol: "USDC",
            decimals: 6,
          },
          amount: { kind: "min", value: "1900000" },
        },
        recipient: "0x2222222222222222222222222222222222222222",
      },
    };
    orchestratorMocks.tryDeclarativeRoute.mockResolvedValueOnce({
      kind: "hit",
      value: {
        envelopes: [swap],
        decoderId: "declarative.uniswap/v2/swapExactTokensForTokens",
        bundleId: "uniswap/v2/swapExactTokensForTokens@1.0.0",
        source: "layer1",
      },
    });
    // Cedar engine fail-closed: projection_failed synthetic verdict.
    orchestratorMocks.evaluateWithEnvelopes.mockResolvedValueOnce({
      kind: "fail",
      matched: [
        {
          policy_id: "__engine::projection_failed",
          reason:
            "Manifest 'default::swap/max-input-usd-3' requires RPC data not yet wired through declarative branch",
          severity: "deny",
          origin: "engine_error",
        },
      ],
    });

    const result = await decideMessage(txMessage("verdict-oracle-fail"));
    expect(result.verdict.kind).toBe("fail");
    expect(result.ok).toBe(false);
    expect(result.verdict.matched?.[0].policy_id).toBe(
      "__engine::projection_failed",
    );
    expect(result.verdict.matched?.[0].origin).toBe("engine_error");
    expect(orchestratorMocks.auditAppend).toHaveBeenCalledWith(
      expect.objectContaining({
        verdict: "fail",
        verdictSource: "declarative",
      }),
    );
  });
});
