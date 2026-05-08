import {
  createPublicClient,
  fallback,
  http,
  parseAbi,
  type Address as ViemAddress,
  type PublicClient,
} from 'viem';
import { chainConfig } from './chain-config';

// Re-export Address so consumers don't need a parallel viem import.
export type Address = ViemAddress;

const ERC20_ABI = parseAbi([
  'function balanceOf(address) view returns (uint256)',
  'function allowance(address owner, address spender) view returns (uint256)',
  'function decimals() view returns (uint8)',
  'function symbol() view returns (string)',
] as const);

const clientCache = new Map<number, PublicClient>();

export function rpcClient(chainId: number): PublicClient {
  const cached = clientCache.get(chainId);
  if (cached) return cached;
  const cfg = chainConfig(chainId);
  const client = createPublicClient({
    chain: cfg.viem,
    transport: fallback(cfg.rpcUrls.map((url) => http(url, { timeout: 8_000 }))),
    batch: { multicall: true },
  });
  clientCache.set(chainId, client);
  return client;
}

export interface BalanceFact {
  owner: Address;
  token: Address;
  chainId: number;
}

export interface AllowanceFact {
  owner: Address;
  token: Address;
  spender: Address;
  chainId: number;
}

export async function readBalances(
  facts: readonly BalanceFact[],
): Promise<readonly (bigint | undefined)[]> {
  if (facts.length === 0) return [];
  const byChain = new Map<number, BalanceFact[]>();
  for (const f of facts) {
    const list = byChain.get(f.chainId) ?? [];
    list.push(f);
    byChain.set(f.chainId, list);
  }
  const out: (bigint | undefined)[] = new Array(facts.length).fill(undefined);
  await Promise.all(
    [...byChain.entries()].map(async ([chainId, perChain]) => {
      const client = rpcClient(chainId);
      const results = await Promise.allSettled(
        perChain.map((f) =>
          client.readContract({
            address: f.token,
            abi: ERC20_ABI,
            functionName: 'balanceOf',
            args: [f.owner],
          }),
        ),
      );
      for (let i = 0; i < perChain.length; i++) {
        const idx = facts.indexOf(perChain[i]);
        const r = results[i];
        if (r.status === 'fulfilled') out[idx] = r.value as bigint;
      }
    }),
  );
  return out;
}

export async function readAllowances(
  facts: readonly AllowanceFact[],
): Promise<readonly (bigint | undefined)[]> {
  if (facts.length === 0) return [];
  const byChain = new Map<number, AllowanceFact[]>();
  for (const f of facts) {
    const list = byChain.get(f.chainId) ?? [];
    list.push(f);
    byChain.set(f.chainId, list);
  }
  const out: (bigint | undefined)[] = new Array(facts.length).fill(undefined);
  await Promise.all(
    [...byChain.entries()].map(async ([chainId, perChain]) => {
      const client = rpcClient(chainId);
      const results = await Promise.allSettled(
        perChain.map((f) =>
          client.readContract({
            address: f.token,
            abi: ERC20_ABI,
            functionName: 'allowance',
            args: [f.owner, f.spender],
          }),
        ),
      );
      for (let i = 0; i < perChain.length; i++) {
        const idx = facts.indexOf(perChain[i]);
        const r = results[i];
        if (r.status === 'fulfilled') out[idx] = r.value as bigint;
      }
    }),
  );
  return out;
}

export async function readDecimals(
  chainId: number,
  token: Address,
): Promise<number | undefined> {
  try {
    const v = await rpcClient(chainId).readContract({
      address: token,
      abi: ERC20_ABI,
      functionName: 'decimals',
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
