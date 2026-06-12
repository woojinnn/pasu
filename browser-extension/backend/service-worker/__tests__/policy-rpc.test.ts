import { beforeEach, describe, expect, it, vi } from "vitest";

import { dispatchCallsV2, formatAuditMatched } from "../policy-rpc";
import type { PlannedCallV2Dto } from "../wasm-bridge.types";

// dambi-auth is imported LAZILY inside the authenticated dispatch path only —
// these factories intercept the dynamic imports. Existing signed-out tests
// (ctx omitted) never reach `hasServerSession`, so the mocks are inert there.
const authMocks = vi.hoisted(() => ({
  getAccessToken: vi.fn<() => Promise<string | null>>(async () => "jwt"),
  serverEvaluate: vi.fn<
    (req: unknown) => Promise<{ policyRequest: Record<string, unknown> }>
  >(async () => ({ policyRequest: { results: {} } })),
}));
vi.mock("../dambi-auth/tokenStore", () => ({
  getAccessToken: authMocks.getAccessToken,
}));
vi.mock("../dambi-auth/client", () => ({
  evaluate: authMocks.serverEvaluate,
}));

describe("formatAuditMatched", () => {
  // D9 surfacing: when WASM returns a `Verdict::Fail` whose first
  // matched entry has `policy_id == "__system__"`, the audit-log
  // matched-policies list must carry that id verbatim (not remap it to
  // `__engine::projection_failed` or strip it). The dashboard reads
  // this list to render the system-failure verdict as a first-class
  // event.
  it("formatAuditMatched preserves __system__ policy id + reason for D9 verdicts", () => {
    const verdict = {
      kind: "fail" as const,
      matched: [
        {
          policy_id: "__system__",
          reason:
            "rpc-unavailable: user/max-input-usd-100::0::swap-total-input-usd",
          severity: "deny" as const,
          origin: "action" as const,
        },
      ],
    };
    const matched = formatAuditMatched(verdict);
    expect(matched[0].id).toBe("__system__");
    expect(matched[0].severity).toBe("deny");
    expect(matched[0].reason).toMatch(/^rpc-unavailable:/);
  });

  it("formatAuditMatched preserves __engine::* reason so the audit page can show the underlying cause", () => {
    const verdict = {
      kind: "fail" as const,
      matched: [
        {
          policy_id: "__engine::policy",
          reason: "context attribute `inputAmountNano` is missing",
          severity: "deny" as const,
          origin: "engine_error" as const,
        },
      ],
    };
    const matched = formatAuditMatched(verdict);
    expect(matched[0].id).toBe("__engine::policy");
    expect(matched[0].reason).toBe(
      "context attribute `inputAmountNano` is missing",
    );
  });

  it("formatAuditMatched omits reason for ordinary policy matches", () => {
    const verdict = {
      kind: "fail" as const,
      matched: [
        {
          policy_id: "bundle::max-input-usd-100",
          reason: "too much USD",
          severity: "deny" as const,
          origin: "action" as const,
        },
      ],
    };
    const matched = formatAuditMatched(verdict);
    expect(matched[0].id).toBe("bundle::max-input-usd-100");
    // Ordinary verdicts drop `reason` to keep the audit-log payload
    // small. The dashboard already has the policy id; it can pull the
    // reason on demand from the catalog.
    expect("reason" in matched[0]).toBe(false);
  });

  it("formatAuditMatched returns [] for pass verdicts", () => {
    expect(formatAuditMatched({ kind: "pass" as const })).toEqual([]);
  });
});

// ── Phase 1 / P2 — v2 (ActionBody) dispatch ───────────────────────────────
describe("dispatchCallsV2", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("fetch", vi.fn());
  });

  function plannedRemote(optional = false): PlannedCallV2Dto {
    return {
      manifest_id: "large-swap-usd-warning",
      call_id: "large-swap-usd-warning::total-input-usd",
      method: "oracle.usd_value",
      params: { chain_id: "eip155:1" },
      outputs: [],
      optional,
    };
  }

  it("returns an empty map for an empty plan (no HTTP)", async () => {
    const results = await dispatchCallsV2([], "http://127.0.0.1:8787");
    expect(results).toEqual({});
    expect(fetch).not.toHaveBeenCalled();
  });

  it("posts remote calls keyed by call_id and folds ok results to unwrapped values", async () => {
    const call = plannedRemote();
    vi.mocked(fetch).mockResolvedValue({
      ok: true,
      json: async () => ({
        request_id: `action-v2:${call.call_id}`,
        results: [
          { id: call.call_id, ok: true, result: { usd: "3500.1200" } },
        ],
      }),
    } as Response);

    const results = await dispatchCallsV2([call], "http://127.0.0.1:8787");

    // The POST body keys the call by `call_id` and hits the same /v1/rpc path.
    expect(fetch).toHaveBeenCalledTimes(1);
    const fetchCall = vi.mocked(fetch).mock.calls[0];
    expect(fetchCall[0]).toBe("http://127.0.0.1:8787/v1/rpc");
    const body = JSON.parse(String((fetchCall[1] as RequestInit).body));
    expect(body.calls).toEqual([
      {
        id: "large-swap-usd-warning::total-input-usd",
        method: "oracle.usd_value",
        params: { chain_id: "eip155:1" },
      },
    ]);
    // The map carries the UNWRAPPED `$.result` payload (not the envelope).
    expect(results).toEqual({
      "large-swap-usd-warning::total-input-usd": { usd: "3500.1200" },
    });
  });

  it("handles token.normalize_to_nano locally and never hits the network", async () => {
    const local: PlannedCallV2Dto = {
      manifest_id: "m",
      call_id: "m::nano",
      method: "token.normalize_to_nano",
      params: { amount: "1000000000000000000", decimals: 18 },
      outputs: [],
      optional: false,
    };
    const results = await dispatchCallsV2([local], "http://127.0.0.1:8787");
    expect(fetch).not.toHaveBeenCalled();
    expect(results).toEqual({ "m::nano": { nano: 1_000_000_000 } });
  });

  it("OMITS failed remote results (fail-closed; no error stub)", async () => {
    const call = plannedRemote();
    vi.mocked(fetch).mockResolvedValue({
      ok: true,
      json: async () => ({
        request_id: `action-v2:${call.call_id}`,
        results: [
          {
            id: call.call_id,
            ok: false,
            error: { code: "invalid_params", message: "bad asset" },
          },
        ],
      }),
    } as Response);

    const results = await dispatchCallsV2([call], "http://127.0.0.1:8787");
    // A failed call is dropped, NOT synthesised into an error stub — the
    // missing required result lets WASM fail closed (`__system__`).
    expect(results).toEqual({});
  });

  it("OMITS all remote calls when the daemon is unreachable (fail-closed)", async () => {
    const call = plannedRemote();
    vi.mocked(fetch).mockRejectedValue(new Error("ECONNREFUSED"));

    const results = await dispatchCallsV2([call], "http://127.0.0.1:8787");
    // No `rpc_unreachable` error stub (the v1 fail-OPEN behaviour); the map
    // is empty so a required call fails closed downstream.
    expect(results).toEqual({});
  });

  it("merges local + remote results into one map", async () => {
    const local: PlannedCallV2Dto = {
      manifest_id: "m",
      call_id: "m::nano",
      method: "token.normalize_to_nano",
      params: { amount: "1000000", decimals: 6 },
      outputs: [],
      optional: false,
    };
    const remote = plannedRemote();
    vi.mocked(fetch).mockResolvedValue({
      ok: true,
      json: async () => ({
        request_id: `action-v2:${remote.call_id}`,
        results: [{ id: remote.call_id, ok: true, result: { usd: "12.00" } }],
      }),
    } as Response);

    const results = await dispatchCallsV2(
      [local, remote],
      "http://127.0.0.1:8787",
    );

    // Only the remote call is POSTed; the local one is computed in-process.
    const body = JSON.parse(
      String((vi.mocked(fetch).mock.calls[0][1] as RequestInit).body),
    );
    expect(body.calls.map((c: { id: string }) => c.id)).toEqual([
      "large-swap-usd-warning::total-input-usd",
    ]);
    expect(results).toEqual({
      "m::nano": { nano: 1_000_000_000 },
      "large-swap-usd-warning::total-input-usd": { usd: "12.00" },
    });
  });

  // ── SW prereq — authenticated /evaluate path: wallet_id identity ─────────
  // The server loads wallet state by `wallet_id.address`. A venue order's
  // `tx.from` is the HL submitter SENTINEL, so the resolved master must be
  // able to override it — otherwise every HL server-state method reads an
  // empty wallet and stays dormant.
  describe("authenticated /evaluate wallet identity", () => {
    const ctxBase = {
      action: { domain: "perp", action: "place_order" },
      meta: { submitter: "0x000000000000000000000000000000000000a01c" },
      tx: {
        chain_id: "hl-mainnet",
        from: "0x000000000000000000000000000000000000a01c",
        to: "0x0000000000000000000000000000000000000000",
      },
    } as const;
    const MASTER = "0x676fa5b94067c2be14bc025df6c5c80dedf49a54";

    it("sends ctx.walletAddress (resolved venue master) as wallet_id.address", async () => {
      const call = plannedRemote();
      authMocks.serverEvaluate.mockResolvedValueOnce({
        policyRequest: {
          results: { [call.call_id]: { dayDrawdownBps: 500 } },
        },
      });

      const results = await dispatchCallsV2([call], "http://127.0.0.1:8787", {
        ...ctxBase,
        walletAddress: MASTER,
      });

      expect(authMocks.serverEvaluate).toHaveBeenCalledTimes(1);
      const req = authMocks.serverEvaluate.mock.calls[0][0] as {
        wallet_id: { address: string; chains: readonly string[] };
      };
      expect(req.wallet_id.address).toBe(MASTER);
      expect(req.wallet_id.chains).toEqual(["hl-mainnet"]);
      // Signed-in → served via /evaluate, never the /v1/rpc fallback.
      expect(fetch).not.toHaveBeenCalled();
      expect(results).toEqual({ [call.call_id]: { dayDrawdownBps: 500 } });
    });

    it("falls back to ctx.tx.from when walletAddress is absent", async () => {
      const call = plannedRemote();
      authMocks.serverEvaluate.mockResolvedValueOnce({
        policyRequest: { results: {} },
      });

      await dispatchCallsV2([call], "http://127.0.0.1:8787", ctxBase);

      const req = authMocks.serverEvaluate.mock.calls[0][0] as {
        wallet_id: { address: string };
      };
      expect(req.wallet_id.address).toBe(ctxBase.tx.from);
    });
  });
});
