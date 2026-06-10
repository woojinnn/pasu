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
}

/** A protocol position (perp / lending / staking …). */
export interface PositionView {
  id: string;
  label: string;
  /** Protocol id (aave / hyperliquid …) — a relevance-filter key. */
  protocol: string;
}

/** An outstanding approval. */
export interface ApprovalView {
  id: string;
  /** Token symbol the approval is on. */
  token: string;
  /** Spender label/address. */
  spender: string;
  unlimited: boolean;
}

/** A wallet's full state snapshot (s0, s1, … in the result step). */
export interface WalletStateView {
  address: string;
  name: string;
  tokens: TokenHolding[];
  positions: PositionView[];
  approvals: ApprovalView[];
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

/** The wizard's 4 steps. */
export type WizardStep = 1 | 2 | 3 | 4;

export const STEP_LABELS: Record<WizardStep, string> = {
  1: "지갑 · 상태",
  2: "정책 선택",
  3: "트랜잭션",
  4: "결과",
};
