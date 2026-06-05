import { beforeEach, describe, expect, it, vi } from "vitest";
import { encodeAbiParameters, encodeFunctionData, type Hex } from "viem";

const mocks = vi.hoisted(() => ({
  installDeclarativeBundleV3: vi.fn(),
  declarativeRouteRequestV3: vi.fn(),
}));

vi.mock("../adapter-loader/declarative-adapter-loader", () => ({
  installDeclarativeBundleV3: mocks.installDeclarativeBundleV3,
  InstallDeclarativeV3Error: class InstallDeclarativeV3Error extends Error {},
}));

vi.mock("../wasm-bridge", () => ({
  declarativeRouteRequestV3: mocks.declarativeRouteRequestV3,
  EngineError: class EngineError extends Error {},
}));

import { tryDeclarativeRouteV3 } from "../adapter-loader/declarative-route";
import type { V3Bundle } from "../adapter-loader/bundle-schema";
import type { InstallDeclarativeV3Result } from "../adapter-loader/declarative-adapter-loader";

const NFPM_BASE = "0x03a520b32c04bf3beef7beb72e919cf822ed34f1";
const nfpmMulticallAbi = [
  {
    name: "multicall",
    type: "function",
    stateMutability: "payable",
    inputs: [{ name: "data", type: "bytes[]" }],
  },
] as const;

const nfpmMulticallBundle: V3Bundle = {
  type: "adapter_action",
  id: "uniswap/v3-nfpm/multicall@1.0.0",
  publisher: "uniswap.eth",
  schema_version: "3",
  match: {
    selector: "0xac9650d8",
    chain_to_addresses: {
      "8453": [NFPM_BASE],
    },
  },
  abi_fragment: {
    function_name: "multicall",
    abi: nfpmMulticallAbi[0],
  },
  emit: {
    strategy: "multicall_recurse",
    recurse_rule_id: "self_array_bytes_last_arg",
    max_depth: 3,
  },
};

const childBundle: V3Bundle = {
  ...nfpmMulticallBundle,
  id: "uniswap/v3-nfpm/collect@1.0.0",
  match: {
    selector: "0xfc6f7865",
    chain_to_addresses: {
      "8453": [NFPM_BASE],
    },
  },
  emit: {
    strategy: "single_emit",
    body: { domain: "amm" },
  },
};

const BUNDLER3 = "0x6566194141eefa99af43bb5aa71460ca2dc90245";
const GA1 = "0x4a6c312ec70e8747a587ee860a0353cd42be0ae0";
const bundler3MulticallAbi = [
  {
    name: "multicall",
    type: "function",
    stateMutability: "payable",
    inputs: [
      {
        name: "bundle",
        type: "tuple[]",
        components: [
          { name: "to", type: "address" },
          { name: "data", type: "bytes" },
          { name: "value", type: "uint256" },
          { name: "skipRevert", type: "bool" },
          { name: "callbackHash", type: "bytes32" },
        ],
      },
    ],
  },
] as const;

const bundler3MulticallBundle: V3Bundle = {
  type: "adapter_action",
  id: "morpho/bundler3/1-multicall@1.0.0",
  publisher: "morpho.eth",
  schema_version: "3",
  match: {
    selector: "0x374f435d",
    chain_to_addresses: { "1": [BUNDLER3] },
  },
  abi_fragment: {
    function_name: "multicall",
    abi: bundler3MulticallAbi[0],
  },
  emit: {
    strategy: "multicall_call_array",
    recurse_arg: "bundle",
    max_depth: 4,
  },
};

function installed(bundle: V3Bundle): InstallDeclarativeV3Result {
  return {
    decoderId: bundle.id,
    bundleId: bundle.id,
    bundle,
  };
}

describe("tryDeclarativeRouteV3", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("preinstalls NFPM multicall child selectors before routing through WASM", async () => {
    const collectCall =
      "0xfc6f786500000000000000000000000000000000000000000000000000000000004f0f97000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000ffffffffffffffffffffffffffffffff00000000000000000000000000000000ffffffffffffffffffffffffffffffff" as Hex;
    const unwrapWethCall =
      "0x49404b7c00000000000000000000000000000000000000000000000000000066ed167a3b000000000000000000000000676fa5b94067c2be14bc025df6c5c80dedf49a54" as Hex;
    const sweepTokenCall =
      "0xdf2ab5bb000000000000000000000000833589fcd6edb6e08f4c7c32d4f71b54bda0291300000000000000000000000000000000000000000000000000000000000002e9000000000000000000000000676fa5b94067c2be14bc025df6c5c80dedf49a54" as Hex;
    const collectAsEthMulticall = encodeFunctionData({
      abi: nfpmMulticallAbi,
      functionName: "multicall",
      args: [[collectCall, unwrapWethCall, sweepTokenCall]],
    });

    mocks.installDeclarativeBundleV3.mockImplementation(
      async ({ selector }: { selector: string }) => {
        if (selector === "0xac9650d8") return installed(nfpmMulticallBundle);
        if (selector === "0xfc6f7865") return installed(childBundle);
        return null;
      },
    );
    mocks.declarativeRouteRequestV3.mockResolvedValue({
      decoder_id: "uniswap/v3-nfpm/multicall@1.0.0",
      actions: [{ body: { domain: "multicall", actions: [] } }],
    });

    const outcome = await tryDeclarativeRouteV3({
      chainId: 8453,
      from: "0x676fa5b94067c2be14bc025df6c5c80dedf49a54",
      to: NFPM_BASE,
      calldataHex: collectAsEthMulticall,
    });

    expect(outcome.kind).toBe("hit");
    expect(mocks.installDeclarativeBundleV3).toHaveBeenCalledTimes(4);
    expect(
      mocks.installDeclarativeBundleV3.mock.calls.map(
        ([arg]) => (arg as { selector: string }).selector,
      ),
    ).toEqual(["0xac9650d8", "0xfc6f7865", "0x49404b7c", "0xdf2ab5bb"]);
    expect(mocks.declarativeRouteRequestV3).toHaveBeenCalledOnce();
  });

  it("preinstalls Bundler3 per-leg-to children at each leg's OWN `to`", async () => {
    const zero32 = `0x${"00".repeat(32)}` as Hex;
    // Two GA1 legs (erc4626Deposit 0x6ef5eeae, morphoSupply 0x5b866db6) — the
    // pre-install only needs each leg's `to` + 4-byte selector.
    const legDeposit = `0x6ef5eeae${"00".repeat(32)}` as Hex;
    const legSupply = `0x5b866db6${"00".repeat(32)}` as Hex;
    const bundleCalldata = encodeFunctionData({
      abi: bundler3MulticallAbi,
      functionName: "multicall",
      args: [
        [
          { to: GA1, data: legDeposit, value: 0n, skipRevert: false, callbackHash: zero32 },
          { to: GA1, data: legSupply, value: 0n, skipRevert: false, callbackHash: zero32 },
        ],
      ],
    });

    mocks.installDeclarativeBundleV3.mockImplementation(
      async ({ selector }: { selector: string }) =>
        selector === "0x374f435d"
          ? installed(bundler3MulticallBundle)
          : installed(childBundle),
    );
    mocks.declarativeRouteRequestV3.mockResolvedValue({
      decoder_id: "morpho/bundler3/1-multicall@1.0.0",
      actions: [{ body: { domain: "multicall", actions: [] } }],
    });

    const outcome = await tryDeclarativeRouteV3({
      chainId: 1,
      from: "0x676fa5b94067c2be14bc025df6c5c80dedf49a54",
      to: BUNDLER3,
      calldataHex: bundleCalldata,
    });

    expect(outcome.kind).toBe("hit");
    // outer install (Bundler3) + each leg installed AT GeneralAdapter1 (its own to).
    const calls = mocks.installDeclarativeBundleV3.mock.calls.map(([arg]) => ({
      to: (arg as { to: string }).to.toLowerCase(),
      selector: (arg as { selector: string }).selector,
    }));
    expect(calls).toEqual([
      { to: BUNDLER3, selector: "0x374f435d" },
      { to: GA1, selector: "0x6ef5eeae" },
      { to: GA1, selector: "0x5b866db6" },
    ]);
  });

  it("preinstalls Bundler3 reenter(Call[]) callback legs nested in a morphoFlashLoan (D-C)", async () => {
    const zero32 = `0x${"00".repeat(32)}` as Hex;
    const callTupleArray = [
      {
        type: "tuple[]",
        components: [
          { name: "to", type: "address" },
          { name: "data", type: "bytes" },
          { name: "value", type: "uint256" },
          { name: "skipRevert", type: "bool" },
          { name: "callbackHash", type: "bytes32" },
        ],
      },
    ] as const;
    const morphoFlashLoanAbi = [
      {
        name: "morphoFlashLoan",
        type: "function",
        stateMutability: "nonpayable",
        inputs: [
          { name: "token", type: "address" },
          { name: "assets", type: "uint256" },
          { name: "data", type: "bytes" },
        ],
      },
    ] as const;
    // The installed morphoFlashLoan manifest tells the pre-installer (via
    // emit.reenter_callback_arg + the ABI) that arg `data` is a reenter callback —
    // NO hardcoded selector. strategy=reenter_only (a pure trampoline, no body).
    const flashLoanBundle: V3Bundle = {
      type: "adapter_action",
      id: "morpho/general-adapter1/1-morphoFlashLoan@1.0.0",
      publisher: "morpho.eth",
      schema_version: "3",
      match: { selector: "0xe2975912", chain_to_addresses: { "1": [GA1] } },
      abi_fragment: { function_name: "morphoFlashLoan", abi: morphoFlashLoanAbi[0] },
      emit: { strategy: "reenter_only", reenter_callback_arg: "data" },
    };

    // The flashLoan's reenter callback carries one leg (selector 0xaabbccdd at
    // GA1) that exists ONLY inside the callback — never at the top level.
    const callbackData = encodeAbiParameters(callTupleArray, [
      [
        {
          to: GA1,
          data: "0xaabbccdd" as Hex,
          value: 0n,
          skipRevert: false,
          callbackHash: zero32,
        },
      ],
    ]);
    const flashLoanData = encodeFunctionData({
      abi: morphoFlashLoanAbi,
      functionName: "morphoFlashLoan",
      args: ["0xdac17f958d2ee523a2206206994597c13d831ec7", 1000n, callbackData],
    });
    const bundleCalldata = encodeFunctionData({
      abi: bundler3MulticallAbi,
      functionName: "multicall",
      args: [
        [
          {
            to: GA1,
            data: flashLoanData,
            value: 0n,
            skipRevert: false,
            callbackHash: zero32,
          },
        ],
      ],
    });

    mocks.installDeclarativeBundleV3.mockImplementation(
      async ({ selector }: { selector: string }) => {
        if (selector === "0x374f435d") return installed(bundler3MulticallBundle);
        if (selector === "0xe2975912") return installed(flashLoanBundle);
        return installed(childBundle);
      },
    );
    mocks.declarativeRouteRequestV3.mockResolvedValue({
      decoder_id: "morpho/bundler3/1-multicall@1.0.0",
      actions: [{ body: { domain: "multicall", actions: [] } }],
    });

    const outcome = await tryDeclarativeRouteV3({
      chainId: 1,
      from: "0x676fa5b94067c2be14bc025df6c5c80dedf49a54",
      to: BUNDLER3,
      calldataHex: bundleCalldata,
    });

    expect(outcome.kind).toBe("hit");
    const calls = mocks.installDeclarativeBundleV3.mock.calls.map(([arg]) => ({
      to: (arg as { to: string }).to.toLowerCase(),
      selector: (arg as { selector: string }).selector,
    }));
    // Outer bundle + the top-level flashLoan leg + the CALLBACK-only leg.
    expect(calls).toContainEqual({ to: BUNDLER3, selector: "0x374f435d" });
    expect(calls).toContainEqual({ to: GA1, selector: "0xe2975912" });
    expect(calls).toContainEqual({ to: GA1, selector: "0xaabbccdd" });
  });
});

const STETH = "0xae7ab96520de3a18e5e111b5eaab095312d7fe84";
const stethStakeEthBundle: V3Bundle = {
  type: "adapter_action",
  id: "lido/steth/stake-eth@1.0.0",
  publisher: "lido.eth",
  schema_version: "3",
  match: {
    selector: "0x00000000",
    chain_to_addresses: { "1": [STETH] },
  },
  abi_fragment: {
    function_name: "fallback",
    abi: { name: "fallback", type: "function", stateMutability: "payable", inputs: [] },
  },
  emit: {
    strategy: "single_emit",
    body: { domain: "liquid_staking" },
  },
};

describe("tryDeclarativeRouteV3 — selector-less native-ETH stake", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("routes a value-bearing empty-calldata tx under the 0x00000000 sentinel", async () => {
    mocks.installDeclarativeBundleV3.mockImplementation(
      async ({ selector }: { selector: string }) =>
        selector === "0x00000000" ? installed(stethStakeEthBundle) : null,
    );
    mocks.declarativeRouteRequestV3.mockResolvedValue({
      decoder_id: "lido/steth/stake-eth@1.0.0",
      actions: [{ body: { domain: "liquid_staking", action: "stake" } }],
    });

    const outcome = await tryDeclarativeRouteV3({
      chainId: 1,
      from: "0x31ca56db7d434bcb3a588149acf5d2b615aec477",
      to: STETH,
      valueWei: "1530000000000000000",
      calldataHex: "0x",
    });

    expect(outcome.kind).toBe("hit");
    expect(mocks.installDeclarativeBundleV3).toHaveBeenCalledWith({
      chainId: 1,
      to: STETH,
      selector: "0x00000000",
    });
    // Empty calldata is normalised to "0x" when handed to the WASM route.
    expect(mocks.declarativeRouteRequestV3).toHaveBeenCalledWith(
      expect.objectContaining({ selector: "0x00000000", calldata: "0x" }),
    );
  });

  it("misses for a bare-ETH transfer to an address with no sentinel manifest", async () => {
    mocks.installDeclarativeBundleV3.mockResolvedValue(null);

    const outcome = await tryDeclarativeRouteV3({
      chainId: 1,
      from: "0xabc0000000000000000000000000000000000001",
      to: "0x1111111111111111111111111111111111111111",
      valueWei: "1000000000000000000",
      calldataHex: "0x",
    });

    expect(outcome).toEqual({ kind: "miss", reason: "bundle_not_installed" });
    expect(mocks.installDeclarativeBundleV3).toHaveBeenCalledWith(
      expect.objectContaining({ selector: "0x00000000" }),
    );
  });

  it("does NOT synthesize the sentinel for a 0-value empty-calldata tx", async () => {
    const outcome = await tryDeclarativeRouteV3({
      chainId: 1,
      from: "0xabc0000000000000000000000000000000000001",
      to: STETH,
      valueWei: "0",
      calldataHex: "0x",
    });

    expect(outcome).toEqual({ kind: "miss", reason: "no_selector" });
    expect(mocks.installDeclarativeBundleV3).not.toHaveBeenCalled();
  });
});
