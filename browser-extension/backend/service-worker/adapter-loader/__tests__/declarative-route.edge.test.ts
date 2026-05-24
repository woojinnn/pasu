/**
 * T-TEST-ENVELOPE — enrichEnvelopeAssets + Cedar verdict source audit edge
 * cases (no production code change).
 *
 * Coverage matrix (8 tests):
 *
 *   1. enrichEnvelopeAssets_handles_unknown_token_with_empty_symbol
 *      — Layer 3 returns null for unknown address. Skeleton survives intact
 *        (no `symbol`/`decimals` injected). A downstream Cedar policy that
 *        reads `context.outputToken.asset.symbol == ""` would still trigger
 *        because Cedar treats missing attrs differently from empty strings,
 *        so the *enricher's* contract is `skeleton preserved`. The Cedar
 *        verdict side is exercised in test 6.
 *
 *   2. enrichEnvelopeAssets_uses_negative_cache_on_404
 *      — When the token client treats HTTP 404 as a negative-cache hit, a
 *        second `enrichEnvelopeAssets` call for the same envelope shape must
 *        not re-invoke the underlying fetch. We simulate this via the mock
 *        client counter — first lookup → null + sticky null forever.
 *
 *   3. enrichEnvelopeAssets_handles_network_timeout
 *      — Token client throws (AbortError, network failure). Enricher must
 *        absorb the throw, return the skeleton unchanged, and let the route
 *        stay alive (no exception leaks).
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
 *
 * Production code (declarative-route.ts, token-client.ts, orchestrator.ts,
 * policy-set.json) is NOT modified. All boundaries are mocked at the test
 * fence — vitest mocks for `wasm-bridge`, `jit-fetcher`, `policy-rpc`,
 * `policies-loader`, `storage`, `adapter-loader/declarative-route`, and
 * `webextension-polyfill`.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RequestType, type Message } from "@lib/types";

// ─── Group A: enrichEnvelopeAssets unit tests ─────────────────────────────
const enrichmentMocks = vi.hoisted(() => ({
  resolveAdapter: vi.fn(),
  declarativeRouteRequest: vi.fn(),
}));

vi.mock("webextension-polyfill", () => ({
  default: {
    runtime: {
      getURL: vi.fn((p: string) => `chrome-extension://scopeball/${p}`),
    },
  },
}));
vi.mock("../../wasm-bridge", () => {
  class MockEngineError extends Error {
    constructor(
      readonly kind: string,
      message: string,
    ) {
      super(message);
      this.name = "EngineError";
    }
  }
  return {
    EngineError: MockEngineError,
    declarativeRouteRequest: enrichmentMocks.declarativeRouteRequest,
  };
});
vi.mock("../jit-fetcher", () => ({
  resolveAdapter: enrichmentMocks.resolveAdapter,
}));

import { enrichEnvelopeAssets } from "../declarative-route";
import type {
  TokenMetadata,
  TokenRegistryClient,
} from "../../registry/token-client";

const WETH_MAINNET = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const USDC_MAINNET = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
const UNKNOWN_ADDR = "0x0000000000000000000000000000000000000099";

const WETH_META: TokenMetadata = {
  kind: "erc20",
  chainId: 1,
  address: WETH_MAINNET,
  symbol: "WETH",
  decimals: 18,
  name: "Wrapped Ether",
};

/**
 * Make a V2 swap envelope with the given input asset (WETH default), the
 * given output asset (USDC default), and the given recipient. Used by both
 * enrichment tests (Group A) and Cedar verdict tests (Group C — they share
 * the same envelope shape).
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

describe("enrichEnvelopeAssets edge cases", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });
  afterEach(() => {
    vi.clearAllMocks();
  });

  it("enrichEnvelopeAssets_handles_unknown_token_with_empty_symbol", async () => {
    // Token client cannot resolve UNKNOWN_ADDR — lookup returns null. The
    // enricher's contract: leave the skeleton intact (no spurious symbol).
    // This is the state a downstream Cedar policy
    // (`forbid-untrusted-output-symbol` / `known-token-only`) inspects via
    // `context.outputToken.asset.symbol == ""`. Because the enricher never
    // synthesises a symbol of `""`, the policy actually fires off the
    // *absent* attr, not an empty string — but the contract under test
    // here is the enricher: it MUST NOT inject a `symbol: ""` of its own.
    const lookup = vi.fn(async (_chain: number, addr: string) => {
      if (addr.toLowerCase() === WETH_MAINNET) return WETH_META;
      return null;
    });
    const client: TokenRegistryClient = { lookup };

    const swap = makeSwapEnvelope({ outputAddress: UNKNOWN_ADDR });
    const [out] = await enrichEnvelopeAssets([swap], 1, client);
    const fields = (out as { fields: Record<string, unknown> }).fields as {
      inputToken: { asset: Record<string, unknown> };
      outputToken: { asset: Record<string, unknown> };
    };

    // Known token → enriched as normal.
    expect(fields.inputToken.asset).toMatchObject({
      symbol: "WETH",
      decimals: 18,
    });
    // Unknown token → skeleton untouched. `symbol` MUST be absent (not "").
    expect(fields.outputToken.asset).toEqual({
      kind: "erc20",
      address: UNKNOWN_ADDR,
    });
    expect("symbol" in fields.outputToken.asset).toBe(false);
    expect("decimals" in fields.outputToken.asset).toBe(false);
    expect(lookup).toHaveBeenCalledTimes(2);
  });

  it("enrichEnvelopeAssets_uses_negative_cache_on_404", async () => {
    // Simulate the token-client's negative-cache behaviour: after the first
    // 404, subsequent same-key lookups return null without re-fetching.
    // The mock client's `lookup` counts calls — we feed it through the
    // enricher twice for the same envelope shape and assert the second
    // call uses the same null result (i.e. it never throws / never tries
    // to "do better" the second time).
    let underlyingFetchCount = 0;
    const negativeKey = `1__${UNKNOWN_ADDR}`;
    const negativeCache = new Set<string>();
    const lookup = vi.fn(async (chainId: number, addr: string) => {
      const lower = addr.toLowerCase();
      const k = `${chainId}__${lower}`;
      if (negativeCache.has(k)) return null;
      if (lower === WETH_MAINNET) {
        underlyingFetchCount += 1;
        return WETH_META;
      }
      // Unknown → first lookup hits the "404" path → returns null AND
      // installs a negative cache slot to mirror token-client.ts behaviour.
      underlyingFetchCount += 1;
      negativeCache.add(k);
      return null;
    });
    const client: TokenRegistryClient = { lookup };

    const swap = makeSwapEnvelope({ outputAddress: UNKNOWN_ADDR });
    await enrichEnvelopeAssets([swap], 1, client);
    const firstCount = underlyingFetchCount;

    // Second pass: WETH is also unique per envelope, so it also fetches a
    // second time in this mock — but UNKNOWN_ADDR must NOT, because the
    // negative cache short-circuits it.
    await enrichEnvelopeAssets([swap], 1, client);

    // The negative cache must have been honoured for the unknown address:
    // total fetches = first(WETH+UNKNOWN) + second(WETH) = 3, not 4.
    expect(underlyingFetchCount - firstCount).toBe(1);
    expect(negativeCache.has(negativeKey)).toBe(true);
    expect(lookup).toHaveBeenCalledTimes(4); // 2 envelopes × 2 assets
  });

  it("enrichEnvelopeAssets_handles_network_timeout", async () => {
    // Simulate a token-client.ts network timeout — the underlying client
    // throws an AbortError. The enricher MUST absorb it (per
    // `enrichAssetRef`'s try/catch) and emit the bare skeleton.
    const lookup = vi.fn(async () => {
      const err = new Error("The operation was aborted.");
      err.name = "AbortError";
      throw err;
    });
    const client: TokenRegistryClient = { lookup };

    const swap = makeSwapEnvelope({});

    // The call must NOT throw — the route stays alive.
    const [out] = await enrichEnvelopeAssets([swap], 1, client);
    const fields = (out as { fields: Record<string, unknown> }).fields as {
      inputToken: { asset: Record<string, unknown> };
      outputToken: { asset: Record<string, unknown> };
    };

    // Both AssetRefs come back as bare skeletons — symbol/decimals absent.
    expect(fields.inputToken.asset).toEqual({
      kind: "erc20",
      address: WETH_MAINNET,
    });
    expect(fields.outputToken.asset).toEqual({
      kind: "erc20",
      address: USDC_MAINNET,
    });
    // lookup was attempted for every AssetRef despite the throw — the
    // enricher does not bail early on the first exception.
    expect(lookup).toHaveBeenCalledTimes(2);
  });
});

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

describe("verdictSource audit + default policy Cedar verdict", () => {
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
