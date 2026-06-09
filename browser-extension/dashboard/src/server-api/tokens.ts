/**
 * `/tokens` — global token catalog (every token row known to the
 * user's DB, with CoinGecko-sourced metadata when available).
 */

import { request } from "./client";
import type { TokenCatalogRow } from "./types";

export type { TokenCatalogRow };

/** `GET /tokens` — every token row in the catalog. */
export async function listTokens(): Promise<TokenCatalogRow[]> {
  return request<TokenCatalogRow[]>("/tokens");
}

/** The ERC20 address / NFT contract of a token `key`, lowercased — or `null`
 *  for the native gas asset / unparseable keys. `key` is the opaque TokenKey
 *  the server serialises (serde tag `standard`, snake_case): ERC20 carries
 *  `address`, ERC721/1155 carry `contract`. */
export function tokenAddress(key: unknown): string | null {
  if (!key || typeof key !== "object") return null;
  const k = key as { address?: unknown; contract?: unknown };
  const raw = typeof k.address === "string" ? k.address : typeof k.contract === "string" ? k.contract : null;
  return raw ? raw.toLowerCase() : null;
}
