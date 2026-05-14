import { RpcMethodError } from "./types.js";

const coinGeckoPlatforms = new Map<number, string>([
  [1, "ethereum"],
  [10, "optimistic-ethereum"],
  [56, "binance-smart-chain"],
  [137, "polygon-pos"],
  [8453, "base"],
  [42161, "arbitrum-one"],
]);

const wrappedNativeAddresses = new Map<number, string>([
  [1, "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2"],
  [10, "0x4200000000000000000000000000000000000006"],
  [56, "0xbb4cdb9cbd36b01bd1cbaebf2de08d9173bc095c"],
  [137, "0x0d500b1d8e8ef31e21c99d1db9a6444d3adf1270"],
  [8453, "0x4200000000000000000000000000000000000006"],
  [42161, "0x82af49447d8a07e3bd95bd0d56f35241523fbab1"],
]);

export function coinGeckoPlatformForChain(chainId: number): string {
  const platform = coinGeckoPlatforms.get(chainId);

  if (!platform) {
    throw new RpcMethodError("unsupported_chain", `Unsupported chain_id ${chainId}`);
  }

  return platform;
}

export function wrappedNativeAddressForChain(chainId: number): string {
  const address = wrappedNativeAddresses.get(chainId);

  if (!address) {
    throw new RpcMethodError(
      "unsupported_chain",
      `Unsupported native asset chain_id ${chainId}`,
    );
  }

  return address;
}
