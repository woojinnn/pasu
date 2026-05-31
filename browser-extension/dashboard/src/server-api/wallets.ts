/**
 * Typed wrappers for `/wallets/...` server endpoints.
 *
 * All routes are authenticated — `client.request()` attaches the stored
 * JWT automatically. The TypeScript shapes mirror the Rust DTOs in
 * `crates/simulation/server/src/read_handlers.rs` and the underlying
 * `simulation-state` types.
 */

import { request } from "./client";

/** Mirrors `simulation_state::WalletId` (address + chains set). */
export interface WalletId {
  address: string;
  chains: string[]; // CAIP-2 strings, e.g. "eip155:1"
}

/** Per-chain sync block. */
export interface BlockHeight {
  number: number;
  time: number;
}

/** A `WalletState` row as returned by `GET /wallets/:addr/state`. The
 * shape is the same as the Rust `WalletState` serde output; we keep the
 * sub-types `unknown` for now and rely on the page-level code to read
 * the parts it cares about. */
export interface WalletStateView {
  wallet_id: WalletId;
  tokens: Array<[unknown, unknown]>;
  approvals: unknown;
  positions: unknown[];
  pending: unknown[];
  block_heights: Array<[string, BlockHeight]>;
}

/** `GET /wallets` — every wallet the authenticated user has. */
export async function listWallets(): Promise<WalletId[]> {
  return request<WalletId[]>("/wallets");
}

/** `GET /wallets/:addr/state` — full state snapshot. */
export async function getWalletState(address: string): Promise<WalletStateView> {
  return request<WalletStateView>(`/wallets/${address}/state`);
}

/** `GET /wallets/:addr/holdings` — token holdings array. */
export async function getWalletHoldings(address: string): Promise<unknown[]> {
  return request<unknown[]>(`/wallets/${address}/holdings`);
}

/** `GET /wallets/:addr/approvals` — full approval set (ERC20 + setForAll + Permit2). */
export async function getWalletApprovals(address: string): Promise<unknown> {
  return request<unknown>(`/wallets/${address}/approvals`);
}

/** `GET /wallets/:addr/block-heights` — per-chain block height list. */
export async function getWalletBlockHeights(
  address: string,
): Promise<Array<{ chain: string } & BlockHeight>> {
  return request<Array<{ chain: string } & BlockHeight>>(
    `/wallets/${address}/block-heights`,
  );
}

/** `PATCH /wallets/:addr` — partial update. `label: null` clears. */
export async function patchWallet(
  address: string,
  patch: { label?: string | null; is_owned?: boolean },
): Promise<void> {
  await request<void>(`/wallets/${address}`, { method: "PATCH", body: patch });
}

/** `DELETE /wallets/:addr` — soft delete (archive). */
export async function deleteWallet(address: string): Promise<void> {
  await request<void>(`/wallets/${address}`, { method: "DELETE" });
}
