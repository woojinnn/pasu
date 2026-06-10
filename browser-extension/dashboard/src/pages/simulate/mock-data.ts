/**
 * Fixtures for the /simulate wizard's MockProvider. Everything the UI needs is
 * fabricated here so the whole 4-step flow is clickable before any backend is
 * wired. Shapes match {@link types} exactly, so the RealProvider can later drop
 * in by producing the same view-models.
 *
 * The deny diagram uses a REAL PolicyIR + the canonical `enumeratePaths` scheme,
 * so the red "어디가 막혔는지" highlight is genuine (not a picture).
 */

import type { Expr, PolicyIR } from "../../cedar/blocks/ir";
import { enumeratePaths } from "../../cedar/diagnosis/path";

import type {
  PackageView,
  PolicyView,
  RunResult,
  TxRow,
  WalletStateView,
  WalletView,
} from "./types";

// ── known mainnet tokens (address ⇄ symbol) ────────────────────────────────
export const TOKENS = {
  USDC: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
  USDT: "0xdac17f958d2ee523a2206206994597c13d831ec7",
  WETH: "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2",
  DAI: "0x6b175474e89094c44da98b954eedeac495271d0f",
  WBTC: "0x2260fac5e5542a773aa44fbcfedf7c193bc2c599",
  LINK: "0x514910771af9ca656af840dff83e8264ecf986ca",
} as const;

const SYMBOL_BY_ADDR: Record<string, string> = Object.fromEntries(
  Object.entries(TOKENS).map(([sym, addr]) => [addr, sym]),
);

/** Address → "SYMBOL(0xabcd…wxyz)" for diagram chip labels. */
export function humanizeAddr(text: string): string {
  return text.replace(/0x[a-fA-F0-9]{40}/g, (m) => {
    const sym = SYMBOL_BY_ADDR[m.toLowerCase()];
    const short = `${m.slice(0, 6)}…${m.slice(-4)}`;
    return sym ? `${sym}(${short})` : m;
  });
}

// ── wallets ─────────────────────────────────────────────────────────────────
const W_BUJA = "0x1111111111111111111111111111111111111111";
const W_PLAIN = "0x2222222222222222222222222222222222222222";

export const MOCK_WALLETS: WalletView[] = [
  { address: W_BUJA, name: "부자성준", chains: ["eip155:1"] },
  { address: W_PLAIN, name: "일반지갑", chains: ["eip155:1", "eip155:42161"] },
];

// ── per-wallet state (s0) ────────────────────────────────────────────────────
function holding(symbol: keyof typeof TOKENS, balance: string, usd?: string) {
  return { symbol, address: TOKENS[symbol], balance, usd };
}

export const MOCK_STATES: Record<string, WalletStateView> = {
  [W_BUJA]: {
    address: W_BUJA,
    name: "부자성준",
    tokens: [
      holding("USDC", "12,500.00", "$12,500"),
      holding("USDT", "3,000.00", "$3,000"),
      holding("WETH", "8.20", "$28,700"),
      holding("WBTC", "0.45", "$29,250"),
    ],
    positions: [{ id: "p-hl-1", label: "BTC-PERP 롱 2x", protocol: "hyperliquid" }],
    approvals: [
      { id: "a1", token: "USDC", spender: "Uniswap V3", unlimited: true },
      { id: "a2", token: "WETH", spender: "Aave V3", unlimited: false },
    ],
  },
  [W_PLAIN]: {
    address: W_PLAIN,
    name: "일반지갑",
    tokens: [
      holding("USDC", "420.00", "$420"),
      holding("DAI", "150.00", "$150"),
      holding("LINK", "85.00", "$1,190"),
    ],
    positions: [],
    approvals: [{ id: "a3", token: "DAI", spender: "Curve", unlimited: true }],
  },
};

// ── policies ─────────────────────────────────────────────────────────────────
export const MOCK_POLICIES: PolicyView[] = [
  {
    id: "swap-token-allowlist",
    name: "스왑 토큰 화이트리스트 (부자성준)",
    action: "Amm::Swap",
    tokens: ["USDC", "USDT", "WETH", "DAI", "WBTC", "LINK"],
    protocols: [],
    walletAddress: W_BUJA,
  },
  { id: "large-transfer-block", name: "대량 송금 차단", action: "Token::Erc20Transfer", tokens: ["USDC", "USDT"], protocols: [] },
  { id: "unlimited-approve-warn", name: "무제한 승인 경고", action: "Token::Erc20Approve", tokens: [], protocols: [] },
  { id: "perp-leverage-cap", name: "레버리지 상한 (Hyperliquid)", action: "Perp::PlaceOrder", tokens: [], protocols: ["hyperliquid"] },
  { id: "aave-withdraw-guard", name: "Aave 출금 제한", action: "Lending::Withdraw", tokens: ["WETH"], protocols: ["aave"] },
  { id: "blind-sign-warn", name: "블라인드 서명 경고", action: "Token::Erc20Permit", tokens: [], protocols: [] },
];

export const MOCK_PACKAGES: PackageView[] = [
  { id: "pkg-safe", name: "기본 안전팩", policyIds: ["large-transfer-block", "unlimited-approve-warn", "blind-sign-warn"] },
  { id: "pkg-defi", name: "DeFi 보호팩", policyIds: ["perp-leverage-cap", "aave-withdraw-guard"] },
];

/** Initially "선택된" policies (what the real getEnabledPolicyIds would seed). */
export const MOCK_ENABLED_IDS = ["swap-token-allowlist", "large-transfer-block", "unlimited-approve-warn"];

// ── transaction queue ─────────────────────────────────────────────────────────
export const MOCK_TX_ROWS: TxRow[] = [
  {
    id: "tx-1",
    label: "USDC 송금 1,000",
    fromWallet: W_BUJA,
    to: "0xbeef000000000000000000000000000000000001",
    calldata: "0xa9059cbb…",
    value: "0",
  },
  {
    id: "tx-2",
    label: "USDT → WETH 스왑",
    fromWallet: W_BUJA,
    to: "0x68b3465833fb72a70ecdf485e0e4c7bd8665fc45",
    calldata: "0x5ae401dc…",
    value: "0",
  },
];

// ── deny diagram: a real PolicyIR (forbid Swap when tokenIn ∈ allowlist) ──────
const attr = (of: Expr, name: string): Expr => ({ kind: "attr", of, attr: name });
const ctx = (path: string): Expr =>
  path.split(".").reduce<Expr>((e, p) => attr(e, p), { kind: "var", name: "context" });

const ALLOWLIST = ["USDC", "USDT", "WETH", "DAI", "WBTC", "LINK"] as const;

/** forbid(Amm::Swap) when [allowlist].contains(context.tokenIn.key.address) */
export const SWAP_ALLOWLIST_IR: PolicyIR = {
  kind: "policy",
  effect: "forbid",
  annotations: [{ name: "id", value: "swap-token-allowlist" }],
  scope: {
    principal: { kind: "scopeAll" },
    action: { kind: "scopeEq", entity: { type: "Amm::Action", id: "Swap" } },
    resource: { kind: "scopeAll" },
  },
  conditions: [
    {
      kind: "when",
      body: {
        kind: "binary",
        op: "contains",
        left: { kind: "set", elements: ALLOWLIST.map((s) => ({ kind: "lit", litType: "string", value: TOKENS[s] })) },
        right: ctx("tokenIn.key.address"),
      },
    },
  ],
};

/** The canonical path of the set element matching `addr` — the chip to trace red. */
function memberPath(ir: PolicyIR, addr: string): string {
  const hit = enumeratePaths(ir).find((p) => p.node.kind === "lit" && String(p.node.value).toLowerCase() === addr);
  return hit?.path ?? "";
}

// Step 2 (USDT→WETH swap) is blocked because tokenIn USDT is on the allowlist.
const USDT_HIGHLIGHT = [memberPath(SWAP_ALLOWLIST_IR, TOKENS.USDT)];

// ── run result: s0 → s1 → s2 with one pass + one deny ────────────────────────
function delta(base: WalletStateView, tokens: { symbol: string; newBalance: string }[]): WalletStateView {
  const map = new Map(tokens.map((t) => [t.symbol, t.newBalance]));
  return { ...base, tokens: base.tokens.map((t) => (map.has(t.symbol) ? { ...t, balance: map.get(t.symbol)! } : t)) };
}

const buja0 = MOCK_STATES[W_BUJA];
const plain0 = MOCK_STATES[W_PLAIN];

// s1: tx-1 USDC 1,000 송금 → 부자성준 USDC 12,500 → 11,500
const buja1 = delta(buja0, [{ symbol: "USDC", newBalance: "11,500.00" }]);
// s2: tx-2 swap DENIED → state unchanged from s1
const buja2 = buja1;

export const MOCK_RUN: RunResult = {
  wallets: [W_BUJA, W_PLAIN],
  histories: {
    [W_BUJA]: [buja0, buja1, buja2],
    [W_PLAIN]: [plain0, plain0, plain0],
  },
  steps: [
    {
      index: 1,
      rowId: "tx-1",
      fromWallet: W_BUJA,
      label: "USDC 송금 1,000",
      verdict: "pass",
      diff: { tokens: [{ symbol: "USDC", delta: "-1,000.00", sign: "down" }], gas: "0.0012 ETH" },
      denies: [],
    },
    {
      index: 2,
      rowId: "tx-2",
      fromWallet: W_BUJA,
      label: "USDT → WETH 스왑",
      verdict: "fail",
      diff: { tokens: [], note: "정책 차단으로 상태 변화 없음" },
      denies: [
        {
          policyId: "swap-token-allowlist",
          policyName: "스왑 토큰 화이트리스트 (부자성준)",
          reason: "입력 토큰 USDT 가 차단 목록에 있어 스왑이 거부되었습니다",
          severity: "deny",
          step: 2,
          ir: SWAP_ALLOWLIST_IR,
          highlightPaths: USDT_HIGHLIGHT,
        },
      ],
    },
  ],
};
