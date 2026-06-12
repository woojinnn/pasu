/**
 * Wallet selector + chain picker for the simulation page.
 *
 * The simulator no longer accepts an arbitrary address — it operates on the
 * logged-in user's registered wallets only. This panel:
 *   - Pulls `listWallets()` (auth-scoped to the active user) and renders one
 *     row per wallet with a checkbox.
 *   - Surfaces a page-level chain selector populated from the union of
 *     every selected wallet's `chains`. Picking a chain narrows the
 *     downstream state views (so multi-chain wallets don't show tokens the
 *     simulation can't act on).
 *   - Reports `selectedAddresses` and `chain` upward so the page-level
 *     orchestrator can fan state queries out per wallet.
 */

import { useMemo } from "react";
import { Trans, useTranslation } from "react-i18next";

import { shortAddr } from "./state-view";
import type { WalletId } from "../../server-api";

export interface WalletSelectorPanelProps {
  /** All registered wallets for the active user. Empty when the user has
   *  none yet (the panel surfaces a hint pointing to the Wallets page). */
  wallets: ReadonlyArray<WalletId>;
  /** Lowercase 0x addresses of currently-selected wallets. */
  selected: Set<string>;
  /** Called when a checkbox flips. */
  toggle: (addr: string) => void;
  selectAll: () => void;
  clearAll: () => void;
  /** Active CAIP-2 chain string (e.g. `"eip155:1"`). The picker constrains
   *  itself to the union of chains across SELECTED wallets, but reports
   *  upward unconstrained so the page renders the same chain across panels. */
  chain: string;
  setChain: (chain: string) => void;
  /** Disabled while a run is in flight. */
  isRunning: boolean;
}

export function WalletSelectorPanel(props: WalletSelectorPanelProps) {
  const { t } = useTranslation("simulation");
  const {
    wallets,
    selected,
    toggle,
    selectAll,
    clearAll,
    chain,
    setChain,
    isRunning,
  } = props;

  // Union of chains across selected wallets. When nothing is selected we
  // fall back to the union of every wallet's chains so the user can pick
  // a chain BEFORE choosing wallets if they want.
  const chainOptions = useMemo(() => {
    const set = new Set<string>();
    const sourceWallets =
      selected.size === 0 ? wallets : wallets.filter((w) => selected.has(w.address.toLowerCase()));
    for (const w of sourceWallets) {
      for (const c of w.chains) set.add(c);
    }
    if (set.size === 0) set.add("eip155:1");
    return [...set].sort();
  }, [wallets, selected]);

  return (
    <div className="sim-card wallet-selector-card">
      <div className="card-head">
        <h3>{t("historic.wallets.title")}</h3>
        <span className="meta">
          {selected.size} / {wallets.length}
        </span>
      </div>

      <div className="wallet-selector-toolbar">
        <label className="wallet-selector-chain">
          <span>chain</span>
          <select
            value={chain}
            onChange={(e) => setChain(e.target.value)}
            disabled={isRunning}
          >
            {chainOptions.map((c) => (
              <option key={c} value={c}>
                {prettyChain(c)}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          className="btn xs"
          onClick={selectAll}
          disabled={isRunning || wallets.length === 0}
        >
          {t("historic.wallets.selectAll")}
        </button>
        <button
          type="button"
          className="btn xs ghost"
          onClick={clearAll}
          disabled={isRunning || selected.size === 0}
        >
          {t("historic.wallets.clearAll")}
        </button>
      </div>

      {wallets.length === 0 ? (
        <div className="muted-line">
          <Trans t={t} i18nKey="historic.wallets.empty" components={{ code: <code /> }} />
        </div>
      ) : (
        <ul className="wallet-selector-list">
          {wallets.map((w) => {
            const addr = w.address.toLowerCase();
            const on = selected.has(addr);
            const supportsChain = w.chains.includes(chain);
            return (
              <li
                key={addr}
                className={`wallet-row ${on ? "is-on" : ""} ${supportsChain ? "" : "is-off-chain"}`}
              >
                <label className="wallet-row-label">
                  <input
                    type="checkbox"
                    checked={on}
                    onChange={() => toggle(addr)}
                    disabled={isRunning}
                  />
                  <code className="wallet-row-addr">{shortAddr(w.address)}</code>
                  <span className="wallet-row-chains">
                    {w.chains.map((c) => (
                      <span
                        key={c}
                        className={`wallet-chain-pill ${c === chain ? "is-active" : ""}`}
                      >
                        {prettyChain(c)}
                      </span>
                    ))}
                  </span>
                </label>
                {!supportsChain && on && (
                  <span className="wallet-row-warn">
                    {t("historic.wallets.chainUnsupported", { chain: prettyChain(chain) })}
                  </span>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

// ── helpers ────────────────────────────────────────────────────────────────

/** Human-readable chain label for the dropdown. CAIP-2 input
 *  (`eip155:1`) → readable name (`Ethereum`); unknown chains fall back
 *  to the raw string. */
function prettyChain(caip: string): string {
  const m = caip.match(/^eip155:(\d+)$/);
  if (!m) return caip;
  switch (m[1]) {
    case "1":
      return "Ethereum";
    case "10":
      return "Optimism";
    case "137":
      return "Polygon";
    case "8453":
      return "Base";
    case "42161":
      return "Arbitrum";
    case "11155111":
      return "Sepolia";
    default:
      return `eip155:${m[1]}`;
  }
}
