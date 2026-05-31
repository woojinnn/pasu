// monitoring-data.js — placeholder data for the Monitoring console.
// ⚠️ All wallets / addresses / tokens / spenders / amounts are PLACEHOLDERS for layout only.
// NOT real Scopeball data. Replace with live SDK Findings (P0) + balance/price (P1) + allowance (P2).
//
// Honesty rule (brief §4): VaR/amount cells have 3 states — filled / loading / none.
//   The `dataState` tweak ('full' | 'p0' | 'loading') decides which is shown; numbers below are
//   the "full" (future) values. In 'p0' only Findings are real; balances/VaR show empty states.

/* ── chains ── */
const CHAINS = {
  eth: { name: 'Ethereum', short: 'ETH', color: '#627EEA' },
  arb: { name: 'Arbitrum', short: 'ARB', color: '#2D9BF0' },
  base: { name: 'Base', short: 'BASE', color: '#0052FF' },
  op: { name: 'Optimism', short: 'OP', color: '#FF0420' },
  poly: { name: 'Polygon', short: 'POLY', color: '#8247E5' },
};

/* ── spender reputation ── */
// rep: 'known' (green) | 'unknown' (grey + ⚠) | 'blocked' (red)
const SPENDERS = {
  uniswap:  { label: 'Uniswap: Universal Router', short: 'Uniswap', addr: '0x66a9···2A65', rep: 'known' },
  opensea:  { label: 'OpenSea: Seaport 1.6', short: 'OpenSea', addr: '0x0000···0006', rep: 'known' },
  aave:     { label: 'Aave: Pool V3', short: 'Aave V3', addr: '0x8787···A4E2', rep: 'known' },
  permit2:  { label: 'Permit2', short: 'Permit2', addr: '0x0000···3000', rep: 'known' },
  unknown1: { label: 'Unverified contract', short: '미상 spender', addr: '0x9F3a···0D21', rep: 'unknown' },
  unknown2: { label: 'Unverified contract', short: '미상 spender', addr: '0x4B0a···8F19', rep: 'unknown' },
  drainer:  { label: 'Flagged: drainer (chainalysis tier-1)', short: 'drainer 플래그', addr: '0xBd3f···6c02', rep: 'blocked' },
};

/* ── wallets (4 EOAs) ──
   status: 'fail' | 'warn' | 'calm'   (worst verdict present)
   totalUsd: portfolio value (P1)      */
const WALLETS = [
  { id: 'main',     name: { ko: '메인 지갑', en: 'Main' },        addr: '0xA1c4···7e29', icon: 'M', status: 'fail', totalUsd: 184230, varUsd: 23980, unlimited: 3, pending: 2 },
  { id: 'hot',      name: { ko: '빠른 송금', en: 'Hot' },         addr: '0xCe88···91d2', icon: 'H', status: 'warn', totalUsd: 42810,  varUsd: 6120,  unlimited: 1, pending: 1 },
  { id: 'treasury', name: { ko: '운영 자금', en: 'Treasury' },    addr: '0xB7d1···3f4a', icon: 'T', status: 'calm', totalUsd: 512400, varUsd: 0,     unlimited: 0, pending: 0 },
  { id: 'dev',      name: { ko: '개발 지갑', en: 'Dev' },         addr: '0x3F9a···c014', icon: 'D', status: 'calm', totalUsd: 11640,  varUsd: 0,     unlimited: 0, pending: 0 },
];

/* ── chain breakdown (aggregate, P1) ── */
const CHAIN_BREAKDOWN = [
  { chain: 'eth',  usd: 348900, pct: 46.0 },
  { chain: 'arb',  usd: 144200, pct: 19.0 },
  { chain: 'base', usd: 121600, pct: 16.0 },
  { chain: 'op',   usd: 98300,  pct: 13.0 },
  { chain: 'poly', usd: 45980,  pct: 6.0 },
];

/* ── holdings (token balances + positions) ──
   risk: array of overlay tags { kind:'unlimited'|'fail'|'warn'|'none', spender, label }
   varUsd: min(allowance,balance)×price; null when no approval (no exposure)         */
const HOLDINGS = [
  { id: 'h1', wallet: 'main', asset: 'USDC', name: 'USD Coin', chain: 'eth', kind: 'token',
    balance: '184,210', balanceUsd: 184230, priceNote: '$1.00',
    approval: { spender: 'unknown1', allowance: 'Unlimited', type: 'Token' },
    risk: [{ kind: 'fail', label: { ko: '무제한 · 미상 spender', en: 'Unlimited · unknown spender' } }],
    varUsd: 184230 },

  { id: 'h2', wallet: 'main', asset: 'WETH', name: 'Wrapped Ether', chain: 'arb', kind: 'token',
    balance: '4.12', balanceUsd: 12340, priceNote: '$2,995',
    approval: { spender: 'opensea', allowance: 'Unlimited', type: 'Token' },
    risk: [{ kind: 'unlimited', label: { ko: '무제한 · OpenSea', en: 'Unlimited · OpenSea' } }],
    varUsd: 12340 },

  { id: 'h3', wallet: 'main', asset: 'aArbUSDC', name: 'Aave aUSDC (position)', chain: 'arb', kind: 'position',
    balance: '52,000', balanceUsd: 52010, priceNote: 'Aave V3',
    approval: { spender: 'aave', allowance: '52,000', type: 'Token' },
    risk: [{ kind: 'none', label: { ko: '한도 승인 · Aave', en: 'Capped · Aave' } }],
    varUsd: 52010 },

  { id: 'h4', wallet: 'main', asset: 'PEPE', name: 'Pepe', chain: 'eth', kind: 'token',
    balance: '8,400,000,000', balanceUsd: 0, priceNote: '— no price',
    approval: { spender: 'drainer', allowance: 'Unlimited', type: 'Token' },
    risk: [{ kind: 'fail', label: { ko: '드레이너 플래그 spender', en: 'Drainer-flagged spender' } }],
    varUsd: null },

  { id: 'h5', wallet: 'main', asset: 'BAYC', name: 'Bored Ape (NFT)', chain: 'eth', kind: 'nft',
    balance: '1 NFT', balanceUsd: 38600, priceNote: 'floor 12.9 ETH',
    approval: { spender: 'unknown2', allowance: 'All items', type: 'NFT' },
    risk: [{ kind: 'fail', label: { ko: '컬렉션 전체 승인 · 미상', en: 'Approve-all · unknown' } }],
    varUsd: 38600 },

  { id: 'h6', wallet: 'main', asset: 'ETH', name: 'Ether', chain: 'eth', kind: 'native',
    balance: '6.80', balanceUsd: 20370, priceNote: '$2,995',
    approval: null, risk: [], varUsd: null },

  { id: 'h7', wallet: 'hot', asset: 'USDT', name: 'Tether', chain: 'base', kind: 'token',
    balance: '6,140', balanceUsd: 6140, priceNote: '$1.00',
    approval: { spender: 'unknown1', allowance: 'Unlimited', type: 'Permit2' },
    risk: [{ kind: 'warn', label: { ko: 'Permit2 무제한 · 미상', en: 'Permit2 unlimited · unknown' } }],
    varUsd: 6120 },

  { id: 'h8', wallet: 'hot', asset: 'ARB', name: 'Arbitrum', chain: 'arb', kind: 'token',
    balance: '18,200', balanceUsd: 14560, priceNote: '$0.80',
    approval: { spender: 'uniswap', allowance: '20,000', type: 'Token' },
    risk: [{ kind: 'none', label: { ko: '한도 승인 · Uniswap', en: 'Capped · Uniswap' } }],
    varUsd: 16000 },

  { id: 'h9', wallet: 'treasury', asset: 'USDC', name: 'USD Coin', chain: 'eth', kind: 'token',
    balance: '480,000', balanceUsd: 480000, priceNote: '$1.00',
    approval: { spender: 'aave', allowance: '100,000', type: 'Token' },
    risk: [{ kind: 'none', label: { ko: '한도 승인 · Aave', en: 'Capped · Aave' } }],
    varUsd: 100000 },

  { id: 'h10', wallet: 'treasury', asset: 'ETH', name: 'Ether', chain: 'eth', kind: 'native',
    balance: '10.8', balanceUsd: 32400, priceNote: '$2,995',
    approval: null, risk: [], varUsd: null },

  { id: 'h11', wallet: 'dev', asset: 'OP', name: 'Optimism', chain: 'op', kind: 'token',
    balance: '4,200', balanceUsd: 7560, priceNote: '$1.80',
    approval: null, risk: [], varUsd: null },
];

/* ── findings feed (P0 — live SDK detections) ──
   verdict: 'fail' | 'warn' | 'pass' ; type label is always a detection (탐지)            */
const FINDINGS = [
  { id: 'f1', wallet: 'main', verdict: 'fail', ts: '2m', title: { ko: '블록리스트 매치', en: 'Blocklist match' },
    to: '0x8c5f···a91', value: '1.2 ETH ≈ $3,594', rule: 'rule#blocklist', source: 'chainalysis · tier-1', method: 'eth_sendTransaction' },
  { id: 'f2', wallet: 'main', verdict: 'fail', ts: '14m', title: { ko: '무제한 승인 차단', en: 'Unlimited approval blocked' },
    to: SPENDERS.unknown1.addr, value: 'MAX_UINT256', rule: 'rule#approve.cap', source: 'policy', method: 'approve()' },
  { id: 'f3', wallet: 'main', verdict: 'warn', ts: '18m', title: { ko: '가스 스파이크', en: 'Gas spike' },
    to: SPENDERS.aave.addr, value: '82 gwei (+38%)', rule: 'rule#gas.guard', source: 'policy', method: 'supply()' },
  { id: 'f4', wallet: 'hot', verdict: 'warn', ts: '41m', title: { ko: 'Permit2 무제한 서명', en: 'Permit2 unlimited sign' },
    to: SPENDERS.unknown1.addr, value: 'Unlimited', rule: 'rule#permit2.cap', source: 'policy', method: 'PermitSingle' },
  { id: 'f5', wallet: 'main', verdict: 'pass', ts: '52m', title: { ko: '스왑 통과', en: 'Swap passed' },
    to: SPENDERS.uniswap.addr, value: '4,200 USDC', rule: 'rule#allowlist', source: 'policy', method: 'swap()' },
  { id: 'f6', wallet: 'treasury', verdict: 'pass', ts: '1h 12m', title: { ko: '대량 송금 통과', en: 'Large transfer passed' },
    to: '0xB7d1···3f4a', value: '50,000 USDC', rule: 'rule#multisig', source: 'policy', method: 'transfer()' },
  { id: 'f7', wallet: 'hot', verdict: 'pass', ts: '2h 04m', title: { ko: '스왑 통과', en: 'Swap passed' },
    to: SPENDERS.uniswap.addr, value: '8,000 ARB', rule: 'rule#allowlist', source: 'policy', method: 'swap()' },
];

/* ── approvals table (P2 — revoke targets) ──
   priority score: ① fail/blocklist ② unlimited+active/unknown (even $0) ③ VaR desc        */
const APPROVALS = [
  { id: 'a1', wallet: 'main', asset: 'USDC', chain: 'eth', type: 'Token',
    allowance: 'Unlimited', spender: 'unknown1', varUsd: 184230, risk: 'fail' },
  { id: 'a2', wallet: 'main', asset: 'BAYC', chain: 'eth', type: 'NFT',
    allowance: 'All items', spender: 'unknown2', varUsd: 38600, risk: 'fail' },
  { id: 'a3', wallet: 'main', asset: 'PEPE', chain: 'eth', type: 'Token',
    allowance: 'Unlimited', spender: 'drainer', varUsd: null, risk: 'fail' },
  { id: 'a4', wallet: 'main', asset: 'WETH', chain: 'arb', type: 'Token',
    allowance: 'Unlimited', spender: 'opensea', varUsd: 12340, risk: 'warn' },
  { id: 'a5', wallet: 'hot', asset: 'USDT', chain: 'base', type: 'Permit2',
    allowance: 'Unlimited', spender: 'unknown1', varUsd: 6120, risk: 'warn' },
  { id: 'a6', wallet: 'main', asset: 'aArbUSDC', chain: 'arb', type: 'Token',
    allowance: '52,000', spender: 'aave', varUsd: 52010, risk: 'none' },
  { id: 'a7', wallet: 'hot', asset: 'ARB', chain: 'arb', type: 'Token',
    allowance: '20,000', spender: 'uniswap', varUsd: 16000, risk: 'none' },
  { id: 'a8', wallet: 'treasury', asset: 'USDC', chain: 'eth', type: 'Token',
    allowance: '100,000', spender: 'aave', varUsd: 100000, risk: 'none' },
  { id: 'a9', wallet: 'main', asset: 'USDC', chain: 'base', type: 'Delegation',
    allowance: 'EIP-7702', spender: 'unknown2', varUsd: null, risk: 'warn', delegation: true },
];

/* ── aggregate summary (P1/P2) ── */
const SUMMARY = {
  totalUsd: 751080,           // 4-wallet sum
  totalVarUsd: 30100,         // honest current exposure (Unlimited excluded from $ sum)
  failCount: 2,
  unlimitedCount: 4,
  updated: { ko: '방금 갱신', en: 'updated just now' },
};

Object.assign(window, { CHAINS, SPENDERS, WALLETS, CHAIN_BREAKDOWN, HOLDINGS, FINDINGS, APPROVALS, SUMMARY });
