/**
 * Shared TypeScript types matching the scopeball policy-rpc server's
 * Rust DTOs. Hand-written today; the Rust side uses `tsify_next` for
 * the policy-state types, so a future build step could generate
 * these instead.
 *
 * Group conventions:
 * - `Wallet*` mirrors `policy_state::wallet::WalletState`
 * - `Token*` mirrors `policy_state::token::*`
 * - `Policy*` is legacy server API shape; policy management is extension-local
 * - `Verdict*` is the audit/history model
 */

// ---------- primitives ----------

export type Address = string; // lowercase 0x... 40 hex
export type ChainId = string; // CAIP-2, e.g. "eip155:1"
export type Decimal = string; // decimal as string, never f64
export type UnixSeconds = number;

// ---------- wallet identity ----------

export interface WalletId {
  address: Address;
  chains: ChainId[];
}

export interface BlockHeight {
  number: number;
  time: UnixSeconds;
}

// ---------- token holdings ----------

export interface TokenMetadata {
  logo_url?: string;
  website_url?: string;
  description?: string;
  coingecko_id?: string;
}

export interface LiveFieldPrice {
  value: Decimal;
  synced_at: UnixSeconds;
  ttl_sec: number;
  confidence_bp?: number;
  source: unknown; // DataSource union — opaque on the FE side
}

export interface Balance {
  form: "fungible" | "owned";
  amount?: Decimal;
}

/**
 * Mirrors `TokenHolding`. `value_usd` and `key` are server-computed view
 * fields populated by `WalletState::populate_computed_values()`.
 */
export interface TokenHolding {
  key: unknown; // TokenKey enum — opaque; consumers usually only need symbol/chain
  kind: unknown; // TokenKind
  symbol: string;
  decimals: number;
  balance: Balance;
  committed: Balance;
  approved_to?: Address;
  price_usd?: LiveFieldPrice;
  metadata?: TokenMetadata;
  value_usd?: Decimal;
  last_synced_at: UnixSeconds;
  primitives_source: unknown;
}

// ---------- wallet state ----------

export interface WalletState {
  wallet_id: WalletId;
  /** Serialized as Array<[TokenKey, TokenHolding]>; consumers usually iterate. */
  tokens: Array<[unknown, TokenHolding]>;
  approvals: unknown; // ApprovalSet — opaque; use the holdings/approvals helpers
  positions: unknown[];
  pending: unknown[];
  block_heights: Array<[ChainId, BlockHeight]>;
  /** Server-computed; absent when no holding has a USD price. */
  portfolio_value_usd?: Decimal;
}

// ---------- token catalog ----------

export interface TokenCatalogRow {
  token_hash: string;
  key: unknown;
  symbol: string | null;
  decimals: number | null;
  first_seen_at: UnixSeconds;
  logo_url?: string;
  website_url?: string;
  description?: string;
  coingecko_id?: string;
  metadata_synced_at?: UnixSeconds;
}

// ---------- transactions / state_deltas ----------

export interface TxRow {
  id: number;
  source: "live" | "backfill";
  status: string;
  created_at: UnixSeconds;
  signed_at: UnixSeconds | null;
  confirmed_at: UnixSeconds | null;
  action_domain: string;
  action_kind: string;
  submitter: string;
  tx_hash: string | null;
  predicted_verdict: string | null;
  action: unknown;
  predicted_delta: unknown | null;
  realized_delta: unknown | null;
}

// ---------- policies ----------

export type PolicySeverity = "deny" | "warn" | "info";

export interface InstalledPolicy {
  id: number;
  name: string;
  description: string | null;
  cedar_text: string;
  severity: PolicySeverity;
  enabled: boolean;
  created_at: UnixSeconds;
  updated_at: UnixSeconds;
}

// ---------- auth ----------

export interface AuthUser {
  user_id: string;
  email: string;
}

// ---------- i18n (Decision #8) ----------

/** Server convention: every user-facing string ships both locales. */
export interface I18nString {
  ko: string;
  en: string;
}

// ---------- verdict model ----------

export type Verdict = "pass" | "warn" | "fail";

export interface VerdictRow {
  /** UUID string assigned by the SW at append time (replaces the old DB autoincrement). */
  id: string;
  delta_id: number;
  policy_id: number | null;
  severity: PolicySeverity;
  verdict: Verdict;
  ts: UnixSeconds;
  wallet: Address;
  dapp_origin: string | null;
  decoded_fn: string | null;
  method: string | null;
  contract?: { addr: Address; symbol: string | null };
  selector?: { sig: string; decoded: string | null };
  policy?: { id: number; name: string; severity: PolicySeverity };
  reason: I18nString;
  user_decision: "trusted" | "cancelled" | null;
}

// ---------- dashboard summary ----------

export interface DashboardSummary {
  wallet_count: number;
  total_portfolio_usd: Decimal;
  chain_breakdown: Array<{ chain: ChainId; usd: Decimal; pct: number }>;
  wallets: Array<{
    id: number;
    address: Address;
    label: string | null;
    total_usd: Decimal;
    unlimited_count: number;
    pending_count: number;
  }>;
  // `unresolved_findings` was removed when the verdict log moved to
  // chrome.storage.local. See `dashboard.ts` for the migration note.
}
