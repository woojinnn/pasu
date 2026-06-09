/**
 * `/dashboard/summary` — workspace aggregate (Home + Monitoring L1).
 *
 * Single round-trip: total USD across every tracked wallet, per-chain /
 * per-venue breakdown, per-wallet badges (unlimited approvals + pending tx counts).
 */

import type { Address, ChainId, Decimal } from "./types";

import { request } from "./client";

export interface ChainShare {
  chain: ChainId;
  usd: Decimal;
  /** 0–100, share of the total. */
  pct: number;
}

export interface VenueShare {
  venue: string;
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
  total_portfolio_usd: Decimal;
  chain_breakdown: ChainShare[];
  venue_breakdown: VenueShare[];
  wallets: DashboardWalletSummary[];
  // `unresolved_findings` was removed when the verdict log moved to
  // chrome.storage.local. The dashboard now reads that counter directly via
  // `verdicts:count`. Pages that previously displayed it should call
  // `getAuditCounts({ verdict: "warn" })` and filter for `user_decision === null`.
}

export async function getDashboardSummary(): Promise<DashboardSummary> {
  return request<DashboardSummary>("/dashboard/summary");
}
