import { describe, expect, it } from "vitest";
import {
  isValidCallKeySegment,
  isValidChainSegment,
  isValidAddressSegment,
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
});
