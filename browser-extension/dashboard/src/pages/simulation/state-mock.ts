/**
 * Simulation state mock + heuristic delta applier.
 *
 * The policy-state reducer that produces real S₀ → Sₙ snapshots lives on
 * the server (see `policy-state` crate). Until the wasm bridge exposes a
 * `reduce-step` op, this module fabricates state evolution from the step
 * context so the UI has something to visualize.
 *
 * The shapes here are intentionally narrower than `WalletState` — they
 * cover what the visualization actually renders. The server's real types
 * remain authoritative; swap this for live data when the reducer ships.
 */
import type { SequenceStepInput } from "../../cedar";

export type StateChainId = "ethereum" | "base" | "arbitrum" | "bnb";

/** A token holding row in the visualized state. */
export interface SimTokenRow {
  /** unique id like "usdc-ethereum" — used as state-key for highlighting */
  key: string;
  symbol: string;
  chain: StateChainId;
  amount: number;
  /** USD value at view time; null when price is missing */
  usd: number | null;
  stale?: boolean;
  unknown?: boolean;
  /** Mostly-opaque blob shown under the symbol (address suffix, etc.) */
  note?: string;
}

export interface SimPositionRow {
  /** unique id like "aave-eth-usdc" */
  key: string;
  protocol: string;
  pair: string;
  /** USD-denominated; reducer will fill in chain id when wired up. */
  collateralUsd: number;
  debtUsd: number;
  healthFactor: number;
  /** 0..1, displayed as % */
  ltv: number;
  /** policy-side floor for HF (UI shows under the bar) */
  hfFloor?: number;
  /** policy-side max LTV */
  ltvMax?: number;
}

export interface SimNftRow {
  key: string;
  name: string;
  collection: string;
  chain: StateChainId;
  floorEth?: number;
}

export interface SimState {
  tokens: SimTokenRow[];
  positions: SimPositionRow[];
  nfts: SimNftRow[];
  portfolioUsd: number;
}

export interface SimDelta {
  /** signed deltas keyed by state key — only fields that changed */
  amountDeltas: Record<string, number>;
  /** value before/after (for tooltip / detail) */
  before: Record<string, unknown>;
  after: Record<string, unknown>;
}

export interface SimSnapshot {
  /** state after this step */
  state: SimState;
  /** set of state keys that changed vs. previous snapshot */
  changed: Set<string>;
  /** structured diff for the detail panel */
  delta: SimDelta;
}

// ── defaults ──────────────────────────────────────────────────────────────

/** A deliberately diverse seed state that exercises tokens / positions /
 *  NFTs and across multiple chains so the visualization isn't empty. */
export const DEFAULT_INITIAL_STATE: SimState = {
  tokens: [
    { key: "usdc-ethereum", symbol: "USDC", chain: "ethereum", amount: 12_000, usd: 12_000 },
    { key: "weth-ethereum", symbol: "WETH", chain: "ethereum", amount: 3.5, usd: 11_200, stale: true },
    { key: "eth-ethereum",  symbol: "ETH",  chain: "ethereum", amount: 1.8,   usd: 5_760 },
    { key: "usdc-base",     symbol: "USDC", chain: "base",     amount: 4_200, usd: 4_200 },
    { key: "arb-arbitrum",  symbol: "ARB",  chain: "arbitrum", amount: 1_500, usd: 1_230 },
    {
      key: "unknown-bnb",
      symbol: "Unknown Token",
      chain: "bnb",
      amount: 880_000,
      usd: null,
      unknown: true,
      note: "0x3f9a··B71e",
    },
  ],
  positions: [
    {
      key: "aave-v3-eth-usdc",
      protocol: "Aave v3",
      pair: "ETH-USDC",
      collateralUsd: 12_800,
      debtUsd: 7_000,
      healthFactor: 1.51,
      ltv: 0.55,
      hfFloor: 1.5,
      ltvMax: 0.7,
    },
  ],
  nfts: [
    { key: "pudgy-4021", name: "Pudgy Penguin #4021", collection: "Pudgy Penguins", chain: "ethereum", floorEth: 12.4 },
    { key: "ens-soyeon", name: "soyeon.eth", collection: "ENS Names", chain: "ethereum" },
  ],
  portfolioUsd: 33_790,
};

// ── delta applier ─────────────────────────────────────────────────────────

/**
 * Apply a single step against a state, returning the next snapshot.
 *
 * The applier is intentionally lossy and heuristic — it inspects
 * `step.action` (e.g. `Action::"Amm::Swap"`) and `step.context`
 * (e.g. `{ amountIn, amountOut, tokenIn, tokenOut }`) and mutates
 * the closest matching state row. When the step has no fields it
 * recognizes, the snapshot is returned unchanged with an empty
 * `changed` set, so the timeline still shows a node.
 */
export function applyStep(prev: SimState, step: SequenceStepInput): SimSnapshot {
  const next: SimState = {
    tokens: prev.tokens.map((t) => ({ ...t })),
    positions: prev.positions.map((p) => ({ ...p })),
    nfts: prev.nfts.map((n) => ({ ...n })),
    portfolioUsd: prev.portfolioUsd,
  };
  const changed = new Set<string>();
  const amountDeltas: Record<string, number> = {};
  const before: Record<string, unknown> = {};
  const after: Record<string, unknown> = {};

  const kind = parseActionKind(step.action);
  const ctx = (step.context ?? {}) as Record<string, unknown>;

  switch (kind.domain) {
    case "Amm": {
      // Swap: tokenIn (-amountIn), tokenOut (+amountOut)
      if (kind.kind === "Swap") {
        applyTokenDelta(next, changed, amountDeltas, before, after, str(ctx.tokenIn), -num(ctx.amountIn));
        applyTokenDelta(next, changed, amountDeltas, before, after, str(ctx.tokenOut), num(ctx.amountOut));
      }
      break;
    }
    case "Lending": {
      // Supply / Borrow / Withdraw / Repay — all touch the first position
      const posIdx = 0;
      const pos = next.positions[posIdx];
      const amt = num(ctx.amount);
      if (!pos) break;
      const k = pos.key;
      const beforePos = { ...prev.positions[posIdx] };
      if (kind.kind === "Supply") {
        pos.collateralUsd += amt;
      } else if (kind.kind === "Withdraw") {
        pos.collateralUsd = Math.max(0, pos.collateralUsd - amt);
      } else if (kind.kind === "Borrow") {
        pos.debtUsd += amt;
      } else if (kind.kind === "Repay") {
        pos.debtUsd = Math.max(0, pos.debtUsd - amt);
      }
      // Recompute HF + LTV (toy formula — visualization only)
      pos.ltv = pos.collateralUsd > 0 ? pos.debtUsd / pos.collateralUsd : 0;
      pos.healthFactor = pos.debtUsd > 0
        ? +(pos.collateralUsd * 0.825 / pos.debtUsd).toFixed(2)
        : 99.99;
      if (pos.collateralUsd !== beforePos.collateralUsd
          || pos.debtUsd !== beforePos.debtUsd) {
        changed.add(k);
        before[k] = beforePos;
        after[k] = { ...pos };
      }
      break;
    }
    case "Transfer": {
      const sym = str(ctx.token) || str(ctx.symbol);
      const amt = num(ctx.amount);
      if (sym && amt) {
        applyTokenDelta(next, changed, amountDeltas, before, after, sym, -amt);
      }
      break;
    }
    default:
      break;
  }

  next.portfolioUsd = computePortfolioUsd(next);
  return { state: next, changed, delta: { amountDeltas, before, after } };
}

/** Replays the whole sequence from S₀ and returns all snapshots, including S₀. */
export function buildTimeline(
  initial: SimState,
  steps: SequenceStepInput[],
): SimSnapshot[] {
  const out: SimSnapshot[] = [{
    state: initial,
    changed: new Set(),
    delta: { amountDeltas: {}, before: {}, after: {} },
  }];
  let cur = initial;
  for (const step of steps) {
    const snap = applyStep(cur, step);
    out.push(snap);
    cur = snap.state;
  }
  return out;
}

// ── helpers ───────────────────────────────────────────────────────────────

function parseActionKind(action: string): { domain: string; kind: string } {
  // Action::"Domain::Kind"
  const m = action.match(/Action::"([^:]+)::([^"]+)"/);
  if (!m) return { domain: "", kind: "" };
  return { domain: m[1], kind: m[2] };
}

function str(v: unknown): string {
  return typeof v === "string" ? v : "";
}
function num(v: unknown): number {
  if (typeof v === "number") return v;
  if (typeof v === "string") {
    const n = Number(v);
    return Number.isFinite(n) ? n : 0;
  }
  return 0;
}

function applyTokenDelta(
  state: SimState,
  changed: Set<string>,
  amountDeltas: Record<string, number>,
  before: Record<string, unknown>,
  after: Record<string, unknown>,
  symbolOrKey: string,
  delta: number,
): void {
  if (!symbolOrKey || delta === 0) return;
  const lower = symbolOrKey.toLowerCase();
  const idx = state.tokens.findIndex(
    (t) => t.symbol.toLowerCase() === lower || t.key === lower,
  );
  if (idx === -1) return;
  const row = state.tokens[idx];
  const prevAmount = row.amount;
  row.amount = +(row.amount + delta).toFixed(4);
  // Cheap USD revaluation (constant unit price)
  const unit = prevAmount > 0 && row.usd != null ? row.usd / prevAmount : null;
  if (unit != null && row.usd != null) {
    row.usd = +(row.amount * unit).toFixed(2);
  }
  changed.add(row.key);
  amountDeltas[row.key] = (amountDeltas[row.key] ?? 0) + delta;
  before[row.key] = { amount: prevAmount };
  after[row.key] = { amount: row.amount };
}

function computePortfolioUsd(state: SimState): number {
  let total = 0;
  for (const t of state.tokens) if (t.usd != null) total += t.usd;
  return Math.round(total);
}
