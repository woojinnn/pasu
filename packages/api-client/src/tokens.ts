/**
 * `/tokens` — global token catalog (every token row known to the
 * user's DB, with CoinGecko-sourced metadata when available).
 */

import type { TokenCatalogRow } from "@scopeball/types";

import { request } from "./client";

export type { TokenCatalogRow };

/** `GET /tokens` — every token row in the catalog. */
export async function listTokens(): Promise<TokenCatalogRow[]> {
  return request<TokenCatalogRow[]>("/tokens");
}
