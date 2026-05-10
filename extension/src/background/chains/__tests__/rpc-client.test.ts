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
  readAllowances,
  readBalances,
  readDecimals,
  rpcClient,
  type Address,
} from "../rpc-client";

const OWNER = "0x1111111111111111111111111111111111111111" as Address;
const TOKEN_A = "0x2222222222222222222222222222222222222222" as Address;
const TOKEN_B = "0x3333333333333333333333333333333333333333" as Address;
const SPENDER_A = "0x4444444444444444444444444444444444444444" as Address;
const SPENDER_B = "0x5555555555555555555555555555555555555555" as Address;

describe("rpc-client", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    __resetClientCache();
    mocks.createPublicClient.mockReturnValue({
      readContract: mocks.readContract,
    });
    mocks.readContract.mockImplementation(async ({ functionName, address }) => {
      if (address === TOKEN_B) throw new Error("revert");
      if (functionName === "balanceOf") return 100n;
      if (functionName === "allowance") return 50n;
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

  it("reads balances in token input order and leaves failed calls undefined", async () => {
    const result = await readBalances(1, OWNER, [TOKEN_A, TOKEN_B]);
    expect(result).toEqual([100n, undefined]);
    expect(mocks.readContract).toHaveBeenCalledTimes(2);
  });

  it("reads allowances using the matching spender for each token", async () => {
    const result = await readAllowances(
      1,
      OWNER,
      [TOKEN_A, TOKEN_B],
      [SPENDER_A, SPENDER_B],
    );
    expect(result).toEqual([50n, undefined]);
    expect(mocks.readContract.mock.calls[0][0].args).toEqual([
      OWNER,
      SPENDER_A,
    ]);
    expect(mocks.readContract.mock.calls[1][0].args).toEqual([
      OWNER,
      SPENDER_B,
    ]);
  });

  it("reads decimals and returns undefined on failure", async () => {
    await expect(readDecimals(1, TOKEN_A)).resolves.toBe(6);
    await expect(readDecimals(1, TOKEN_B)).resolves.toBeUndefined();
  });
});
