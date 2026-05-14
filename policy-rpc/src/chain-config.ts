import { RpcMethodError } from "./types";

const coinGeckoPlatforms = new Map<number, string>([
  [1, "ethereum"],
  [10, "optimistic-ethereum"],
  [56, "binance-smart-chain"],
  [137, "polygon-pos"],
  [8453, "base"],
  [42161, "arbitrum-one"],
]);

export function coinGeckoPlatformForChain(chainId: number): string {
  const platform = coinGeckoPlatforms.get(chainId);

  if (!platform) {
    throw new RpcMethodError("unsupported_chain", `Unsupported chain_id ${chainId}`);
  }

  return platform;
}
