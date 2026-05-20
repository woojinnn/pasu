import { beforeEach, describe, expect, it, vi } from "vitest";
import { RequestType, type Message } from "@lib/types";

const mocks = vi.hoisted(() => ({
  planPolicyRpc: vi.fn(),
  evaluatePolicyRpc: vi.fn(),
}));

vi.mock("../wasm-bridge", () => ({
  planPolicyRpc: mocks.planPolicyRpc,
  evaluatePolicyRpc: mocks.evaluatePolicyRpc,
}));

import { evaluateWithPolicyRpc, formatAuditMatched } from "../policy-rpc";

function txMessage(): Message {
  return {
    requestId: "req-1",
    data: {
      type: RequestType.TRANSACTION,
      chainId: 1,
      hostname: "app.example",
      transaction: {
        from: "0x1111111111111111111111111111111111111111",
        to: "0x2222222222222222222222222222222222222222",
        value: "0x0",
        data: "0x1234",
      },
    },
  } as Message;
}

describe("policy-rpc coordinator", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("fetch", vi.fn());
  });

  it("posts planned calls to policy-rpc and evaluates with the response", async () => {
    const plan = {
      request_id: "req-1",
      root: {
        chain_id: 1,
        from: "0x1111111111111111111111111111111111111111",
        to: "0x2222222222222222222222222222222222222222",
        value_wei: "0",
      },
      envelopes: [],
      calls: [{ id: "call-1", method: "oracle.usd_value", params: {} }],
      manifest_set_hash: "sha256:manifest",
      schema_hash: "sha256:schema",
      diagnostics: [],
    };
    const rpcResponse = {
      request_id: "req-1",
      results: [{ id: "call-1", ok: true, result: { value: "1.00" } }],
    };
    const verdict = { kind: "pass" as const };
    mocks.planPolicyRpc.mockResolvedValue(plan);
    mocks.evaluatePolicyRpc.mockResolvedValue(verdict);
    vi.mocked(fetch).mockResolvedValue({
      ok: true,
      json: async () => rpcResponse,
    } as Response);

    const result = await evaluateWithPolicyRpc(txMessage(), {
      policyRpcUrl: "http://127.0.0.1:8787",
    });

    expect(fetch).toHaveBeenCalledWith(
      "http://127.0.0.1:8787/v1/rpc",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ request_id: "req-1", calls: plan.calls }),
      }),
    );
    expect(mocks.evaluatePolicyRpc).toHaveBeenCalledWith({
      plan,
      rpc_response: rpcResponse,
      manifests: [],
    });
    expect(result).toEqual({
      verdict,
      audit: {
        request_id: "req-1",
        manifest_set_hash: "sha256:manifest",
        schema_hash: "sha256:schema",
        call_ids: ["call-1"],
        methods: ["oracle.usd_value"],
      },
    });
  });

  it("skips HTTP when the plan has no calls", async () => {
    const plan = {
      request_id: "req-1",
      root: {
        chain_id: 1,
        from: "0x1111111111111111111111111111111111111111",
        to: "0x2222222222222222222222222222222222222222",
        value_wei: "0",
      },
      envelopes: [],
      calls: [],
      manifest_set_hash: "sha256:manifest",
      schema_hash: "sha256:schema",
      diagnostics: [],
    };
    mocks.planPolicyRpc.mockResolvedValue(plan);
    mocks.evaluatePolicyRpc.mockResolvedValue({ kind: "pass" });

    await evaluateWithPolicyRpc(txMessage());

    expect(fetch).not.toHaveBeenCalled();
    expect(mocks.evaluatePolicyRpc).toHaveBeenCalledWith({
      plan,
      rpc_response: { request_id: "req-1", results: [] },
      manifests: [],
    });
  });

  it("passes per-call RPC failures back into WASM evaluation", async () => {
    const plan = {
      request_id: "req-1",
      root: {
        chain_id: 1,
        from: "0x1111111111111111111111111111111111111111",
        to: "0x2222222222222222222222222222222222222222",
        value_wei: "0",
      },
      envelopes: [],
      calls: [{ id: "call-1", method: "oracle.usd_value", params: {} }],
      manifest_set_hash: "sha256:manifest",
      schema_hash: "sha256:schema",
      diagnostics: [],
    };
    const rpcResponse = {
      request_id: "req-1",
      results: [
        {
          id: "call-1",
          ok: false,
          error: { code: "invalid_params", message: "bad asset" },
        },
      ],
    };
    const verdict = {
      kind: "fail" as const,
      matched: [
        {
          policy_id: "__engine::projection_failed",
          reason: "__engine::projection_failed",
          severity: "deny" as const,
          origin: "engine_error" as const,
        },
      ],
    };
    mocks.planPolicyRpc.mockResolvedValue(plan);
    mocks.evaluatePolicyRpc.mockResolvedValue(verdict);
    vi.mocked(fetch).mockResolvedValue({
      ok: true,
      json: async () => rpcResponse,
    } as Response);

    const result = await evaluateWithPolicyRpc(txMessage());

    expect(mocks.evaluatePolicyRpc).toHaveBeenCalledWith({
      plan,
      rpc_response: rpcResponse,
      manifests: [],
    });
    expect(result.verdict).toEqual(verdict);
  });

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

  it("rejects malformed RPC responses before WASM evaluation", async () => {
    const plan = {
      request_id: "req-1",
      root: {
        chain_id: 1,
        from: "0x1111111111111111111111111111111111111111",
        to: "0x2222222222222222222222222222222222222222",
        value_wei: "0",
      },
      envelopes: [],
      calls: [{ id: "call-1", method: "oracle.usd_value", params: {} }],
      manifest_set_hash: "sha256:manifest",
      schema_hash: "sha256:schema",
      diagnostics: [],
    };
    mocks.planPolicyRpc.mockResolvedValue(plan);
    vi.mocked(fetch).mockResolvedValue({
      ok: true,
      json: async () => ({ request_id: "different", results: [] }),
    } as Response);

    await expect(evaluateWithPolicyRpc(txMessage())).rejects.toThrow(
      "policy-rpc returned malformed response",
    );
    expect(mocks.evaluatePolicyRpc).not.toHaveBeenCalled();
  });
});
