import { chainConfig } from '@background/chains/chain-config';

const COINGECKO_BASE = 'https://api.coingecko.com/api/v3';
const MAX_BATCH = 30;

export interface CoinGeckoPrice {
  /** Lowercased token contract address. */
  address: string;
  /** USD price as a decimal string. */
  usd: string;
  /** Server-reported `last_updated_at` (unix seconds) if available. */
  asOfTs: number;
}

export interface CoinGeckoNativePrice {
  chainId: number;
  usd: string;
  asOfTs: number;
}

/**
 * Fetch USD prices for ERC-20 tokens on one chain. Tokens absent from
 * CoinGecko are simply omitted from the result (never represented as 0).
 * Network failures and HTTP errors return empty arrays — never throws.
 */
export async function fetchUsdPrices(
  chainId: number,
  addresses: readonly string[],
  fetchImpl: typeof fetch = fetch,
): Promise<readonly CoinGeckoPrice[]> {
  if (addresses.length === 0) return [];
  const platform = chainConfig(chainId).coingeckoPlatform;
  const out: CoinGeckoPrice[] = [];

  // Dedupe input to avoid wasting CoinGecko free-tier rate budget.
  const unique = Array.from(new Set(addresses.map((a) => a.toLowerCase())));

  for (let i = 0; i < unique.length; i += MAX_BATCH) {
    const slice = unique.slice(i, i + MAX_BATCH);
    const url = new URL(`${COINGECKO_BASE}/simple/token_price/${platform}`);
    url.searchParams.set('contract_addresses', slice.join(','));
    url.searchParams.set('vs_currencies', 'usd');
    url.searchParams.set('include_last_updated_at', 'true');

    let response: Response;
    try {
      response = await fetchImpl(url.toString(), {
        signal: AbortSignal.timeout(5_000),
      });
    } catch {
      continue;
    }
    if (!response.ok) continue;

    let body: Record<string, { usd?: number; last_updated_at?: number }>;
    try {
      body = (await response.json()) as Record<
        string,
        { usd?: number; last_updated_at?: number }
      >;
    } catch {
      continue;
    }

    for (const [address, entry] of Object.entries(body)) {
      if (typeof entry?.usd !== 'number') continue;
      out.push({
        address: address.toLowerCase(),
        usd: entry.usd.toString(),
        asOfTs: entry.last_updated_at ?? Math.floor(Date.now() / 1000),
      });
    }
  }
  return out;
}

/**
 * Fetch USD prices for native chain assets via /simple/price?ids=. Each
 * chain's native coin id comes from `chainConfig(chainId).coingeckoNativeId`.
 * Multiple chains can share a coin id (op + base both use 'ethereum').
 */
export async function fetchNativeUsdPrices(
  chainIds: readonly number[],
  fetchImpl: typeof fetch = fetch,
): Promise<readonly CoinGeckoNativePrice[]> {
  if (chainIds.length === 0) return [];
  const idsByCoin = new Map<string, number[]>();
  for (const cid of chainIds) {
    const id = chainConfig(cid).coingeckoNativeId;
    const list = idsByCoin.get(id) ?? [];
    list.push(cid);
    idsByCoin.set(id, list);
  }
  const ids = [...idsByCoin.keys()];

  const url = new URL(`${COINGECKO_BASE}/simple/price`);
  url.searchParams.set('ids', ids.join(','));
  url.searchParams.set('vs_currencies', 'usd');
  url.searchParams.set('include_last_updated_at', 'true');

  let response: Response;
  try {
    response = await fetchImpl(url.toString(), { signal: AbortSignal.timeout(5_000) });
  } catch {
    return [];
  }
  if (!response.ok) return [];
  let body: Record<string, { usd?: number; last_updated_at?: number }>;
  try {
    body = (await response.json()) as Record<
      string,
      { usd?: number; last_updated_at?: number }
    >;
  } catch {
    return [];
  }
  const out: CoinGeckoNativePrice[] = [];
  for (const [coinId, entry] of Object.entries(body)) {
    if (typeof entry?.usd !== 'number') continue;
    const cidsForCoin = idsByCoin.get(coinId) ?? [];
    for (const cid of cidsForCoin) {
      out.push({
        chainId: cid,
        usd: entry.usd.toString(),
        asOfTs: entry.last_updated_at ?? Math.floor(Date.now() / 1000),
      });
    }
  }
  return out;
}
