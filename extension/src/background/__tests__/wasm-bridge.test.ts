import { beforeEach, describe, expect, it, vi } from "vitest";
import { parseVerdict, WasmDecodeError } from "../wasm-bridge.types";
import {
  EngineError,
  evaluateEnvelope,
  installPolicies,
  routeRequest,
} from "../wasm-bridge";

const wasmMocks = vi.hoisted(() => ({
  init: vi.fn(async () => undefined),
  installPoliciesJson: vi.fn(),
  evaluateEnvelopeJson: vi.fn(),
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
  evaluate_envelope_json: wasmMocks.evaluateEnvelopeJson,
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

  it("installPolicies unwraps the WASM ok envelope", async () => {
    wasmMocks.installPoliciesJson.mockReturnValue(
      JSON.stringify({ ok: true, data: null }),
    );

    await expect(
      installPolicies({ schema_text: "schema", policy_set: [] }),
    ).resolves.toBeUndefined();
    expect(wasmMocks.installPoliciesJson).toHaveBeenCalledOnce();
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

  it("evaluateEnvelope passes through the WASM verdict envelope", async () => {
    const verdict = {
      kind: "warn",
      matched: [
        {
          policy_id: "policy::warn",
          reason: "watch",
          severity: "warn",
          origin: "action",
        },
      ],
    };
    wasmMocks.evaluateEnvelopeJson.mockReturnValue(
      JSON.stringify({ ok: true, data: verdict }),
    );

    const input = {
      envelope: {
        category: "dex",
        action: "swap",
        fields: { mode: "exact_in" },
      },
      from: "0x1111111111111111111111111111111111111111",
      to: "0x2222222222222222222222222222222222222222",
      value_wei: "0",
      chain_id: 1,
      block_timestamp: 1_700_000_000,
      host_snapshot: {},
    };

    const result = await evaluateEnvelope(input);

    expect(result).toEqual(verdict);
    expect(wasmMocks.evaluateEnvelopeJson).toHaveBeenCalledWith(
      JSON.stringify(input),
    );
  });

  it("evaluateEnvelope surfaces engine errors as EngineError", async () => {
    wasmMocks.evaluateEnvelopeJson.mockReturnValue(
      JSON.stringify({
        ok: false,
        error: { kind: "engine_failure", message: "x" },
      }),
    );

    await expect(
      evaluateEnvelope({
        envelope: { category: "dex", action: "swap", fields: {} },
        from: "0x1111111111111111111111111111111111111111",
        to: "0x2222222222222222222222222222222222222222",
        value_wei: "0",
        chain_id: 1,
        block_timestamp: 1_700_000_000,
        host_snapshot: {},
      }),
    ).rejects.toBeInstanceOf(EngineError);
  });
});
