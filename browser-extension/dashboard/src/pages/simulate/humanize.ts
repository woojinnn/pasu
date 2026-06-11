/**
 * Address → friendly chip label for the simulation diagram. Maps known mainnet
 * token addresses to their symbol; everything else is left as a shortened hex.
 */

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
