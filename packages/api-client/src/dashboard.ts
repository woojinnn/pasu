/**
 * `/dashboard/summary` — workspace aggregate (Home + Monitoring L1).
 *
 * Single round-trip: total USD across every tracked wallet, per-chain
 * breakdown, per-wallet badges (unlimited approvals + pending tx
 * counts), policy + unresolved-finding counters.
 */

import type { Address, ChainId, Decimal } from "@scopeball/types";

import { request } from "./client";

export interface ChainShare {
  chain: ChainId;
  usd: Decimal;
  /** 0–100, share of the total. */
  pct: number;
}

export interface DashboardWalletSummary {
  id: number;
  address: Address;
  label: string | null;
  total_usd: Decimal;
  unlimited_count: number;
  pending_count: number;
}

export interface DashboardSummary {
  wallet_count: number;
  policy_count: number;
  total_portfolio_usd: Decimal;
  chain_breakdown: ChainShare[];
  wallets: DashboardWalletSummary[];
  /** warn-level verdicts the user hasn't yet trusted/cancelled. */
  unresolved_findings: number;
}

export async function getDashboardSummary(): Promise<DashboardSummary> {
  return request<DashboardSummary>("/dashboard/summary");
}
