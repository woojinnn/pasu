/**
 * Typed projection of the WASM-side `StateDelta` envelope into a shape the
 * dashboard can render directly. Pairs with the `OpaqueStateDelta`
 * pass-through type in `sim-bridge.ts` — opaque on the wire, typed at the
 * render boundary.
 *
 * Source of truth: `crates/policy-server/asset-model/state/src/delta/`
 *   - `mod.rs::StateDelta { token_changes, position_changes, pending_changes, gas_paid? }`
 *   - `token_change.rs::TokenChange` — discriminated on `kind`:
 *       `balance_delta { key, delta: SignedI256 as string }`
 *       `approval_set { key, spender, allowance }`
 *       `approval_revoke { key, spender, scope }`
 *   - `position_change.rs::PositionChange` — `open | update | close`
 *   - `pending_change.rs::PendingChange` — `add | update | remove`
 *
 * The view here covers the slice the probe UI actually renders (balance +
 * approval changes, gas paid). Position and pending changes are surfaced as
 * structured rows but their inner payload is kept opaque — the dashboard
 * doesn't model the full Position / PendingTx schema yet.
 */

import type { OpaqueStateDelta } from "./sim-bridge";

/** Fungibility-unit identifier of a token. Mirrors `TokenKey` — three-tuple
 *  `(standard, chain, address)` that uniquely IDs a balance row. */
export interface TokenKey {
  standard: string;
  chain: string;
  address: string;
}

export type ApprovalScope =
  | "erc20"
  | "set_for_all"
  | "permit2"
  | "erc721_token";

/** Balance went up or down by a signed decimal-string amount. Negative
 *  `delta` is a debit. The raw value is base-10 SignedI256, kept as string
 *  to preserve precision. */
export interface BalanceChangeRow {
  kind: "balance_delta";
  key: TokenKey;
  /** Signed base-10 decimal string; `-` prefix for debits. */
  delta: string;
}

/** A new approval was granted (ERC20 approve / setApprovalForAll / Permit2)
 *  or an existing one's allowance was raised. */
export interface ApprovalSetRow {
  kind: "approval_set";
  key: TokenKey;
  spender: string;
  /** Allowance payload kept opaque — `AllowanceSpec` shape varies by scope
   *  (ERC20 vs Permit2 with expiry, etc.) and we don't render its internals
   *  yet. */
  allowance: unknown;
}

/** A previously-granted approval was revoked. */
export interface ApprovalRevokeRow {
  kind: "approval_revoke";
  key: TokenKey;
  spender: string;
  scope: ApprovalScope;
}

export type TokenChangeRow =
  | BalanceChangeRow
  | ApprovalSetRow
  | ApprovalRevokeRow;

/** Position lifecycle event. `inner` carries the engine-side payload
 *  (`Position` for open, `PositionPatch` for update, just an id for close)
 *  and is kept opaque — the probe renders it as JSON until a richer Position
 *  view exists. */
export interface PositionChangeRow {
  kind: "open" | "update" | "close";
  /** Position id for `update` / `close`. `null` for `open` (id only exists
   *  after the engine assigns it; the open payload carries the new Position
   *  inside `inner`). */
  id: string | null;
  inner: unknown;
}

/** Pending-tx lifecycle event. `inner` keeps the `PendingTx` / patch payload
 *  opaque for the same reason as `PositionChangeRow.inner`. */
export interface PendingChangeRow {
  kind: "add" | "update" | "remove";
  id: string | null;
  inner: unknown;
}

/** Gas paid for the action — present only on transaction-shaped actions
 *  (signature-only flows never pay gas). Mirrors `StateDelta.gas_paid:
 *  Option<(TokenRef, U256)>` — the raw `U256` value is kept as a base-10
 *  string to preserve precision. */
export interface GasPaid {
  /** Token used to pay gas. `TokenRef` shape is the same as `TokenKey`
   *  (the engine reuses the type), so we name the field accordingly. */
  token: TokenKey;
  /** Amount paid as a base-10 decimal string. */
  amount: string;
}

/** Renderable projection of one step's `StateDelta`. Empty arrays + `null`
 *  `gasPaid` represent a "no-op" delta (the engine returned `StateDelta::default()`
 *  for the step). */
export interface StateDeltaView {
  tokenChanges: TokenChangeRow[];
  positionChanges: PositionChangeRow[];
  pendingChanges: PendingChangeRow[];
  gasPaid: GasPaid | null;
}

/** Defensive read for optional record fields — pulls `key` off `obj` only
 *  when `obj` is a record. Used to walk the opaque JSON without `any`. */
function pick(obj: unknown, key: string): unknown {
  if (obj && typeof obj === "object" && !Array.isArray(obj)) {
    return (obj as Record<string, unknown>)[key];
  }
  return undefined;
}

function asString(v: unknown): string {
  return typeof v === "string" ? v : "";
}

function asTokenKey(v: unknown): TokenKey {
  return {
    standard: asString(pick(v, "standard")),
    chain: asString(pick(v, "chain")),
    address: asString(pick(v, "address")),
  };
}

function asTokenChange(raw: unknown): TokenChangeRow | null {
  const kind = pick(raw, "kind");
  if (kind === "balance_delta") {
    return {
      kind: "balance_delta",
      key: asTokenKey(pick(raw, "key")),
      delta: asString(pick(raw, "delta")),
    };
  }
  if (kind === "approval_set") {
    return {
      kind: "approval_set",
      key: asTokenKey(pick(raw, "key")),
      spender: asString(pick(raw, "spender")),
      allowance: pick(raw, "allowance"),
    };
  }
  if (kind === "approval_revoke") {
    const scope = pick(raw, "scope");
    return {
      kind: "approval_revoke",
      key: asTokenKey(pick(raw, "key")),
      spender: asString(pick(raw, "spender")),
      scope:
        scope === "erc20" ||
        scope === "set_for_all" ||
        scope === "permit2" ||
        scope === "erc721_token"
          ? scope
          : "erc20",
    };
  }
  return null;
}

function asPositionChange(raw: unknown): PositionChangeRow | null {
  const kind = pick(raw, "kind");
  if (kind === "open") {
    return { kind: "open", id: null, inner: pick(raw, "position") };
  }
  if (kind === "update") {
    return {
      kind: "update",
      id: asString(pick(raw, "id")) || null,
      inner: pick(raw, "patch"),
    };
  }
  if (kind === "close") {
    return {
      kind: "close",
      id: asString(pick(raw, "id")) || null,
      inner: null,
    };
  }
  return null;
}

function asPendingChange(raw: unknown): PendingChangeRow | null {
  const kind = pick(raw, "kind");
  if (kind === "add") {
    return { kind: "add", id: null, inner: pick(raw, "pending") };
  }
  if (kind === "update") {
    return {
      kind: "update",
      id: asString(pick(raw, "id")) || null,
      inner: raw,
    };
  }
  if (kind === "remove") {
    return {
      kind: "remove",
      id: asString(pick(raw, "id")) || null,
      inner: raw,
    };
  }
  return null;
}

function asGasPaid(raw: unknown): GasPaid | null {
  // Serde shape: `[TokenRef, "0x..."]` — a 2-tuple. Some serialization paths
  // also produce `{ token, amount }`; accept both defensively.
  if (Array.isArray(raw) && raw.length === 2) {
    return { token: asTokenKey(raw[0]), amount: asString(raw[1]) };
  }
  if (raw && typeof raw === "object" && !Array.isArray(raw)) {
    return {
      token: asTokenKey(pick(raw, "token")),
      amount: asString(pick(raw, "amount")),
    };
  }
  return null;
}

/**
 * Project the wire-shape `OpaqueStateDelta` JSON into a `StateDeltaView`
 * the UI can iterate. Best-effort: unknown variants are skipped (the
 * dashboard receives a newer engine version than it knows about) rather
 * than throwing, so a partial render still happens.
 */
export function parseStateDelta(opaque: OpaqueStateDelta): StateDeltaView {
  const tokenRaw = pick(opaque, "token_changes");
  const posRaw = pick(opaque, "position_changes");
  const pendRaw = pick(opaque, "pending_changes");
  const gasRaw = pick(opaque, "gas_paid");

  const tokenChanges = (Array.isArray(tokenRaw) ? tokenRaw : [])
    .map(asTokenChange)
    .filter((x): x is TokenChangeRow => x !== null);
  const positionChanges = (Array.isArray(posRaw) ? posRaw : [])
    .map(asPositionChange)
    .filter((x): x is PositionChangeRow => x !== null);
  const pendingChanges = (Array.isArray(pendRaw) ? pendRaw : [])
    .map(asPendingChange)
    .filter((x): x is PendingChangeRow => x !== null);
  const gasPaid = asGasPaid(gasRaw);

  return { tokenChanges, positionChanges, pendingChanges, gasPaid };
}

/** Quick "did anything change at all" check — useful for hiding empty rows
 *  in the timeline rendering. */
export function isStateDeltaEmpty(view: StateDeltaView): boolean {
  return (
    view.tokenChanges.length === 0 &&
    view.positionChanges.length === 0 &&
    view.pendingChanges.length === 0 &&
    view.gasPaid === null
  );
}

/** Short address rendering for compact rows — `0xabcd…1234`. Used both for
 *  spender addresses (approval rows) and token contract addresses. */
export function shortAddr(addr: string): string {
  if (!addr || addr.length < 10) return addr;
  return `${addr.slice(0, 6)}…${addr.slice(-4)}`;
}

/** Format a hex-encoded U256 balance into a human-readable decimal string,
 *  shifting by `decimals` to land on the token's display unit. Trailing
 *  zeros in the fractional part are trimmed; we cap the fraction at 6
 *  digits so the rendered cell stays compact even for 18-decimal tokens.
 *
 *  `formatBalance("0x3b9aca00", 6)` → `"1,000"` (USDC).
 *  `formatBalance("0x16345785d8a0000", 18)` → `"0.1"` (1e17 wei = 0.1 ETH). */
export function formatBalance(hex: string, decimals: number): string {
  if (!hex) return "0";
  const value = (() => {
    try {
      const s = hex.startsWith("0x") || hex.startsWith("0X") ? hex : `0x${hex}`;
      return BigInt(s);
    } catch {
      return 0n;
    }
  })();
  if (decimals <= 0) return value.toLocaleString("en-US");
  const factor = 10n ** BigInt(decimals);
  const whole = value / factor;
  const frac = value % factor;
  const wholeStr = whole.toLocaleString("en-US");
  if (frac === 0n) return wholeStr;
  // Pad to full width, trim trailing zeros, cap at 6 visible digits.
  let fracStr = frac.toString().padStart(decimals, "0");
  fracStr = fracStr.replace(/0+$/, "");
  if (fracStr.length === 0) return wholeStr;
  if (fracStr.length > 6) fracStr = fracStr.slice(0, 6);
  return `${wholeStr}.${fracStr}`;
}

/** Format a signed base-10 decimal string (the shape `BalanceChangeRow.delta`
 *  carries) into a human-readable +/-prefixed value, with the same decimals
 *  shift `formatBalance` applies. Used in the per-wallet panel to show how
 *  much a touched row CHANGED in this step. */
export function formatSignedDelta(decimal: string, decimals: number): string {
  if (!decimal) return "0";
  const isNegative = decimal.startsWith("-");
  const rawStr = isNegative ? decimal.slice(1) : decimal;
  const value = (() => {
    try {
      return BigInt(rawStr);
    } catch {
      return 0n;
    }
  })();
  if (value === 0n) return "0";
  const sign = isNegative ? "-" : "+";
  if (decimals <= 0) return `${sign}${value.toLocaleString("en-US")}`;
  const factor = 10n ** BigInt(decimals);
  const whole = value / factor;
  const frac = value % factor;
  const wholeStr = whole.toLocaleString("en-US");
  if (frac === 0n) return `${sign}${wholeStr}`;
  let fracStr = frac.toString().padStart(decimals, "0").replace(/0+$/, "");
  if (fracStr.length > 6) fracStr = fracStr.slice(0, 6);
  return `${sign}${wholeStr}.${fracStr}`;
}

// ── wallet state projection ───────────────────────────────────────────────
//
// Mirrors the WASM-side `WalletState` shape (see
// `crates/policy-server/asset-model/state/src/`). Only the slices the
// simulator UI actually renders are typed — positions and pending entries
// are kept opaque-ish since their shapes are protocol-dependent and the
// simulator only needs counts/summaries today.

/** One holding row projected from `WalletState.tokens`. The on-disk JSON
 *  encodes tokens as `Vec<(TokenKey, TokenAmount)>` — a tuple list, NOT a
 *  map — so the projector flattens each `[key, data]` pair into this view. */
export interface TokenHoldingRow {
  key: TokenKey;
  symbol: string;
  decimals: number;
  /** Raw amount as the engine emits it (hex `0x…` string in the
   *  fungible case). Kept verbatim — the UI does the formatting if it
   *  wants decimal precision. */
  balance: string;
  /** Amount already committed to pending txs (subtract from balance for
   *  "spendable"). Hex `0x…` like `balance`. */
  committed: string;
  /** Unix seconds — `state.tokens[*].last_synced_at`. Stale if the
   *  reducer ran a tx without resyncing. */
  lastSyncedAt: number;
}

/** Renderable projection of one `WalletState`. Empty arrays + `null`
 *  `walletAddress` represent a brand-new / cleared state. */
export interface WalletStateView {
  /** `state.wallet_id.address`. `null` for synthesized/empty state. */
  walletAddress: string | null;
  /** Flattened token holdings list. Ordering follows the engine's
   *  serialization (tuple-list order). */
  tokens: TokenHoldingRow[];
  /** Number of open positions. We don't render their internals yet —
   *  this is just the count for the StatePanel header pill. */
  positionCount: number;
  /** Number of pending txs queued against this wallet. */
  pendingCount: number;
  /** Approval bucket sizes, kept as counts for the header pill. */
  approvalCounts: { erc20: number; setForAll: number; permit2: number };
}

function asTokenHoldingRow(raw: unknown): TokenHoldingRow | null {
  const key = asTokenKey(pick(raw, "key"));
  if (!key.address) return null;
  const balance = pick(raw, "balance");
  const committed = pick(raw, "committed");
  return {
    key,
    symbol: asString(pick(raw, "symbol")),
    decimals: typeof pick(raw, "decimals") === "number" ? (pick(raw, "decimals") as number) : 0,
    balance: asString(pick(balance, "amount")),
    committed: asString(pick(committed, "amount")),
    lastSyncedAt:
      typeof pick(raw, "last_synced_at") === "number"
        ? (pick(raw, "last_synced_at") as number)
        : 0,
  };
}

/**
 * Project the wire-shape `WalletState` JSON into a `WalletStateView`. The
 * engine serializes `tokens` as a Vec<(key, data)> tuple list — each entry
 * is a 2-element array. Best-effort: malformed entries are dropped so a
 * partial render still happens.
 */
export function parseWalletState(state: unknown): WalletStateView {
  const walletId = pick(state, "wallet_id");
  const walletAddress =
    typeof pick(walletId, "address") === "string"
      ? (pick(walletId, "address") as string)
      : null;

  const tokensRaw = pick(state, "tokens");
  const tokens: TokenHoldingRow[] = [];
  if (Array.isArray(tokensRaw)) {
    for (const entry of tokensRaw) {
      // tuple-list element shape: [TokenKey, TokenData]. Index 1 holds the
      // displayable fields; index 0 is the routing key (already on data.key
      // too).
      if (Array.isArray(entry) && entry.length === 2) {
        const row = asTokenHoldingRow(entry[1]);
        if (row) tokens.push(row);
      } else {
        // Fallback: a plain object also works if a future serializer emits
        // `{ key, ... }` directly. Defensive — no production path emits this
        // today but cheap to support.
        const row = asTokenHoldingRow(entry);
        if (row) tokens.push(row);
      }
    }
  }

  const positionsRaw = pick(state, "positions");
  const pendingRaw = pick(state, "pending");
  const approvalsRaw = pick(state, "approvals");
  const erc20Raw = pick(approvalsRaw, "erc20");
  const setForAllRaw = pick(approvalsRaw, "set_for_all");
  const permit2Raw = pick(approvalsRaw, "permit2");

  return {
    walletAddress,
    tokens,
    positionCount: Array.isArray(positionsRaw) ? positionsRaw.length : 0,
    pendingCount: Array.isArray(pendingRaw) ? pendingRaw.length : 0,
    approvalCounts: {
      erc20: Array.isArray(erc20Raw) ? erc20Raw.length : 0,
      setForAll: Array.isArray(setForAllRaw) ? setForAllRaw.length : 0,
      permit2: Array.isArray(permit2Raw) ? permit2Raw.length : 0,
    },
  };
}

/** Set of token-key strings (`<standard>:<chain>:<address>`) that changed
 *  in a single delta — used by the StatePanel scrubber to highlight rows
 *  in the rendered post-state list. */
export function tokenKeysTouchedBy(delta: StateDeltaView): Set<string> {
  const out = new Set<string>();
  for (const t of delta.tokenChanges) {
    out.add(`${t.key.standard}:${t.key.chain}:${t.key.address}`);
  }
  return out;
}

/** Stable string id for a `TokenKey`. Same projection
 *  `tokenKeysTouchedBy` uses, exposed for callers that need to match
 *  delta-touched rows against holding rows. */
export function tokenKeyId(key: TokenKey): string {
  return `${key.standard}:${key.chain}:${key.address}`;
}

// ── account-level aggregation ─────────────────────────────────────────────
//
// The simulation page surfaces both per-wallet state (each registered wallet
// rendered separately) and an account-level aggregate (every selected wallet
// rolled up — balances summed by token, position/pending counts summed). The
// aggregate is purely derived; the simulator still runs per-wallet, so its
// inputs and outputs are vanilla `OpaqueWalletState`. This block holds the
// helpers that fold those per-wallet states into a single renderable view.

/** Parse a hex `0x…` value as a BigInt. Returns `0n` for empty / invalid
 *  input so a malformed balance row doesn't poison a sum. */
function hexToBigInt(hex: string): bigint {
  if (!hex) return 0n;
  const s = hex.startsWith("0x") || hex.startsWith("0X") ? hex : `0x${hex}`;
  try {
    return BigInt(s);
  } catch {
    return 0n;
  }
}

/** Format a BigInt back into the engine's hex string convention. We keep
 *  lowercase + no leading zeros (apart from `0x0`) so two equal-valued
 *  sums render identically across calls. */
function bigIntToHex(n: bigint): string {
  return `0x${n.toString(16)}`;
}

/** One row in the account-level aggregated token list. Mirrors
 *  {@link TokenHoldingRow} but the balance is the SUM across every wallet
 *  that holds the same `TokenKey`, and `walletCount` records how many of
 *  the selected wallets contributed. */
export interface AggregatedTokenRow {
  key: TokenKey;
  /** Symbol of the first contributor — assumed identical across wallets
   *  for a given token. */
  symbol: string;
  decimals: number;
  /** Sum of `balance` across all contributing wallets, encoded as hex. */
  totalBalance: string;
  /** Sum of `committed` across all contributing wallets, encoded as hex. */
  totalCommitted: string;
  /** Number of selected wallets contributing to this token row. */
  walletCount: number;
}

/** Account-level rollup of every selected wallet's state. The simulator
 *  doesn't operate against this — it's purely a display projection. */
export interface AggregatedAccountView {
  /** Number of wallets folded into this rollup. */
  walletCount: number;
  /** Token holdings keyed by `tokenKeyId(key)` with summed balances. */
  tokens: AggregatedTokenRow[];
  /** Sum of `state.positions.length` across contributors. */
  positionCount: number;
  /** Sum of `state.pending.length` across contributors. */
  pendingCount: number;
  /** Sum of approval bucket sizes across contributors. */
  approvalCounts: { erc20: number; setForAll: number; permit2: number };
}

/**
 * Aggregate a list of per-wallet `WalletStateView`s into a single
 * account-level rollup. Tokens with the same `(standard, chain, address)`
 * triple across wallets are merged — balances + committed amounts summed,
 * `walletCount` incremented. Position / pending / approval entries are
 * counted (the simulator does not yet render their bodies in the account
 * view), so the rollup carries totals, not concatenated lists.
 *
 * Returns an empty view for an empty input — the caller renders a
 * "no wallets selected" placeholder in that case.
 */
export function aggregateWalletStates(
  views: ReadonlyArray<WalletStateView>,
): AggregatedAccountView {
  const buckets = new Map<string, AggregatedTokenRow>();
  let positionCount = 0;
  let pendingCount = 0;
  let erc20 = 0;
  let setForAll = 0;
  let permit2 = 0;

  for (const v of views) {
    for (const t of v.tokens) {
      const id = tokenKeyId(t.key);
      const prior = buckets.get(id);
      if (prior) {
        const total = hexToBigInt(prior.totalBalance) + hexToBigInt(t.balance);
        const committed =
          hexToBigInt(prior.totalCommitted) + hexToBigInt(t.committed);
        buckets.set(id, {
          ...prior,
          totalBalance: bigIntToHex(total),
          totalCommitted: bigIntToHex(committed),
          walletCount: prior.walletCount + 1,
        });
      } else {
        buckets.set(id, {
          key: t.key,
          symbol: t.symbol,
          decimals: t.decimals,
          totalBalance: bigIntToHex(hexToBigInt(t.balance)),
          totalCommitted: bigIntToHex(hexToBigInt(t.committed)),
          walletCount: 1,
        });
      }
    }
    positionCount += v.positionCount;
    pendingCount += v.pendingCount;
    erc20 += v.approvalCounts.erc20;
    setForAll += v.approvalCounts.setForAll;
    permit2 += v.approvalCounts.permit2;
  }

  return {
    walletCount: views.length,
    tokens: [...buckets.values()],
    positionCount,
    pendingCount,
    approvalCounts: { erc20, setForAll, permit2 },
  };
}

/** Filter a `WalletStateView`'s tokens to a single CAIP-2 chain string
 *  (e.g. `"eip155:1"`). Used when the page-level chain selector restricts
 *  the visible set so multi-chain wallets don't render rows the current
 *  simulation can't act on. */
export function filterViewByChain(
  view: WalletStateView,
  chain: string,
): WalletStateView {
  return {
    ...view,
    tokens: view.tokens.filter((t) => t.key.chain === chain),
  };
}

/** Same filter for the aggregate view. */
export function filterAggregateByChain(
  view: AggregatedAccountView,
  chain: string,
): AggregatedAccountView {
  return {
    ...view,
    tokens: view.tokens.filter((t) => t.key.chain === chain),
  };
}
