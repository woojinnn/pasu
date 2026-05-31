/**
 * Typed wrappers for `/wallets/...` server endpoints.
 *
 * All routes are authenticated — `client.request()` attaches the stored
 * JWT automatically. Response shapes live in `@scopeball/types`.
 */

import type {
  Address,
  BlockHeight,
  ChainId,
  TokenHolding,
  WalletId,
  WalletState,
} from "@scopeball/types";

import { request } from "./client";

/** `GET /wallets` — every wallet the authenticated user has. */
export async function listWallets(): Promise<WalletId[]> {
  return request<WalletId[]>("/wallets");
}

/** `GET /wallets/:addr/state` — full state snapshot (with portfolio_value_usd). */
export async function getWalletState(address: Address): Promise<WalletState> {
  return request<WalletState>(`/wallets/${address}/state`);
}

/** `GET /wallets/:addr/holdings` — token holdings array (each with value_usd). */
export async function getWalletHoldings(address: Address): Promise<TokenHolding[]> {
  return request<TokenHolding[]>(`/wallets/${address}/holdings`);
}

/** `GET /wallets/:addr/approvals` — full approval set (ERC20 + setForAll + Permit2). */
export async function getWalletApprovals(address: Address): Promise<unknown> {
  return request<unknown>(`/wallets/${address}/approvals`);
}

/** `GET /wallets/:addr/block-heights` — per-chain block height list. */
export async function getWalletBlockHeights(
  address: Address,
): Promise<Array<{ chain: ChainId } & BlockHeight>> {
  return request<Array<{ chain: ChainId } & BlockHeight>>(
    `/wallets/${address}/block-heights`,
  );
}

/** `PATCH /wallets/:addr` — partial update. `label: null` clears. */
export async function patchWallet(
  address: Address,
  patch: { label?: string | null; is_owned?: boolean },
): Promise<void> {
  await request<void>(`/wallets/${address}`, { method: "PATCH", body: patch });
}

/** `DELETE /wallets/:addr` — soft delete (archive). */
export async function deleteWallet(address: Address): Promise<void> {
  await request<void>(`/wallets/${address}`, { method: "DELETE" });
}

// Re-export types for callers that imported them from "./wallets" before.
export type { WalletId, BlockHeight, WalletState };
