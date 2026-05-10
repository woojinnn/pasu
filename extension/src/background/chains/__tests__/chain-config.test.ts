import { describe, expect, it } from "vitest";
import { chainConfig, isChainSupported } from "../chain-config";

describe("chain-config", () => {
  it("exposes mainnet chains", () => {
    expect(isChainSupported(1)).toBe(true);
    expect(isChainSupported(8453)).toBe(true);
    expect(isChainSupported(99999)).toBe(false);
  });

  it("returns ordered RPC urls for mainnet with free fallback", () => {
    const c = chainConfig(1);
    expect(c.rpcUrls.length).toBeGreaterThan(0);
    expect(c.rpcUrls.some((u) => u.includes("llamarpc"))).toBe(true);
  });

  it("throws for unsupported chains", () => {
    expect(() => chainConfig(99999)).toThrow();
  });

  it("exposes coingecko platform + native ids per chain", () => {
    expect(chainConfig(1).coingeckoPlatform).toBe("ethereum");
    expect(chainConfig(137).coingeckoNativeId).toBe("matic-network");
    expect(chainConfig(8453).coingeckoNativeId).toBe("ethereum");
  });
});
