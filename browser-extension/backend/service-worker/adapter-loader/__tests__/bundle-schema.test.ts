import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import {
  BundleParseError,
  matchEntriesV3,
  parseBundle,
  parseBundleV3,
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

  it("requires.adapter_capabilities surfaces token_metadata (Phase 7B)", () => {
    const bundle = parseBundle(loadV2SwapFixture());
    expect(bundle.requires.imperative).toEqual([]);
    // Phase 7B: token_metadata is now adapter-layer (static lookup);
    // host_capabilities is narrowed to dynamic-only.
    expect(bundle.requires.adapter_capabilities).toEqual(["token_metadata"]);
    expect(bundle.requires.host_capabilities).toEqual([]);
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

  it("accepts unfold_slipstream_path as a BuiltinFn (Phase 8)", () => {
    expect(() =>
      parseBundle({
        type: "adapter_function",
        id: "aerodrome/slipstream/exactInput@1.0.0",
        publisher: "aerodrome.eth",
        match: { chain_ids: [8453], to: [], selector: "0x12345678" },
        abi_fragment: { function_name: "exactInput", abi: {} },
        emit: {
          strategy: "single_emit",
          category: "dex",
          action: "swap",
          fields: {
            "inputToken.asset.address": {
              fn: "unfold_slipstream_path",
              args: [
                { from: "$.args.params.path" },
                { literal: "first_token" },
              ],
            },
            "extension.aerodrome.tick_spacing": {
              fn: "unfold_slipstream_path",
              args: [
                { from: "$.args.params.path" },
                { literal: "tick_spacing_at_hop" },
                { literal: 0 },
              ],
            },
          },
        },
        requires: { imperative: [], host_capabilities: [], extension: ">=0.1.0" },
      }),
    ).not.toThrow();
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

  it("rejects schema_version=3 — caller must use parseBundleV3", () => {
    expect(() =>
      parseBundle({
        type: "adapter_action",
        id: "x@1.0.0",
        publisher: "p",
        schema_version: "3",
        match: {
          chain_to_addresses: {
            "1": ["0x0000000000000000000000000000000000000001"],
          },
          selector: "0x12345678",
        },
        abi_fragment: { function_name: "x", abi: {} },
        emit: { strategy: "single_emit", body: {} },
      }),
    ).toThrow(/parseBundleV3/);
  });
});

describe("parseBundleV3", () => {
  const v3Bundle = {
    type: "adapter_action",
    id: "uniswap/v2-router-02/swapExactTokensForETH@1.0.0",
    publisher: "uniswap.eth",
    schema_version: "3",
    match: {
      selector: "0x18cbafe5",
      chain_to_addresses: {
        "1": ["0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"],
        "8453": ["0x4752ba5DBc23f44D87826276BF6Fd6b1C372aD24"],
      },
    },
    abi_fragment: {
      function_name: "swapExactTokensForETH",
      abi: { name: "swapExactTokensForETH", type: "function", inputs: [] },
    },
    emit: {
      strategy: "single_emit",
      body: {
        domain: "amm",
        amm: { action: "swap", swap: { venue: { name: "uniswap_v2" } } },
      },
    },
  };

  it("parses a valid v3 manifest and preserves emit pass-through", () => {
    const bundle = parseBundleV3(v3Bundle);
    expect(bundle).not.toBeNull();
    expect(bundle!.type).toBe("adapter_action");
    expect(bundle!.schema_version).toBe("3");
    expect(bundle!.id).toBe(
      "uniswap/v2-router-02/swapExactTokensForETH@1.0.0",
    );
    expect(bundle!.match.selector).toBe("0x18cbafe5");
    expect(bundle!.match.chain_to_addresses).toEqual({
      "1": ["0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"],
      "8453": ["0x4752ba5DBc23f44D87826276BF6Fd6b1C372aD24"],
    });
    // Pass-through invariant: the raw emit tree flows untouched so the
    // WASM-side action_builder can consume the exact registry payload.
    expect(bundle!.emit).toEqual(v3Bundle.emit);
  });

  it("returns null for v2 manifests (schema_version=2)", () => {
    expect(
      parseBundleV3({
        type: "adapter_function",
        id: "uniswap/swap-router-02/wrapETH@1.0.0",
        publisher: "uniswap.eth",
        schema_version: "2",
        match: {
          selector: "0x1c58db4f",
          chain_to_addresses: {
            "1": ["0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45"],
          },
        },
        abi_fragment: { function_name: "wrapETH", abi: {} },
        emit: { strategy: "single_emit", category: "misc", action: "wrap", fields: {} },
        requires: { imperative: [], host_capabilities: [], extension: ">=0.1.0" },
      }),
    ).toBeNull();
  });

  it("returns null for v1 manifests (schema_version absent)", () => {
    expect(
      parseBundleV3({
        type: "adapter_function",
        id: "demo@1.0.0",
        match: { chain_ids: [1], to: [], selector: "0x12345678" },
        abi_fragment: { function_name: "x", abi: {} },
        emit: { strategy: "single_emit", category: "x", action: "y", fields: {} },
        requires: { imperative: [], host_capabilities: [], extension: ">=0.1.0" },
      }),
    ).toBeNull();
  });

  it("throws BundleParseError when schema_version=3 but type != adapter_action", () => {
    expect(() =>
      parseBundleV3({
        ...v3Bundle,
        type: "adapter_function",
      }),
    ).toThrow(/adapter_action/);
  });

  it("throws BundleParseError when match.selector is malformed", () => {
    expect(() =>
      parseBundleV3({
        ...v3Bundle,
        match: { ...v3Bundle.match, selector: "0xshort" },
      }),
    ).toThrow(/selector/);
  });

  it("matchEntriesV3 expands chain_to_addresses to (chainId, address) pairs", () => {
    const bundle = parseBundleV3(v3Bundle)!;
    expect(matchEntriesV3(bundle.match)).toEqual([
      [1, "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"],
      [8453, "0x4752ba5DBc23f44D87826276BF6Fd6b1C372aD24"],
    ]);
  });
});
