import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import {
  BundleParseError,
  parseBundle,
  type AdapterFunctionBundle,
} from "../bundle-schema";

// Single fixture file shared with the Rust side
// (crates/adapters/mappers/tests/fixtures/uniswap-v2-swap-exact-tokens.json).
// Reading it as text keeps Rust and TS in lockstep — no copy-paste drift.
const FIXTURE_PATH = path.resolve(
  __dirname,
  "../../../../../crates/adapters/mappers/tests/fixtures/uniswap-v2-swap-exact-tokens.json",
);

function loadV2SwapFixture(): unknown {
  return JSON.parse(readFileSync(FIXTURE_PATH, "utf8"));
}

describe("parseBundle — V2 swap fixture", () => {
  it("parses the canonical §4.1 example", () => {
    const bundle = parseBundle(loadV2SwapFixture());

    expect(bundle.type).toBe("adapter_function");
    expect(bundle.id).toBe("uniswap/v2/swapExactTokensForTokens@1.0.0");
    expect(bundle.publisher).toBe("uniswap.eth");
    expect(bundle.match.chain_ids).toEqual([1, 8453, 10, 42161]);
    expect(bundle.match.selector).toBe("0x38ed1739");
    expect(bundle.abi_fragment.function_name).toBe("swapExactTokensForTokens");
  });

  it("matches the SingleEmit strategy with expected fields", () => {
    const bundle = parseBundle(loadV2SwapFixture());
    expect(bundle.emit.strategy).toBe("single_emit");

    if (bundle.emit.strategy !== "single_emit") return;
    expect(bundle.emit.category).toBe("dex");
    expect(bundle.emit.action).toBe("swap");

    const fields = bundle.emit.fields;

    // Literal field
    expect(fields["inputToken.asset.kind"]).toEqual({ literal: "erc20" });

    // FromArg field
    expect(fields["inputToken.amount.value"]).toEqual({
      from: "$.args.amountIn",
    });

    // Transform field (select_address w/ nested args)
    expect(fields["inputToken.asset.address"]).toEqual({
      fn: "select_address",
      args: [{ from: "$.args.path" }, { literal: 0 }],
    });

    // outputToken uses -1 index (last element of path)
    expect(fields["outputToken.asset.address"]).toEqual({
      fn: "select_address",
      args: [{ from: "$.args.path" }, { literal: -1 }],
    });
  });

  it("requires.host_capabilities surfaces token_metadata", () => {
    const bundle = parseBundle(loadV2SwapFixture());
    expect(bundle.requires.imperative).toEqual([]);
    expect(bundle.requires.host_capabilities).toEqual(["host:token_metadata"]);
    expect(bundle.requires.extension).toBe(">=0.1.0");
  });
});

describe("parseBundle — all 4 strategies parse", () => {
  const baseShell = (emit: unknown) => ({
    type: "adapter_function",
    id: "demo/x@1.0.0",
    publisher: "demo.eth",
    match: {
      chain_ids: [1],
      to: ["0x0000000000000000000000000000000000000001"],
      selector: "0xdeadbeef",
    },
    abi_fragment: { function_name: "x", abi: {} },
    emit,
    requires: { imperative: [], host_capabilities: [], extension: ">=0.1.0" },
  });

  it("single_emit", () => {
    const b: AdapterFunctionBundle = parseBundle(
      baseShell({
        strategy: "single_emit",
        category: "dex",
        action: "swap",
        fields: {},
      }),
    );
    expect(b.emit.strategy).toBe("single_emit");
  });

  it("opcode_stream_dispatch", () => {
    const b = parseBundle(
      baseShell({
        strategy: "opcode_stream_dispatch",
        dispatcher_id: "universal_router",
        mask: "0x7f",
        allow_revert_bit: "0x80",
        per_opcode_emit: {
          "0x00": {
            name: "V3_SWAP_EXACT_IN",
            category: "dex",
            action: "swap",
            fields: {},
          },
        },
        unknown_opcode_policy: "deny",
      }),
    );
    expect(b.emit.strategy).toBe("opcode_stream_dispatch");
  });

  it("enum_tagged_dispatch", () => {
    const b = parseBundle(
      baseShell({
        strategy: "enum_tagged_dispatch",
        dispatcher_id: "balancer_v2",
        tag_path: "$.args.request.userData",
        tag_decoder: "uint256_at_offset_0",
        per_variant_emit: {
          "0": {
            name: "INIT",
            category: "dex",
            action: "liquidity_init",
            fields: {},
          },
        },
        unknown_variant_policy: "deny",
      }),
    );
    expect(b.emit.strategy).toBe("enum_tagged_dispatch");
  });

  it("multicall_recurse", () => {
    const b = parseBundle(
      baseShell({
        strategy: "multicall_recurse",
        recurse_rule_id: "self_array_bytes_last_arg",
        max_depth: 3,
      }),
    );
    expect(b.emit.strategy).toBe("multicall_recurse");
  });
});

describe("parseBundle — error paths", () => {
  it("rejects non-object input", () => {
    expect(() => parseBundle(null)).toThrow(BundleParseError);
    expect(() => parseBundle("not-an-object")).toThrow(BundleParseError);
  });

  it("rejects wrong bundle type", () => {
    expect(() =>
      parseBundle({
        type: "policy_bundle",
        id: "x",
        publisher: "p",
        match: { chain_ids: [1], to: [], selector: "0x12345678" },
        abi_fragment: { function_name: "x", abi: {} },
        emit: {
          strategy: "single_emit",
          category: "x",
          action: "y",
          fields: {},
        },
        requires: { imperative: [], host_capabilities: [], extension: ">=0.1.0" },
      }),
    ).toThrow(/adapter_function/);
  });

  it("rejects malformed selector", () => {
    expect(() =>
      parseBundle({
        type: "adapter_function",
        id: "x",
        publisher: "p",
        match: { chain_ids: [1], to: [], selector: "0xshort" },
        abi_fragment: { function_name: "x", abi: {} },
        emit: {
          strategy: "single_emit",
          category: "x",
          action: "y",
          fields: {},
        },
        requires: { imperative: [], host_capabilities: [], extension: ">=0.1.0" },
      }),
    ).toThrow(/selector/);
  });

  it("rejects unknown emit strategy", () => {
    expect(() =>
      parseBundle({
        type: "adapter_function",
        id: "x",
        publisher: "p",
        match: { chain_ids: [1], to: [], selector: "0x12345678" },
        abi_fragment: { function_name: "x", abi: {} },
        emit: { strategy: "bogus" },
        requires: { imperative: [], host_capabilities: [], extension: ">=0.1.0" },
      }),
    ).toThrow(/strategy/);
  });

  it("rejects ValueExpr that mixes literal + from", () => {
    expect(() =>
      parseBundle({
        type: "adapter_function",
        id: "x",
        publisher: "p",
        match: { chain_ids: [1], to: [], selector: "0x12345678" },
        abi_fragment: { function_name: "x", abi: {} },
        emit: {
          strategy: "single_emit",
          category: "x",
          action: "y",
          fields: { foo: { literal: 1, from: "$.args.a" } },
        },
        requires: { imperative: [], host_capabilities: [], extension: ">=0.1.0" },
      }),
    ).toThrow(/literal/);
  });

  it("rejects transform with > 4 args", () => {
    expect(() =>
      parseBundle({
        type: "adapter_function",
        id: "x",
        publisher: "p",
        match: { chain_ids: [1], to: [], selector: "0x12345678" },
        abi_fragment: { function_name: "x", abi: {} },
        emit: {
          strategy: "single_emit",
          category: "x",
          action: "y",
          fields: {
            foo: {
              fn: "concat_bytes",
              args: [
                { literal: 1 },
                { literal: 2 },
                { literal: 3 },
                { literal: 4 },
                { literal: 5 },
              ],
            },
          },
        },
        requires: { imperative: [], host_capabilities: [], extension: ">=0.1.0" },
      }),
    ).toThrow(/max 4 args/);
  });

  it("rejects unknown BuiltinFn", () => {
    expect(() =>
      parseBundle({
        type: "adapter_function",
        id: "x",
        publisher: "p",
        match: { chain_ids: [1], to: [], selector: "0x12345678" },
        abi_fragment: { function_name: "x", abi: {} },
        emit: {
          strategy: "single_emit",
          category: "x",
          action: "y",
          fields: { foo: { fn: "not_a_builtin", args: [] } },
        },
        requires: { imperative: [], host_capabilities: [], extension: ">=0.1.0" },
      }),
    ).toThrow(/unknown function/);
  });

  it("rejects multicall_recurse with out-of-range max_depth", () => {
    expect(() =>
      parseBundle({
        type: "adapter_function",
        id: "x",
        publisher: "p",
        match: { chain_ids: [1], to: [], selector: "0x12345678" },
        abi_fragment: { function_name: "x", abi: {} },
        emit: {
          strategy: "multicall_recurse",
          recurse_rule_id: "x",
          max_depth: 99,
        },
        requires: { imperative: [], host_capabilities: [], extension: ">=0.1.0" },
      }),
    ).toThrow(/max_depth/);
  });
});
