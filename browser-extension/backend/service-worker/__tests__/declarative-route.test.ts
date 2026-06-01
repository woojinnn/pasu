import { beforeEach, describe, expect, it, vi } from "vitest";
import { encodeFunctionData, type Hex } from "viem";

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
});
