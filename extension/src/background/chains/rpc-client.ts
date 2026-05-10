import {
  createPublicClient,
  fallback,
  http,
  parseAbi,
  type Address as ViemAddress,
  type PublicClient,
} from "viem";
import { chainConfig } from "./chain-config";

// Re-export Address so consumers don't need a parallel viem import.
export type Address = ViemAddress;

const ERC20_ABI = parseAbi([
  "function balanceOf(address) view returns (uint256)",
  "function allowance(address owner, address spender) view returns (uint256)",
  "function decimals() view returns (uint8)",
  "function symbol() view returns (string)",
] as const);

const clientCache = new Map<number, PublicClient>();

export function rpcClient(chainId: number): PublicClient {
  const cached = clientCache.get(chainId);
  if (cached) return cached;
  const cfg = chainConfig(chainId);
  const client = createPublicClient({
    chain: cfg.viem,
    transport: fallback(
      cfg.rpcUrls.map((url) => http(url, { timeout: 8_000 })),
    ),
    batch: { multicall: true },
  });
  clientCache.set(chainId, client);
  return client;
}

export async function readBalances(
  chainId: number,
  walletAddress: Address,
  tokenAddresses: readonly Address[],
): Promise<readonly (bigint | undefined)[]> {
  if (tokenAddresses.length === 0) return [];
  let client: PublicClient;
  try {
    client = rpcClient(chainId);
  } catch {
    return tokenAddresses.map(() => undefined);
  }
  const results = await Promise.allSettled(
    tokenAddresses.map((token) =>
      client.readContract({
        address: token,
        abi: ERC20_ABI,
        functionName: "balanceOf",
        args: [walletAddress],
      }),
    ),
  );
  return results.map((result) =>
    result.status === "fulfilled" ? (result.value as bigint) : undefined,
  );
}

export async function readAllowances(
  chainId: number,
  walletAddress: Address,
  tokenAddresses: readonly Address[],
  spenders: readonly Address[],
): Promise<readonly (bigint | undefined)[]> {
  if (tokenAddresses.length === 0) return [];
  let client: PublicClient;
  try {
    client = rpcClient(chainId);
  } catch {
    return tokenAddresses.map(() => undefined);
  }
  const results = await Promise.allSettled(
    tokenAddresses.map((token, index) => {
      const spender = spenders[index];
      if (!spender) return Promise.reject(new Error("missing spender"));
      return client.readContract({
        address: token,
        abi: ERC20_ABI,
        functionName: "allowance",
        args: [walletAddress, spender],
      });
    }),
  );
  return results.map((result) =>
    result.status === "fulfilled" ? (result.value as bigint) : undefined,
  );
}

export async function readDecimals(
  chainId: number,
  token: Address,
): Promise<number | undefined> {
  try {
    const v = await rpcClient(chainId).readContract({
      address: token,
      abi: ERC20_ABI,
      functionName: "decimals",
    });
    return Number(v);
  } catch {
    return undefined;
  }
}

/** Test-only: clear the cached PublicClients. */
export function __resetClientCache(): void {
  clientCache.clear();
}
