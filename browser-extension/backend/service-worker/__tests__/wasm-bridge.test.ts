import { beforeEach, describe, expect, it, vi } from "vitest";
import { parseVerdict, WasmDecodeError } from "../wasm-bridge.types";
import {
  EngineError,
  installPolicies,
  evaluatePolicyRpc,
  planPolicyRpc,
  routeRequest,
} from "../wasm-bridge";

const wasmMocks = vi.hoisted(() => ({
  init: vi.fn(async () => undefined),
  installPoliciesJson: vi.fn(),
  evaluatePolicyRpcJson: vi.fn(),
  planPolicyRpcJson: vi.fn(),
  routeRequestJson: vi.fn(),
}));

vi.mock("webextension-polyfill", () => ({
  default: {
    runtime: {
      getURL: vi.fn((path: string) => `chrome-extension://scopeball/${path}`),
    },
  },
}));

vi.mock("../../wasm/policy_engine_wasm", () => ({
  default: wasmMocks.init,
  install_policies_json: wasmMocks.installPoliciesJson,
  evaluate_policy_rpc_json: wasmMocks.evaluatePolicyRpcJson,
  plan_policy_rpc_json: wasmMocks.planPolicyRpcJson,
  route_request_json: wasmMocks.routeRequestJson,
}));

describe("wasm bridge parsers", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("parses and rejects verdict shapes", () => {
    const verdict = {
      kind: "fail",
      matched: [
        {
          policy_id: "policy::deny",
          reason: null,
          severity: "deny",
          origin: "tx",
        },
      ],
    };

    expect(parseVerdict(verdict)).toEqual(verdict);
    expect(() => parseVerdict({ wrong: "shape" })).toThrow(WasmDecodeError);
    expect(() =>
      parseVerdict({
        ...verdict,
        matched: [{ ...verdict.matched[0], severity: "info" }],
      }),
    ).toThrow(WasmDecodeError);
  });

  it("installPolicies returns null for the legacy null envelope", async () => {
    wasmMocks.installPoliciesJson.mockReturnValue(
      JSON.stringify({ ok: true, data: null }),
    );

    // Legacy (Vec-shaped) install path returns `data: null` — bridge
    // surfaces that as `null` so callers can detect they used the wrong
    // shape and missed `enrichedSchemaHash`.
    await expect(
      installPolicies({ schema_text: "schema", policy_set: [] }),
    ).resolves.toBeNull();
    expect(wasmMocks.installPoliciesJson).toHaveBeenCalledOnce();
  });

  it("installPolicies returns enrichedSchemaHash for the map-shape install", async () => {
    // Map-shape install path: WASM composes the enriched schema and the
    // success envelope carries `enrichedSchemaHash` + `addedCustomFields`.
    wasmMocks.installPoliciesJson.mockReturnValue(
      JSON.stringify({
        ok: true,
        data: {
          enrichedSchemaHash: "sha256:abc",
          addedCustomFields: { swap: [{ field: "totalInputUsd" }] },
        },
      }),
    );

    await expect(
      installPolicies({
        schema_text: "",
        policy_set: [],
        manifests: { swap: { id: "x", schema_version: 1, requires: [] } },
      }),
    ).resolves.toEqual({
      enrichedSchemaHash: "sha256:abc",
      addedCustomFields: { swap: [{ field: "totalInputUsd" }] },
    });
  });

  it("installPolicies surfaces engine errors as EngineError", async () => {
    wasmMocks.installPoliciesJson.mockReturnValue(
      JSON.stringify({
        ok: false,
        error: { kind: "install_failed", message: "boom" },
      }),
    );

    await expect(
      installPolicies({ schema_text: "schema", policy_set: [] }),
    ).rejects.toBeInstanceOf(EngineError);
  });

  it("routeRequest passes through the WASM envelope list", async () => {
    const envelopes = [
      { category: "dex", action: "swap", fields: { mode: "exact_in" } },
      { category: "misc", action: "permit", fields: { permitKind: "eip2612" } },
    ];
    wasmMocks.routeRequestJson.mockReturnValue(
      JSON.stringify({ ok: true, data: envelopes }),
    );

    const result = await routeRequest({
      method: "eth_sendTransaction",
      params: [],
      chain_id: 1,
    });

    expect(result).toEqual(envelopes);
    expect(wasmMocks.routeRequestJson).toHaveBeenCalledOnce();
  });

  it("routeRequest surfaces route_failed as an EngineError", async () => {
    wasmMocks.routeRequestJson.mockReturnValue(
      JSON.stringify({
        ok: false,
        error: { kind: "route_failed", message: "no adapter matched" },
      }),
    );

    await expect(
      routeRequest({ method: "eth_sendTransaction", params: [], chain_id: 1 }),
    ).rejects.toBeInstanceOf(EngineError);
  });

  it("routeRequest rejects malformed envelopes (missing category)", async () => {
    wasmMocks.routeRequestJson.mockReturnValue(
      JSON.stringify({
        ok: true,
        data: [{ action: "swap", fields: {} }],
      }),
    );

    await expect(
      routeRequest({ method: "eth_sendTransaction", params: [], chain_id: 1 }),
    ).rejects.toBeInstanceOf(EngineError);
  });

  it("planPolicyRpc unwraps the WASM plan envelope", async () => {
    const plan = {
      request_id: "eval-1",
      root: {
        chain_id: 1,
        from: "0x1111111111111111111111111111111111111111",
        to: "0x2222222222222222222222222222222222222222",
        value_wei: "0",
        block_timestamp: 1_700_000_000,
      },
      envelopes: [],
      calls: [{ id: "call-1", method: "oracle.usd_value", params: {} }],
      manifest_set_hash: "sha256:1",
      schema_hash: "sha256:2",
      diagnostics: [],
    };
    const input = {
      request_id: "eval-1",
      raw_request: {
        method: "eth_sendTransaction",
        params: [],
        chain_id: 1,
      },
      manifests: [],
    };
    wasmMocks.planPolicyRpcJson.mockReturnValue(
      JSON.stringify({ ok: true, data: plan }),
    );

    await expect(planPolicyRpc(input)).resolves.toEqual(plan);
    expect(wasmMocks.planPolicyRpcJson).toHaveBeenCalledWith(
      JSON.stringify(input),
    );
  });

  it("evaluatePolicyRpc unwraps and parses the WASM verdict", async () => {
    const verdict = {
      kind: "pass",
    };
    const input = {
      plan: {
        request_id: "eval-1",
        root: {
          chain_id: 1,
          from: "0x1111111111111111111111111111111111111111",
          to: "0x2222222222222222222222222222222222222222",
          value_wei: "0",
        },
        envelopes: [],
        calls: [],
        manifest_set_hash: "sha256:1",
        schema_hash: "sha256:2",
        diagnostics: [],
      },
      rpc_response: { request_id: "eval-1", results: [] },
      manifests: [],
    };
    wasmMocks.evaluatePolicyRpcJson.mockReturnValue(
      JSON.stringify({ ok: true, data: verdict }),
    );

    await expect(evaluatePolicyRpc(input)).resolves.toEqual(verdict);
    expect(wasmMocks.evaluatePolicyRpcJson).toHaveBeenCalledWith(
      JSON.stringify(input),
    );
  });
});
