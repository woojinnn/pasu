import { beforeEach, describe, expect, it, vi } from "vitest";
import { parseVerdict, WasmDecodeError } from "../wasm-bridge.types";
import {
  EngineError,
  evaluateActionV2,
  installPolicies,
  planActionRpcV2,
} from "../wasm-bridge";

const wasmMocks = vi.hoisted(() => ({
  init: vi.fn(async () => undefined),
  installPoliciesJson: vi.fn(),
  planActionRpcV2Json: vi.fn(),
  evaluateActionV2Json: vi.fn(),
}));

vi.mock("webextension-polyfill", () => ({
  default: {
    runtime: {
      getURL: vi.fn((path: string) => `chrome-extension://dambi/${path}`),
    },
  },
}));

vi.mock("../../wasm/policy_engine_wasm", () => ({
  default: wasmMocks.init,
  install_policies_json: wasmMocks.installPoliciesJson,
  plan_action_rpc_v2_json: wasmMocks.planActionRpcV2Json,
  evaluate_action_v2_json: wasmMocks.evaluateActionV2Json,
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

  it("planActionRpcV2 unwraps data.planned from the WASM envelope", async () => {
    const planned = [
      {
        manifest_id: "large-swap-usd-warning",
        call_id: "large-swap-usd-warning::total-input-usd",
        method: "oracle.usd_value",
        params: { chain_id: "eip155:42161" },
        outputs: [
          { kind: "context", field: "totalInputUsd", type: "Decimal", from: "$.result.usd" },
        ],
        optional: false,
      },
    ];
    const input = {
      manifests: [{ id: "large-swap-usd-warning", schema_version: 2 }],
      action: { amm: { swap: {} } },
      meta: { submitter: "0x1111111111111111111111111111111111111111" },
      tx: {
        chain_id: "eip155:42161",
        from: "0x1111111111111111111111111111111111111111",
        to: "0x2222222222222222222222222222222222222222",
      },
    };
    wasmMocks.planActionRpcV2Json.mockReturnValue(
      JSON.stringify({ ok: true, data: { planned } }),
    );

    await expect(planActionRpcV2(input)).resolves.toEqual(planned);
    expect(wasmMocks.planActionRpcV2Json).toHaveBeenCalledWith(
      JSON.stringify(input),
    );
  });

  it("planActionRpcV2 surfaces invalid_input_json as EngineError", async () => {
    wasmMocks.planActionRpcV2Json.mockReturnValue(
      JSON.stringify({
        ok: false,
        error: { kind: "invalid_input_json", message: "invalid input json: x" },
      }),
    );

    try {
      await planActionRpcV2({
        manifests: [],
        action: { unknown: {} },
        meta: {},
        tx: {
          chain_id: "eip155:1",
          from: "0x1111111111111111111111111111111111111111",
          to: "0x2222222222222222222222222222222222222222",
        },
      });
      expect.fail("expected throw");
    } catch (err) {
      expect(err).toBeInstanceOf(EngineError);
      expect((err as EngineError).kind).toBe("invalid_input_json");
    }
  });

  it("evaluateActionV2 unwraps and parses data.verdict (warn)", async () => {
    const verdict = {
      kind: "warn",
      matched: [
        {
          policy_id: "large-input",
          reason: "large USD input",
          severity: "warn",
          origin: "action",
        },
      ],
    };
    const input = {
      action: { amm: { swap: {} } },
      meta: { submitter: "0x1111111111111111111111111111111111111111" },
      tx: {
        chain_id: "eip155:42161",
        from: "0x1111111111111111111111111111111111111111",
        to: "0x2222222222222222222222222222222222222222",
      },
      bundles: [{ policy: "forbid(...);", manifest: { id: "x", schema_version: 2 } }],
      results: { "large-swap-usd-warning::total-input-usd": { usd: "3500.1200" } },
    };
    wasmMocks.evaluateActionV2Json.mockReturnValue(
      JSON.stringify({ ok: true, data: { verdict } }),
    );

    await expect(evaluateActionV2(input)).resolves.toEqual(verdict);
    expect(wasmMocks.evaluateActionV2Json).toHaveBeenCalledWith(
      JSON.stringify(input),
    );
  });

  it("evaluateActionV2 returns a parsed Fail verdict for an engine fault (never ok:false)", async () => {
    // The v2 evaluate export ALWAYS returns ok:true — a missing required RPC
    // result surfaces as a fail-closed `__system__` Fail inside the envelope,
    // NOT as an `ok:false` error. The wrapper parses it like any other verdict.
    const verdict = {
      kind: "fail",
      matched: [
        {
          policy_id: "__system__",
          reason: "required policy-rpc result missing",
          severity: "deny",
          // `system_fail_verdict` flows through `matched_to_dto`, which only
          // emits "action"/"tx" — "engine_error" is reserved for the
          // `__engine::*` path. Keep the mock faithful to the Rust.
          origin: "tx",
        },
      ],
    };
    wasmMocks.evaluateActionV2Json.mockReturnValue(
      JSON.stringify({ ok: true, data: { verdict } }),
    );

    await expect(
      evaluateActionV2({
        action: { amm: { swap: {} } },
        meta: {},
        tx: {
          chain_id: "eip155:42161",
          from: "0x1111111111111111111111111111111111111111",
          to: "0x2222222222222222222222222222222222222222",
        },
        bundles: [{ policy: "forbid(...);", manifest: { id: "x", schema_version: 2 } }],
        results: {},
      }),
    ).resolves.toEqual(verdict);
  });
});
