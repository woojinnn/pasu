/** Step 1 — pick wallets and see each wallet's current state (s0). */
import { useTranslation } from "react-i18next";

import { StateDashboard } from "./StateDashboard";
import type { SimController } from "./useSimController";

const CHAINS: { id: string; label: string }[] = [
  { id: "eip155:1", label: "Ethereum" },
  { id: "eip155:42161", label: "Arbitrum" },
];

export function Step1Wallets({ c }: { c: SimController }) {
  const { t } = useTranslation("simulation");
  return (
    <div className="sw-step">
      <header className="sw-step-head">
        <h2>{t("wizard.step1.title")}</h2>
        <p>{t("wizard.step1.desc")}</p>
      </header>

      <div className="sw-cols">
        <aside className="sw-pick">
          <div className="sw-pick-head">
            <span>{t("wizard.step1.wallets")}</span>
            <select className="sw-chain" value={c.chain} onChange={(e) => c.setChain(e.target.value)}>
              {CHAINS.map((ch) => (
                <option key={ch.id} value={ch.id}>
                  {ch.label}
                </option>
              ))}
            </select>
          </div>
          {c.wallets.map((w) => (
            <label key={w.address} className={`sw-wallet${c.selected.has(w.address) ? " on" : ""}`}>
              <input type="checkbox" checked={c.selected.has(w.address)} onChange={() => c.toggleWallet(w.address)} />
              <span className="sw-wallet-name">{w.name}</span>
              <span className="sw-wallet-addr">
                {w.address.slice(0, 6)}…{w.address.slice(-4)}
              </span>
            </label>
          ))}
        </aside>

        <section className="sw-states">
          {c.selectedStates.length === 0 ? (
            <div className="sw-empty">{t("wizard.step1.empty")}</div>
          ) : (
            c.selectedStates.map((s) => <StateDashboard key={s.address} s={s} />)
          )}
        </section>
      </div>
    </div>
  );
}
