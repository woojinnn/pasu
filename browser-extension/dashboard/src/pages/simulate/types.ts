/**
 * The /simulate wizard's view-model contract — clean, UI-facing types that the
 * 4 step views consume. The controller ({@link useSimController}) produces these
 * from a provider: a MockProvider (fixtures) today, a RealProvider (server +
 * sim-bridge WASM) later. The UI never sees opaque WASM shapes, so swapping the
 * provider needs no UI change.
 */

import type { PolicyIR } from "../../cedar/blocks/ir";

/** A registered wallet the user can simulate from. */
export interface WalletView {
  /** Lowercase 0x address — the stable id. */
  address: string;
  /** Friendly label (ENS / nickname / shortened address). */
  name: string;
  /** CAIP-2 chains this wallet holds state on. */
  chains: string[];
}

/** One token holding in a wallet's state. */
export interface TokenHolding {
  symbol: string;
  /** Token contract address (lowercase) — the relevance-filter key. */
  address: string;
  /** Human display balance, e.g. "1,250.00". */
  balance: string;
  /** Optional USD value, e.g. "$1,250". */
  usd?: string;
  /** Numeric USD value — drives allocation bars + sorting (display via `usd`). */
  usdNum?: number;
  /** Unit price display, e.g. "$65,000". */
  priceUsd?: string;
  /** Amount locked behind pending txs (display), e.g. "0.05". */
  committed?: string;
  /** CAIP-2 chain the holding lives on, e.g. "eip155:1". */
  chain?: string;
}

/** A protocol position (perp / lending / staking …). Optional fields let the
 *  dashboard render a rich card; the RealProvider fills what the protocol
 *  exposes and leaves the rest undefined. */
export interface PositionView {
  id: string;
  label: string;
  /** Protocol id (aave / hyperliquid …) — a relevance-filter key. */
  protocol: string;
  /** Card layout discriminator. */
  kind?: "perp" | "lending" | "staking" | "other";
  side?: "long" | "short";
  /** Leverage display, e.g. "2x". */
  leverage?: string;
  /** Notional size display, e.g. "$58,400". */
  sizeUsd?: string;
  entryPrice?: string;
  markPrice?: string;
  /** Unrealized PnL display, e.g. "+$1,240". */
  pnlUsd?: string;
  pnlSign?: "up" | "down";
  liqPrice?: string;
  /** Margin posted, display e.g. "$29,200". */
  marginUsd?: string;
  /** Return on equity display, e.g. "+4.2%". */
  roe?: string;
  /** Lending health factor, e.g. "1.82". */
  health?: string;
  /** Lending collateral / debt detail (display). */
  collateralUsd?: string;
  debtUsd?: string;
}

/** An outstanding approval. */
export interface ApprovalView {
  id: string;
  /** Token symbol the approval is on. */
  token: string;
  /** Spender label/address. */
  spender: string;
  unlimited: boolean;
  /** Approved amount display ("무제한" / "1,000"). */
  amount?: string;
  /** Spender contract address (lowercase) for the mono chip. */
  spenderAddress?: string;
  /** Risk tier — unlimited / unknown spender → high. */
  risk?: "high" | "med" | "low";
  /** Approval scope display, e.g. "ERC-20" / "Permit2". */
  scope?: string;
  /** Token contract the approval is on (lowercase). */
  tokenAddress?: string;
  /** When the approval was granted (display), e.g. "2024-11-02". */
  grantedAt?: string;
  /** Short human reason the risk tier is what it is. */
  riskReason?: string;
}

/** A wallet's full state snapshot (s0, s1, … in the result step). */
export interface WalletStateView {
  address: string;
  name: string;
  tokens: TokenHolding[];
  positions: PositionView[];
  approvals: ApprovalView[];
  /** Total portfolio value display, e.g. "$98,300". */
  portfolioUsd?: string;
}

/** A policy selectable in step 2. */
export interface PolicyView {
  id: string;
  name: string;
  /** `Namespace::Action` the policy triggers on, e.g. "Amm::Swap". */
  action: string;
  /** Token symbols/addresses referenced in the policy (relevance filter). */
  tokens: string[];
  /** Protocols referenced (relevance for positions). */
  protocols: string[];
  /** When set, the wallet address this policy is scoped to (→ "지갑 관련"). */
  walletAddress?: string;
}

/** A policy package — a named bundle; toggling it flips all its policies. */
export interface PackageView {
  id: string;
  name: string;
  policyIds: string[];
}

/** A transaction-queue row (raw calldata). Mirrors the existing CalldataTxRow. */
export interface TxRow {
  id: string;
  label: string;
  fromWallet: string;
  to: string;
  calldata: string;
  /** msg.value in wei (decimal string). */
  value: string;
}

/** A signed token-balance change within one step's diff. */
export interface TokenDelta {
  symbol: string;
  /** Signed display delta, e.g. "+100.0" / "-1.0". */
  delta: string;
  sign: "up" | "down";
}

/** One policy deny/warn at a step, with the diagram + blocked-node info. */
export interface DenyView {
  policyId: string;
  policyName: string;
  reason: string;
  severity: "deny" | "warn";
  /** Step index (1-based) this deny first occurred — drives cumulative order. */
  step: number;
  /** Parsed policy for the structure diagram. */
  ir: PolicyIR;
  /** Canonical node paths to trace red ("어디가 막혔는지"). */
  highlightPaths: string[];
}

/** The outcome of one simulated step (one tx → one wallet transition). */
export interface StepView {
  /** 1-based step index. */
  index: number;
  rowId: string;
  /** Wallet (lowercase address) that ran this step. */
  fromWallet: string;
  label: string;
  verdict: "pass" | "warn" | "fail";
  /** What changed in the from-wallet's state at this step. */
  diff: {
    tokens: TokenDelta[];
    gas?: string;
    note?: string;
  };
  /** Policies that denied/warned at THIS step. */
  denies: DenyView[];
}

/** The whole run result: per-wallet state sequence + per-step outcomes. */
export interface RunResult {
  /** Wallets (lowercase addresses) involved, in display order. */
  wallets: string[];
  /** Per wallet: [s0, s1, …, sN] snapshots. All arrays length N+1. */
  histories: Record<string, WalletStateView[]>;
  /** N step outcomes (index 1..N). */
  steps: StepView[];
}

/** The wizard's 4 steps. Labels live in the "simulation" i18n namespace
 *  (`wizard.steps.*`) and are looked up at render time in {@link StepNav}. */
export type WizardStep = 1 | 2 | 3 | 4;
