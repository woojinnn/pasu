import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  parseAction,
  parseTier1Plan,
  parseVerdict,
  parseWindowKeys,
  WasmDecodeError,
} from "../wasm-bridge.types";
import { tier1FactPlan } from "../wasm-bridge";

const wasmMocks = vi.hoisted(() => ({
  init: vi.fn(async () => undefined),
  installPoliciesJson: vi.fn(),
  buildActionJson: vi.fn(),
  tier1FactPlanJson: vi.fn(),
  tier2WindowKeysJson: vi.fn(),
  evaluateJson: vi.fn(),
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
  build_action_json: wasmMocks.buildActionJson,
  tier1_fact_plan_json: wasmMocks.tier1FactPlanJson,
  tier2_window_keys_json: wasmMocks.tier2WindowKeysJson,
  evaluate_json: wasmMocks.evaluateJson,
}));

const token = {
  chain_id: 1,
  address: "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
  symbol: "WETH",
  decimals: 18,
  is_native: false,
};

const dexAction = {
  dex: {
    actor: "0x1111111111111111111111111111111111111111",
    target: "0x2222222222222222222222222222222222222222",
    value_wei: "0",
    facts: {
      protocol_ids: ["uniswap_v3"],
      input_tokens: [token],
      output_tokens: [],
      total_input_usd: null,
      total_min_output_usd: null,
      max_fee_bps: null,
      has_zero_min_output: false,
      has_external_recipient: false,
      total_input_fraction_of_portfolio_bps: null,
      allowances_cover_inputs: null,
      window_stats: null,
    },
    oracle_requirements: [
      {
        kind: "input",
        token,
        raw_amount: "1000000000000000000",
      },
    ],
    trace: { steps: [] },
  },
};

describe("wasm bridge parsers", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("parses and rejects action shapes", () => {
    expect(parseAction(dexAction)).toEqual(dexAction);
    expect(() => parseAction({ kind: "dex" })).toThrow(WasmDecodeError);
    expect(() =>
      parseAction({ dex: { ...dexAction.dex, facts: { protocol_ids: [] } } }),
    ).toThrow(WasmDecodeError);
  });

  it("parses and rejects tier1 plan shapes", () => {
    const plan = {
      tokens_for_oracle: [token],
      balances: [{ owner: dexAction.dex.actor, token }],
      allowances: [{ owner: dexAction.dex.actor, token, spender: dexAction.dex.target }],
      clock_required: false,
      sig_oracle_requirements: [{ kind: "minOutput", token, raw_amount: "1" }],
    };

    expect(parseTier1Plan(plan)).toEqual(plan);
    expect(() => parseTier1Plan({ wrong: "shape" })).toThrow(WasmDecodeError);
    expect(() =>
      parseTier1Plan({ ...plan, sig_oracle_requirements: [{ kind: "output", token }] }),
    ).toThrow(WasmDecodeError);
  });

  it("parses and rejects window key shapes", () => {
    const windowKeys = {
      keys: [{ actor: dexAction.dex.actor, name: "swapVolumeUsd24h" }],
    };

    expect(parseWindowKeys(windowKeys)).toEqual(windowKeys);
    expect(() => parseWindowKeys({ wrong: "shape" })).toThrow(WasmDecodeError);
    expect(() => parseWindowKeys({ keys: [{ actor: dexAction.dex.actor }] })).toThrow(
      WasmDecodeError,
    );
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
      parseVerdict({ ...verdict, matched: [{ ...verdict.matched[0], severity: "info" }] }),
    ).toThrow(WasmDecodeError);
  });

  it("rejects malformed tier1 plans returned by the WASM export", async () => {
    wasmMocks.tier1FactPlanJson.mockReturnValue(
      JSON.stringify({ ok: true, data: { wrong: "shape" } }),
    );

    await expect(tier1FactPlan(dexAction)).rejects.toThrow(WasmDecodeError);
    expect(wasmMocks.tier1FactPlanJson).toHaveBeenCalledWith(
      JSON.stringify(dexAction),
    );
  });
});
