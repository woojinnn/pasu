/**
 * Phase A.1 (Task 5) — SW typed-data router, manifest-driven.
 *
 * `routeTypedData` / `routeTypedSignaturePayload` are now async and
 * delegate the decode to the WASM `declarative_route_typed_data_v3_json`
 * export after installing the `(chainId, verifyingContract, primaryType)`
 * manifest via `installDeclarativeBundleV3ByTypedData`. The legacy Permit2
 * hardcode is gone — these tests assert the generic triple-keyed flow over
 * four representative typed-data protocols plus the install-miss path.
 *
 * Both collaborators are mocked: the adapter-loader install (network +
 * WASM install side-effect) and the WASM route call. The router under test
 * only marshals the typed-data domain triple into those calls and maps the
 * WASM envelope back to `{ actions, decoderId }`.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  installDeclarativeBundleV3ByTypedData: vi.fn(),
  declarativeRouteTypedDataV3: vi.fn(),
}));

vi.mock("../adapter-loader/declarative-adapter-loader", () => ({
  installDeclarativeBundleV3ByTypedData:
    mocks.installDeclarativeBundleV3ByTypedData,
}));

vi.mock("../wasm-bridge", () => ({
  declarativeRouteTypedDataV3: mocks.declarativeRouteTypedDataV3,
}));

import { routeTypedSignaturePayload } from "../sig-routing";
import { RequestType, type TypedSignaturePayload } from "@lib/types";

// ── Fixture addresses ──────────────────────────────────────────────────
const PERMIT2 = "0x000000000022d473030f116ddee9f6b43ac78ba3";
const USDC = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
const UNISWAPX_REACTOR = "0x6000da47483062a0d734ba3dc7576ce6a0b645c4";
const HYPERLIQUID_VC = "0x0000000000000000000000000000000000000000";
const OWNER = "0x1111111111111111111111111111111111111111" as `0x${string}`;

/** Build a `TypedSignaturePayload` envelope around a raw EIP-712 object. */
function payload(typedData: unknown): { payload: TypedSignaturePayload } {
  return {
    payload: {
      type: RequestType.TYPED_SIGNATURE,
      chainId: 1,
      hostname: "app.uniswap.org",
      address: OWNER,
      typedData,
    },
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  // Default: install succeeds. Per-case overrides where needed.
  mocks.installDeclarativeBundleV3ByTypedData.mockResolvedValue({
    ok: true,
    bundleId: "stub@1.0.0",
  });
});

describe("routeTypedSignaturePayload — manifest-driven typed-data router", () => {
  it("Permit2 PermitSingle (mainnet) installs by triple + routes", async () => {
    mocks.declarativeRouteTypedDataV3.mockResolvedValue({
      ok: true,
      data: {
        actions: [{ meta: { nature: { kind: "offchain_sig" } }, body: {} }],
        decoder_id: "uniswap/permit2/permitSingle@1.0.0",
      },
    });

    const typedData = {
      domain: {
        name: "Permit2",
        chainId: 1,
        verifyingContract: PERMIT2,
      },
      primaryType: "PermitSingle",
      types: { PermitSingle: [{ name: "details", type: "PermitDetails" }] },
      message: {
        details: {
          token: USDC,
          amount: "1461501637330902918203684832716283019655932542975",
          expiration: "1700000000",
          nonce: "0",
        },
        spender: UNISWAPX_REACTOR,
        sigDeadline: "1700000000",
      },
    };

    const result = await routeTypedSignaturePayload(payload(typedData));

    expect(mocks.installDeclarativeBundleV3ByTypedData).toHaveBeenCalledWith({
      chainId: 1,
      verifyingContract: PERMIT2,
      primaryType: "PermitSingle",
    });
    expect(result).not.toBeNull();
    expect(result?.decoderId).toBe("uniswap/permit2/permitSingle@1.0.0");
    expect(result?.actions).toHaveLength(1);
    expect(
      (result?.actions[0] as { meta: { nature: { kind: string } } }).meta.nature
        .kind,
    ).toBe("offchain_sig");
  });

  it("UniswapX ExclusiveDutchOrder (mainnet) routes", async () => {
    mocks.declarativeRouteTypedDataV3.mockResolvedValue({
      ok: true,
      data: {
        actions: [{ meta: { nature: { kind: "offchain_sig" } }, body: {} }],
        decoder_id: "uniswap/uniswapx/exclusiveDutchOrder@1.0.0",
      },
    });

    const typedData = {
      domain: {
        name: "UniswapX",
        chainId: 1,
        verifyingContract: UNISWAPX_REACTOR,
      },
      primaryType: "ExclusiveDutchOrder",
      types: {
        ExclusiveDutchOrder: [{ name: "info", type: "OrderInfo" }],
      },
      message: { info: {}, inputToken: USDC },
    };

    const result = await routeTypedSignaturePayload(payload(typedData));

    expect(mocks.installDeclarativeBundleV3ByTypedData).toHaveBeenCalledWith({
      chainId: 1,
      verifyingContract: UNISWAPX_REACTOR,
      primaryType: "ExclusiveDutchOrder",
    });
    expect(result?.decoderId).toBe(
      "uniswap/uniswapx/exclusiveDutchOrder@1.0.0",
    );
    expect(result?.actions).toHaveLength(1);
    expect(
      (result?.actions[0] as { meta: { nature: { kind: string } } }).meta.nature
        .kind,
    ).toBe("offchain_sig");
  });

  it("HyperLiquid UsdSend (chainId 42161) passes the colon primaryType through unescaped", async () => {
    mocks.declarativeRouteTypedDataV3.mockResolvedValue({
      ok: true,
      data: {
        actions: [{ meta: { nature: { kind: "offchain_sig" } }, body: {} }],
        decoder_id: "hyperliquid/exchange/usdSend@1.0.0",
      },
    });

    const typedData = {
      domain: {
        name: "HyperliquidSignTransaction",
        chainId: 42161,
        verifyingContract: HYPERLIQUID_VC,
      },
      primaryType: "HyperliquidTransaction:UsdSend",
      types: {
        "HyperliquidTransaction:UsdSend": [
          { name: "destination", type: "string" },
          { name: "amount", type: "string" },
          { name: "time", type: "uint64" },
        ],
      },
      message: { destination: OWNER, amount: "100", time: 1700000000 },
    };

    const result = await routeTypedSignaturePayload(payload(typedData));

    // The colon primaryType is forwarded verbatim — escaping is the URL
    // builder's job (`typedDataUrl` in registry/client.ts), not the router's.
    expect(mocks.installDeclarativeBundleV3ByTypedData).toHaveBeenCalledWith({
      chainId: 42161,
      verifyingContract: HYPERLIQUID_VC,
      primaryType: "HyperliquidTransaction:UsdSend",
    });
    expect(result?.decoderId).toBe("hyperliquid/exchange/usdSend@1.0.0");
    expect(result?.actions).toHaveLength(1);
    expect(
      (result?.actions[0] as { meta: { nature: { kind: string } } }).meta.nature
        .kind,
    ).toBe("offchain_sig");
  });

  it("EIP-2612 USDC Permit (mainnet) routes by (vc, primaryType) ignoring domain.name", async () => {
    mocks.declarativeRouteTypedDataV3.mockResolvedValue({
      ok: true,
      data: {
        actions: [{ meta: { nature: { kind: "offchain_sig" } }, body: {} }],
        decoder_id: "standard/erc2612/permit@1.0.0",
      },
    });

    const typedData = {
      domain: {
        name: "USD Coin",
        version: "2",
        chainId: 1,
        verifyingContract: USDC,
      },
      primaryType: "Permit",
      types: {
        Permit: [
          { name: "owner", type: "address" },
          { name: "spender", type: "address" },
          { name: "value", type: "uint256" },
          { name: "nonce", type: "uint256" },
          { name: "deadline", type: "uint256" },
        ],
      },
      message: {
        owner: OWNER,
        spender: UNISWAPX_REACTOR,
        value: "1000000",
        nonce: "0",
        deadline: "1700000000",
      },
    };

    const result = await routeTypedSignaturePayload(payload(typedData));

    expect(mocks.installDeclarativeBundleV3ByTypedData).toHaveBeenCalledWith({
      chainId: 1,
      verifyingContract: USDC,
      primaryType: "Permit",
    });
    expect(result?.decoderId).toBe("standard/erc2612/permit@1.0.0");
    expect(result?.actions).toHaveLength(1);
    expect(
      (result?.actions[0] as { meta: { nature: { kind: string } } }).meta.nature
        .kind,
    ).toBe("offchain_sig");
  });

  it("returns null and does NOT call WASM route when install misses", async () => {
    mocks.installDeclarativeBundleV3ByTypedData.mockResolvedValue({
      ok: false,
      reason: "manifest_not_found",
    });

    const typedData = {
      domain: {
        name: "UnknownProtocol",
        chainId: 1,
        verifyingContract: "0x9999999999999999999999999999999999999999",
      },
      primaryType: "MysteryOrder",
      types: { MysteryOrder: [{ name: "x", type: "uint256" }] },
      message: { x: "1" },
    };

    const result = await routeTypedSignaturePayload(payload(typedData));

    expect(result).toBeNull();
    expect(mocks.declarativeRouteTypedDataV3).not.toHaveBeenCalled();
  });
});
