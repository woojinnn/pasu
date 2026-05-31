/**
 * `/tokens` — global token catalog (every token row known to the
 * user's DB, with CoinGecko-sourced metadata when available).
 */

import { request } from "./client";

export interface TokenCatalogRow {
  token_hash: string; // 0x… 32-hex
  key: unknown; // TokenKey enum
  symbol: string | null;
  decimals: number | null;
  first_seen_at: number;
  logo_url?: string;
  website_url?: string;
  description?: string;
  coingecko_id?: string;
  metadata_synced_at?: number;
}

/** `GET /tokens` — every token row in the catalog. */
export async function listTokens(): Promise<TokenCatalogRow[]> {
  return request<TokenCatalogRow[]>("/tokens");
}
