/**
 * `/transactions` — tx lifecycle log from the server's `state_deltas`
 * table. Each row is one action attempt: predicted → pending →
 * confirmed / failed (or `historical` when discovered via backfill).
 */

import type { Address, TxRow } from "@scopeball/types";

import { request } from "./client";

export type { TxRow };

/** `GET /transactions?wallet=<addr>&limit=<n>` — recent tx log. */
export async function listTransactions(
  opts: { wallet?: Address; limit?: number } = {},
): Promise<TxRow[]> {
  const params = new URLSearchParams();
  if (opts.wallet) params.set("wallet", opts.wallet);
  if (opts.limit) params.set("limit", String(opts.limit));
  const qs = params.toString();
  return request<TxRow[]>(`/transactions${qs ? `?${qs}` : ""}`);
}
