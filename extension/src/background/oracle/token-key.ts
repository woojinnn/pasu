export interface TokenKeyParts {
  readonly chainId: number;
  readonly address: string;
  readonly isNative?: boolean;
}

export function tokenKey({ chainId, address }: TokenKeyParts): string {
  return `${chainId}:${address.toLowerCase()}`;
}

export function nativeFallbackTokenKey(chainId: number): string {
  // CoinGecko native-price calls can be made with only chain IDs, so there is
  // no request token address to normalize into the canonical tokenKey shape.
  return `${chainId}:native`;
}
