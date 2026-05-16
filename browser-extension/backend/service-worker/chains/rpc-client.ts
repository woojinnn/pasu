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
  "function decimals() view returns (uint8)",
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
