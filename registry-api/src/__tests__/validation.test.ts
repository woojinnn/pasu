import { describe, expect, it } from "vitest";
import {
  isValidCallKeySegment,
  isValidChainSegment,
  isValidAddressSegment,
  isTypedDataKey,
  parseProxyTarget,
} from "../validation";

describe("validation — callkey segment", () => {
  it("accepts a canonical lowercased callkey", () => {
    expect(
      isValidCallKeySegment(
        "1__0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2__0x38ed1739",
      ),
    ).toBe(true);
  });
  it("rejects path traversal", () => {
    expect(isValidCallKeySegment("../manifests/secret")).toBe(false);
    expect(isValidCallKeySegment("1__0x..__0x38ed1739")).toBe(false);
  });
  it("rejects an uppercased address", () => {
    expect(
      isValidCallKeySegment(
        "1__0xC02AAA39B223FE8D0A0E5C4F27EAD9083C756CC2__0x38ed1739",
      ),
    ).toBe(false);
  });
  it("rejects a wrong-length selector", () => {
    expect(
      isValidCallKeySegment(
        "1__0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2__0x38ed17",
      ),
    ).toBe(false);
  });
});

describe("validation — token segments", () => {
  it("accepts a positive chain id", () => {
    expect(isValidChainSegment("1")).toBe(true);
    expect(isValidChainSegment("8453")).toBe(true);
  });
  it("rejects non-numeric / leading-zero / zero chain id", () => {
    expect(isValidChainSegment("0")).toBe(false);
    expect(isValidChainSegment("01")).toBe(false);
    expect(isValidChainSegment("1a")).toBe(false);
    expect(isValidChainSegment("..")).toBe(false);
  });
  it("accepts a lowercased address", () => {
    expect(
      isValidAddressSegment("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"),
    ).toBe(true);
  });
  it("rejects a non-address token segment", () => {
    expect(isValidAddressSegment("0x123")).toBe(false);
    expect(isValidAddressSegment("../../etc/passwd")).toBe(false);
  });
});

describe("parseProxyTarget", () => {
  it("maps a callkey path to the GCS object name", () => {
    expect(
      parseProxyTarget(
        "/index/by-callkey/1__0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2__0x38ed1739.json",
      ),
    ).toEqual({
      ok: true,
      objectName:
        "index/by-callkey/1__0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2__0x38ed1739.json",
    });
  });
  it("maps a token path to the GCS object name", () => {
    expect(
      parseProxyTarget(
        "/tokens/1/0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2.json",
      ),
    ).toEqual({
      ok: true,
      objectName: "tokens/1/0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2.json",
    });
  });
  it("rejects unknown / traversal paths", () => {
    expect(parseProxyTarget("/manifests/uniswap/v2/foo.json").ok).toBe(false);
    expect(parseProxyTarget("/index/by-callkey/../../manifests/x.json").ok).toBe(
      false,
    );
    expect(parseProxyTarget("/tokens/1/..%2f..%2fsecret.json").ok).toBe(false);
  });

  it("maps generated bundle and context refs to GCS object names", () => {
    expect(
      parseProxyTarget(
        "/bundles/0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.json",
      ),
    ).toEqual({
      ok: true,
      objectName:
        "bundles/0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.json",
    });
    expect(
      parseProxyTarget(
        "/contexts/curve/factory_stable_ng_2coin_mainnet/1/0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2.json",
      ),
    ).toEqual({
      ok: true,
      objectName:
        "contexts/curve/factory_stable_ng_2coin_mainnet/1/0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2.json",
    });
  });

  it("rejects malformed generated bundle and context refs", () => {
    expect(parseProxyTarget("/bundles/not-a-sha.json").ok).toBe(false);
    expect(
      parseProxyTarget(
        "/contexts/curve/../1/0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2.json",
      ).ok,
    ).toBe(false);
    expect(
      parseProxyTarget(
        "/contexts/curve/factory/0/0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2.json",
      ).ok,
    ).toBe(false);
  });
});

describe("validation — typed-data key segment", () => {
  it("accepts canonical typed-data keys (Permit2 / UniswapX / EIP-2612)", () => {
    expect(
      isTypedDataKey(
        "1__0x000000000022d473030f116ddee9f6b43ac78ba3__PermitSingle",
      ),
    ).toBe(true);
    expect(
      isTypedDataKey(
        "1__0x6000da47483062a0d734ba3dc7576ce6a0b645c4__ExclusiveDutchOrder",
      ),
    ).toBe(true);
    expect(
      isTypedDataKey(
        "1__0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48__Permit",
      ),
    ).toBe(true);
  });
  it("accepts a primaryType with an embedded '__' (escaped EIP-712 colon)", () => {
    expect(
      isTypedDataKey(
        "42161__0x0000000000000000000000000000000000000000__HyperliquidTransaction__UsdSend",
      ),
    ).toBe(true);
  });
  it("rejects a hex-style (0x-prefixed) chain id", () => {
    expect(
      isTypedDataKey(
        "0x1__0x000000000022d473030f116ddee9f6b43ac78ba3__PermitSingle",
      ),
    ).toBe(false);
  });
  it("rejects a bad verifyingContract (not 0x + 40 hex)", () => {
    expect(isTypedDataKey("1__0x123__PermitSingle")).toBe(false);
    expect(
      isTypedDataKey(
        "1__0x000000000022D473030F116DDEE9F6B43AC78BA3__PermitSingle",
      ),
    ).toBe(false); // uppercase vc
  });
  it("rejects a missing / empty primaryType", () => {
    expect(
      isTypedDataKey("1__0x000000000022d473030f116ddee9f6b43ac78ba3__"),
    ).toBe(false);
    expect(
      isTypedDataKey("1__0x000000000022d473030f116ddee9f6b43ac78ba3"),
    ).toBe(false);
  });
  it("rejects a primaryType containing a '/' (traversal)", () => {
    expect(
      isTypedDataKey(
        "1__0x000000000022d473030f116ddee9f6b43ac78ba3__a/b",
      ),
    ).toBe(false);
  });
});

describe("parseProxyTarget — typed-data", () => {
  it("maps a Permit2 typed-data path to the GCS object name", () => {
    expect(
      parseProxyTarget(
        "/index/by-typed-data/1__0x000000000022d473030f116ddee9f6b43ac78ba3__PermitSingle.json",
      ),
    ).toEqual({
      ok: true,
      objectName:
        "index/by-typed-data/1__0x000000000022d473030f116ddee9f6b43ac78ba3__PermitSingle.json",
    });
  });
  it("maps a UniswapX ExclusiveDutchOrder typed-data path", () => {
    expect(
      parseProxyTarget(
        "/index/by-typed-data/1__0x6000da47483062a0d734ba3dc7576ce6a0b645c4__ExclusiveDutchOrder.json",
      ),
    ).toEqual({
      ok: true,
      objectName:
        "index/by-typed-data/1__0x6000da47483062a0d734ba3dc7576ce6a0b645c4__ExclusiveDutchOrder.json",
    });
  });
  it("maps an EIP-2612 Permit typed-data path", () => {
    expect(
      parseProxyTarget(
        "/index/by-typed-data/1__0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48__Permit.json",
      ),
    ).toEqual({
      ok: true,
      objectName:
        "index/by-typed-data/1__0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48__Permit.json",
    });
  });
  it("maps a typed-data path whose primaryType has an embedded '__'", () => {
    expect(
      parseProxyTarget(
        "/index/by-typed-data/42161__0x0000000000000000000000000000000000000000__HyperliquidTransaction__UsdSend.json",
      ),
    ).toEqual({
      ok: true,
      objectName:
        "index/by-typed-data/42161__0x0000000000000000000000000000000000000000__HyperliquidTransaction__UsdSend.json",
    });
  });
  it("rejects malformed typed-data paths", () => {
    // hex chain id
    expect(
      parseProxyTarget(
        "/index/by-typed-data/0x1__0x000000000022d473030f116ddee9f6b43ac78ba3__PermitSingle.json",
      ).ok,
    ).toBe(false);
    // bad verifyingContract
    expect(
      parseProxyTarget("/index/by-typed-data/1__0x123__PermitSingle.json").ok,
    ).toBe(false);
    // missing primaryType
    expect(
      parseProxyTarget(
        "/index/by-typed-data/1__0x000000000022d473030f116ddee9f6b43ac78ba3.json",
      ).ok,
    ).toBe(false);
    // traversal via .. (caught by the '%'/'..' guard)
    expect(
      parseProxyTarget(
        "/index/by-typed-data/../../manifests/secret.json",
      ).ok,
    ).toBe(false);
    expect(
      parseProxyTarget(
        "/index/by-typed-data/1__0x000000000022d473030f116ddee9f6b43ac78ba3__..%2fx.json",
      ).ok,
    ).toBe(false);
    // missing .json suffix
    expect(
      parseProxyTarget(
        "/index/by-typed-data/1__0x000000000022d473030f116ddee9f6b43ac78ba3__PermitSingle",
      ).ok,
    ).toBe(false);
    // a '/' inside the segment (would re-route the object path)
    expect(
      parseProxyTarget(
        "/index/by-typed-data/1__0x000000000022d473030f116ddee9f6b43ac78ba3__a/b.json",
      ).ok,
    ).toBe(false);
  });
});
