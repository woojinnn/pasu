/**
 * Typed wrappers for `/wallets/...` server endpoints.
 *
 * All routes are authenticated — `client.request()` attaches the stored
 * JWT automatically. The TypeScript shapes mirror the Rust DTOs in
 * `crates/policy-server/server/src/read_handlers.rs` and the underlying
 * `simulation-state` types.
 */

import { request } from "./client";
import type { Address, ChainId, TokenHolding } from "./types";

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

export interface AddWalletBody {
  /** 0x address (case-insensitive). */
  address: Address;
  /** CAIP-2 list (e.g. `["eip155:1"]`). Omit to track on every configured chain. */
  chains?: ChainId[];
  label?: string;
}

export interface AddWalletResp {
  wallet_id: WalletId;
  /** True when the auto-sync after add succeeded. */
  synced: boolean;
  /** How many TokenHolding rows were seeded for a brand-new wallet. */
  discovered: number;
  /** Non-fatal sync error message — caller can retry with /sync. */
  error?: string;
}

/** `GET /wallets` — every wallet the authenticated user has. */
export async function listWallets(): Promise<WalletId[]> {
  return request<WalletId[]>("/wallets");
}

/** `POST /wallets` — start tracking a new wallet for the authenticated user. */
export async function addWallet(body: AddWalletBody): Promise<AddWalletResp> {
  return request<AddWalletResp>("/wallets", { method: "POST", body });
}

/** `POST /wallets/:addr/sync` — manual resync (balance + price refresh). */
export async function syncWallet(address: string): Promise<void> {
  await request<void>(`/wallets/${address}/sync`, { method: "POST" });
}

/** `GET /wallets/:addr/state` — full state snapshot. */
export async function getWalletState(address: string): Promise<WalletStateView> {
  return request<WalletStateView>(`/wallets/${address}/state`);
}

/** `GET /wallets/:addr/holdings` — token holdings array (each with value_usd). */
export async function getWalletHoldings(address: string): Promise<TokenHolding[]> {
  return request<TokenHolding[]>(`/wallets/${address}/holdings`);
}

/** `GET /wallets/:addr/approvals` — full approval set (ERC20 + setForAll + Permit2). */
export async function getWalletApprovals(address: string): Promise<unknown> {
  return request<unknown>(`/wallets/${address}/approvals`);
}

/** Server risk tags. KNOWN_VENUE / BLOCKED depended on the now-removed
 *  spender label catalog; today the server emits UNLIMITED / OLD /
 *  EXPIRED only. Kept in the union for forward-compat in case the
 *  registry-driven catalog comes back. */
export type ApprovalRisk =
  | "UNLIMITED"
  | "KNOWN_VENUE"
  | "BLOCKED"
  | "OLD"
  | "EXPIRED";

export interface ClassifiedErc20Approval {
  chain: ChainId;
  token: Address;
  spender: Address;
  amount: string;
  is_unlimited: boolean;
  last_set_at: number;
  risk: ApprovalRisk[];
}

export interface ClassifiedSetForAllApproval {
  chain: ChainId;
  collection: Address;
  operator: Address;
  risk: ApprovalRisk[];
}

export interface ClassifiedPermit2Approval {
  chain: ChainId;
  token: Address;
  spender: Address;
  amount: string;
  expiration: number;
  nonce: number;
  risk: ApprovalRisk[];
}

export interface ClassifiedApprovals {
  erc20: ClassifiedErc20Approval[];
  set_for_all: ClassifiedSetForAllApproval[];
  permit2: ClassifiedPermit2Approval[];
}

/** `GET /wallets/:addr/approvals?with_risk=true` — server-classified shape with risk tags. */
export async function getWalletApprovalsWithRisk(
  address: string,
): Promise<ClassifiedApprovals> {
  return request<ClassifiedApprovals>(`/wallets/${address}/approvals?with_risk=true`);
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
