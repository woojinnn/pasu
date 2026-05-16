import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  readContract: vi.fn(),
  createPublicClient: vi.fn(),
  fallback: vi.fn((transports: unknown[]) => ({
    kind: "fallback",
    transports,
  })),
  http: vi.fn((url: string, options: unknown) => ({
    kind: "http",
    url,
    options,
  })),
}));

vi.mock("viem", async () => ({
  createPublicClient: mocks.createPublicClient,
  fallback: mocks.fallback,
  http: mocks.http,
  parseAbi: (abi: readonly string[]) => abi,
}));

import {
  __resetClientCache,
  readDecimals,
  rpcClient,
  type Address,
} from "../rpc-client";

const TOKEN_A = "0x2222222222222222222222222222222222222222" as Address;
const TOKEN_B = "0x3333333333333333333333333333333333333333" as Address;

describe("rpc-client", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    __resetClientCache();
    mocks.createPublicClient.mockReturnValue({
      readContract: mocks.readContract,
    });
    mocks.readContract.mockImplementation(async ({ functionName, address }) => {
      if (address === TOKEN_B) throw new Error("revert");
      if (functionName === "decimals") return 6;
      throw new Error("unexpected call");
    });
  });

  it("creates a viem client with fallback transports and multicall batching", () => {
    const client = rpcClient(1);
    expect(client).toBeDefined();
    expect(mocks.http).toHaveBeenCalled();
    expect(mocks.fallback).toHaveBeenCalled();
    expect(mocks.createPublicClient).toHaveBeenCalledWith(
      expect.objectContaining({ batch: { multicall: true } }),
    );
  });

  it("reads decimals and returns undefined on failure", async () => {
    await expect(readDecimals(1, TOKEN_A)).resolves.toBe(6);
    await expect(readDecimals(1, TOKEN_B)).resolves.toBeUndefined();
  });
});
