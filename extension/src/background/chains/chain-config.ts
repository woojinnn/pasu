import type { Chain } from "viem";
import { arbitrum, base, mainnet, optimism, polygon } from "viem/chains";

const runtime = globalThis as typeof globalThis & {
  process?: { env?: Record<string, string | undefined> };
};
const ALCHEMY_KEY = runtime.process?.env?.ALCHEMY_API_KEY ?? "";

export interface ChainConfig {
  id: number;
  viem: Chain;
  /** Ordered list of RPC URLs; clients fall through on failure. */
  rpcUrls: string[];
  /** Multicall3 address (default `0xcA11bde05977b3631167028862bE2a173976CA11` on every supported chain). */
  multicall3: `0x${string}`;
  /** CoinGecko platform slug for /simple/token_price/{platform}. */
  coingeckoPlatform: string;
  /** CoinGecko coin id for /simple/price (native asset; ETH uses 'ethereum'). */
  coingeckoNativeId: string;
}

const MULTICALL3 = "0xcA11bde05977b3631167028862bE2a173976CA11" as const;

function withAlchemyOrFallback(alchemyTpl: string, free: string): string[] {
  const main = ALCHEMY_KEY ? alchemyTpl.replace("${KEY}", ALCHEMY_KEY) : "";
  return main ? [main, free] : [free];
}

export const CHAINS: Record<number, ChainConfig> = {
  1: {
    id: 1,
    viem: mainnet,
    rpcUrls: withAlchemyOrFallback(
      "https://eth-mainnet.g.alchemy.com/v2/${KEY}",
      "https://eth.llamarpc.com",
    ),
    multicall3: MULTICALL3,
    coingeckoPlatform: "ethereum",
    coingeckoNativeId: "ethereum",
  },
  10: {
    id: 10,
    viem: optimism,
    rpcUrls: withAlchemyOrFallback(
      "https://opt-mainnet.g.alchemy.com/v2/${KEY}",
      "https://mainnet.optimism.io",
    ),
    multicall3: MULTICALL3,
    coingeckoPlatform: "optimistic-ethereum",
    coingeckoNativeId: "ethereum",
  },
  137: {
    id: 137,
    viem: polygon,
    rpcUrls: withAlchemyOrFallback(
      "https://polygon-mainnet.g.alchemy.com/v2/${KEY}",
      "https://polygon-rpc.com",
    ),
    multicall3: MULTICALL3,
    coingeckoPlatform: "polygon-pos",
    coingeckoNativeId: "matic-network",
  },
  8453: {
    id: 8453,
    viem: base,
    rpcUrls: withAlchemyOrFallback(
      "https://base-mainnet.g.alchemy.com/v2/${KEY}",
      "https://mainnet.base.org",
    ),
    multicall3: MULTICALL3,
    coingeckoPlatform: "base",
    coingeckoNativeId: "ethereum",
  },
  42161: {
    id: 42161,
    viem: arbitrum,
    rpcUrls: withAlchemyOrFallback(
      "https://arb-mainnet.g.alchemy.com/v2/${KEY}",
      "https://arb1.arbitrum.io/rpc",
    ),
    multicall3: MULTICALL3,
    coingeckoPlatform: "arbitrum-one",
    coingeckoNativeId: "ethereum",
  },
};

export function chainConfig(chainId: number): ChainConfig {
  const c = CHAINS[chainId];
  if (!c) throw new Error(`Unsupported chainId: ${chainId}`);
  return c;
}

export function isChainSupported(chainId: number): boolean {
  return chainId in CHAINS;
}
