/**
 * `/transactions` — tx lifecycle log from the server's `state_deltas`
 * table. Each row is one action attempt: predicted → pending →
 * confirmed / failed (or `historical` when discovered via backfill).
 */

import { request } from "./client";

export interface TxRow {
  id: number;
  source: string; // "live" | "backfill"
  status: string; // "predicted" | "pending" | "confirmed" | "failed" | "historical"
  created_at: number;
  signed_at: number | null;
  confirmed_at: number | null;
  action_domain: string;
  action_kind: string;
  submitter: string;
  tx_hash: string | null;
  predicted_verdict: string | null;
  action: unknown;
  predicted_delta: unknown | null;
  realized_delta: unknown | null;
}

/** `GET /transactions?wallet=<addr>&limit=<n>` — recent tx log. */
export async function listTransactions(
  opts: { wallet?: string; limit?: number } = {},
): Promise<TxRow[]> {
  const params = new URLSearchParams();
  if (opts.wallet) params.set("wallet", opts.wallet);
  if (opts.limit) params.set("limit", String(opts.limit));
  const qs = params.toString();
  return request<TxRow[]>(`/transactions${qs ? `?${qs}` : ""}`);
}
