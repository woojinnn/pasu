/**
 * Typed wrappers for the wallet `positions` / `pending` views.
 *
 * Shapes mirror the Rust `policy_state` types (Tsify-exported in
 * `backend/wasm/policy_engine_wasm.d.ts`) and the handlers in
 * `crates/policy-server/server/src/read_handlers.rs`
 * (`get_positions` / `get_pending`). Hyperliquid is fully typed (the current
 * focus); the other position kinds are kept loose until Phase 4 surfaces them.
 */

import { request } from "./client";
import type { ChainId, Decimal } from "./types";

// ── Hyperliquid account ──────────────────────────────────────────────────

/** A filled HL perp position. */
export interface HlPosition {
  asset_index: number;
  symbol?: string;
  is_long: boolean;
  size: Decimal;
  entry_price: Decimal;
}

/** A resting (unfilled) HL order, incl. trigger TP/SL. */
export interface HlOpenOrder {
  asset_index: number;
  symbol?: string;
  is_buy: boolean;
  price: Decimal;
  size: Decimal;
  reduce_only: boolean;
  tif: string;
  oid?: number;
  order_type?: string;
  is_trigger?: boolean;
  trigger_price?: Decimal;
  /** A whole-position TP/SL — HL reports `size: 0` because it closes the
   *  entire position at trigger time (the size tracks the position). */
  is_position_tpsl?: boolean;
}

/** Per-asset leverage configuration (the *setting*, not the live position
 *  leverage). */
export interface HlLeverageSetting {
  asset_index: number;
  is_cross: boolean;
  leverage: number;
}

/** The HL L1 account snapshot. Sub-domains (spot/staking/vaults/borrow-lend)
 *  are kept loose — surfaced later as the UI grows. */
export interface HlAccount {
  /** Perp margin balance (USDC). */
  perp_usdc?: Decimal;
  pending_outflow: Decimal;
  positions: HlPosition[];
  open_orders: HlOpenOrder[];
  leverage_settings: HlLeverageSetting[];
  /** Authorized agent (API) wallets — a security-relevant surface. */
  agents: unknown[];
  spot_balances?: unknown[];
  staking?: unknown;
  vault_equities?: unknown[];
  borrow_lend?: unknown;
}

// ── Position (discriminated on `kind`) ───────────────────────────────────

/** The Hyperliquid variant of `PositionKind` — fully typed. */
export type HlPositionKind = { kind: "hyperliquid_account" } & HlAccount;

/** Other position kinds (lending/perp/airdrop/launchpad/vesting) — loose
 *  until Phase 4. Narrow on `kind` and read fields as needed. */
export interface OtherPositionKind {
  kind:
    | "perp_position"
    | "lending_account"
    | "airdrop_claim"
    | "launchpad_allocation"
    | "vesting_schedule";
  [field: string]: unknown;
}

export type PositionKind = HlPositionKind | OtherPositionKind;

/** A protocol position as returned by `GET /wallets/:addr/positions`. */
export interface Position {
  id: unknown; // PositionId
  protocol: unknown; // ProtocolRef
  chain?: ChainId;
  kind: PositionKind;
  primitives_synced_at: number;
  primitives_source: unknown;
}

// ── Pending (off-chain / unsettled intent orders) ────────────────────────

/** What a pending entry commits — discriminated on `kind`. Scalar fields are
 *  typed; `TokenRef`/`VenueRef`/`MarketRef`/`OrderKind` serialize as objects and
 *  are kept loose (the UI extracts a label best-effort). */
export type PendingKind =
  | {
      kind: "offchain_limit_order";
      venue: unknown;
      sell: unknown;
      buy: unknown;
      sell_max: string;
      buy_min: string;
      order_kind: unknown;
    }
  | {
      kind: "perp_venue_order";
      venue: unknown;
      market: unknown;
      side: string;
      size_base: string;
      price: string;
      order_kind: unknown;
      reduce_only: boolean;
    }
  | { kind: "signed_permit2"; token: unknown; spender: string; amount: string; expires_at: number }
  | {
      kind: "signed_permit2_transfer";
      token: unknown;
      owner: string;
      spender: string;
      amount: string;
      expires_at: number;
    }
  | { kind: "signed_e_i_p2612"; token: unknown; spender: string; amount: string; expires_at: number };

/** An off-chain / unsettled intent order (UniswapX, CoW, 1inch Fusion) or a
 *  signed permit. `commitment`/`lifecycle` stay loose; the UI reads what it
 *  renders. */
export interface PendingTx {
  id: unknown; // PendingId
  kind: PendingKind;
  commitment: unknown; // AssetCommitment
  lifecycle: unknown; // PendingLifecycle
  signed_at: number;
  [field: string]: unknown;
}

// ── Fetchers ─────────────────────────────────────────────────────────────

/** `GET /wallets/:addr/positions` — protocol positions (incl. the HL account). */
export async function getWalletPositions(address: string): Promise<Position[]> {
  return request<Position[]>(`/wallets/${address}/positions`);
}

/** `GET /wallets/:addr/pending` — off-chain / unsettled intent orders. */
export async function getWalletPending(address: string): Promise<PendingTx[]> {
  return request<PendingTx[]>(`/wallets/${address}/pending`);
}

/** Extract the single HL account from a positions array, if present. */
export function hlAccountOf(positions: Position[]): HlAccount | null {
  for (const p of positions) {
    if (p.kind.kind === "hyperliquid_account") return p.kind;
  }
  return null;
}
