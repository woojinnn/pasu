import { chainConfig } from "../chains/chain-config";
import { nativeFallbackTokenKey, tokenKey } from "./token-key";

const COINGECKO_BASE = "https://api.coingecko.com/api/v3";
const MAX_BATCH = 30;

interface OracleFetchErrorOptions {
  readonly status?: number;
  readonly cause?: unknown;
  readonly tokenKeys?: readonly string[];
}

export class OracleFetchError extends Error {
  readonly status: number | undefined;
  readonly cause: unknown;
  readonly tokenKeys: readonly string[] | undefined;

  constructor(
    message: string,
    opts: OracleFetchErrorOptions = {},
  ) {
    super(message);
    this.name = "OracleFetchError";
    this.status = opts.status;
    this.cause = opts.cause;
    this.tokenKeys = opts.tokenKeys;
  }
}

const tokenUpdatedAt = new WeakMap<
  ReadonlyMap<string, number>,
  Map<string, number>
>();
const nativeUpdatedAt = new WeakMap<
  ReadonlyMap<number, number>,
  Map<number, number>
>();

export function priceLastUpdatedAt(
  prices: ReadonlyMap<string, number>,
  address: string,
): number | undefined {
  return tokenUpdatedAt.get(prices)?.get(address.toLowerCase());
}

export function nativePriceLastUpdatedAt(
  prices: ReadonlyMap<number, number>,
  chainId: number,
): number | undefined {
  return nativeUpdatedAt.get(prices)?.get(chainId);
}

function unixSecondsToMs(value: number | undefined): number {
  return typeof value === "number" ? value * 1000 : Date.now();
}

function tokenKeysForAddresses(
  chainId: number,
  addresses: readonly string[],
): string[] {
  return addresses.map((address) => tokenKey({ chainId, address }));
}

async function responseErrorCause(response: Response): Promise<unknown> {
  try {
    const text = await response.text();
    return text || response.statusText || `HTTP ${response.status}`;
  } catch (cause) {
    return cause;
  }
}

/**
 * Fetch USD prices for ERC-20 tokens on one chain. Results are keyed by
 * lowercased contract address.
 *
 * @throws OracleFetchError on network failure or non-2xx response; resolves
 * with the price map on success.
 */
export async function fetchUsdPrices(
  chainId: number,
  addresses: readonly string[],
  fetchImpl: typeof fetch = fetch,
): Promise<Map<string, number>> {
  const out = new Map<string, number>();
  const updated = new Map<string, number>();
  tokenUpdatedAt.set(out, updated);

  if (addresses.length === 0) return out;
  const unique = [...new Set(addresses.map((address) => address.toLowerCase()))];
  const allTokenKeys = tokenKeysForAddresses(chainId, unique);
  let platform: string;

  try {
    platform = chainConfig(chainId).coingeckoPlatform;
  } catch (cause) {
    throw new OracleFetchError("CoinGecko token price fetch failed", {
      cause,
      tokenKeys: allTokenKeys,
    });
  }

  for (let i = 0; i < unique.length; i += MAX_BATCH) {
    const batch = unique.slice(i, i + MAX_BATCH);
    const tokenKeys = tokenKeysForAddresses(chainId, batch);
    const url = new URL(`${COINGECKO_BASE}/simple/token_price/${platform}`);
    url.searchParams.set("contract_addresses", batch.join(","));
    url.searchParams.set("vs_currencies", "usd");
    url.searchParams.set("include_last_updated_at", "true");

    let response: Response;
    try {
      response = await fetchImpl(url.toString(), {
        signal: AbortSignal.timeout(5_000),
      });
    } catch (cause) {
      throw new OracleFetchError("CoinGecko token price fetch failed", {
        cause,
        tokenKeys,
      });
    }

    if (!response.ok) {
      throw new OracleFetchError("CoinGecko token price fetch failed", {
        status: response.status,
        cause: await responseErrorCause(response),
        tokenKeys,
      });
    }

    let body: Record<string, { usd?: number; last_updated_at?: number }>;
    try {
      body = (await response.json()) as Record<
        string,
        { usd?: number; last_updated_at?: number }
      >;
    } catch (cause) {
      throw new OracleFetchError("CoinGecko token price response was invalid", {
        cause,
        tokenKeys,
      });
    }

    for (const [address, entry] of Object.entries(body)) {
      if (typeof entry.usd !== "number") continue;
      const lower = address.toLowerCase();
      out.set(lower, entry.usd);
      updated.set(lower, unixSecondsToMs(entry.last_updated_at));
    }
  }
  return out;
}

/**
 * Fetch USD prices for native chain assets via /simple/price?ids=. Multiple
 * chains can share one CoinGecko id, so the returned Map is keyed by chain id.
 *
 * @throws OracleFetchError on network failure or non-2xx response; resolves
 * with the price map on success.
 */
export async function fetchNativeUsdPrices(
  chainIds: readonly number[],
  fetchImpl: typeof fetch = fetch,
  // Callers with request-token addresses pass canonical tokenKey values;
  // chain-only native price calls intentionally fall back to "<chainId>:native".
  tokenKeys: readonly string[] = chainIds.map(nativeFallbackTokenKey),
): Promise<Map<number, number>> {
  const out = new Map<number, number>();
  const updated = new Map<number, number>();
  nativeUpdatedAt.set(out, updated);

  if (chainIds.length === 0) return out;
  const idsByCoin = new Map<string, number[]>();

  try {
    for (const chainId of new Set(chainIds)) {
      const coinId = chainConfig(chainId).coingeckoNativeId;
      const chains = idsByCoin.get(coinId) ?? [];
      chains.push(chainId);
      idsByCoin.set(coinId, chains);
    }
  } catch (cause) {
    throw new OracleFetchError("CoinGecko native price fetch failed", {
      cause,
      tokenKeys,
    });
  }

  const url = new URL(`${COINGECKO_BASE}/simple/price`);
  url.searchParams.set("ids", [...idsByCoin.keys()].join(","));
  url.searchParams.set("vs_currencies", "usd");
  url.searchParams.set("include_last_updated_at", "true");

  let response: Response;
  try {
    response = await fetchImpl(url.toString(), {
      signal: AbortSignal.timeout(5_000),
    });
  } catch (cause) {
    throw new OracleFetchError("CoinGecko native price fetch failed", {
      cause,
      tokenKeys,
    });
  }

  if (!response.ok) {
    throw new OracleFetchError("CoinGecko native price fetch failed", {
      status: response.status,
      cause: await responseErrorCause(response),
      tokenKeys,
    });
  }

  let body: Record<string, { usd?: number; last_updated_at?: number }>;
  try {
    body = (await response.json()) as Record<
      string,
      { usd?: number; last_updated_at?: number }
    >;
  } catch (cause) {
    throw new OracleFetchError("CoinGecko native price response was invalid", {
      cause,
      tokenKeys,
    });
  }

  for (const [coinId, entry] of Object.entries(body)) {
    if (typeof entry.usd !== "number") continue;
    const chains = idsByCoin.get(coinId) ?? [];
    for (const chainId of chains) {
      out.set(chainId, entry.usd);
      updated.set(chainId, unixSecondsToMs(entry.last_updated_at));
    }
  }
  return out;
}
