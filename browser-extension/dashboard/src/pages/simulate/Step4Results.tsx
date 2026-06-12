/**
 * Step 4 — results. Left: the s0 → s1 → s2 … state sequence with per-step diffs.
 * Right: the deny list, accumulated up to the current step, each showing WHERE
 * in the policy it was blocked (red-traced diagram, like History).
 */
import { useTranslation } from "react-i18next";

import { PolicyDiagram } from "../../cedar/diagram/PolicyDiagram";

import { humanizeAddr } from "./humanize";
import type { SimController } from "./useSimController";
import type { DenyView, StepView, WalletStateView } from "./types";

export function Step4Results({ c }: { c: SimController }) {
  const { t } = useTranslation("simulation");
  const r = c.result;
  if (c.running || !r) {
    return (
      <div className="sw-step">
        <div className="sw-empty">{c.running ? t("wizard.step4.runningEmpty") : t("wizard.step4.notRunYet")}</div>
      </div>
    );
  }

  const total = r.steps.length; // s0..s{total}
  const cur = c.cursorIdx;
  const step: StepView | null = cur > 0 ? r.steps[cur - 1] ?? null : null;
  const denies = c.cumulativeDenies(cur);
  const name = (addr: string) => c.wallets.find((w) => w.address === addr)?.name ?? addr.slice(0, 8);

  return (
    <div className="sw-step">
      <header className="sw-step-head">
        <h2>{t("wizard.step4.title")}</h2>
        <p>{t("wizard.step4.desc", { total })}</p>
      </header>

      <div className="sw-scrub">
        {Array.from({ length: total + 1 }, (_, i) => (
          <button
            key={i}
            type="button"
            className={`sw-scrub-btn${i === cur ? " on" : ""}`}
            onClick={() => c.setCursorIdx(i)}
          >
            S{i}
          </button>
        ))}
        <span className="sw-mut">
          {cur === 0 ? t("wizard.step4.initialState") : t("wizard.step4.afterTx", { n: cur, total })}
        </span>
      </div>

      <div className="sw-cols result">
        {/* ── state sequence + diff ── */}
        <section className="sw-result-state">
          {step && (
            <div className={`sw-stepbar ${step.verdict}`}>
              <span className="sw-stepbar-v">{t(`wizard.step4.verdict.${step.verdict}`)}</span>
              <b>{step.label}</b>
              <span className="sw-mut">{t("wizard.step4.executedBy", { name: name(step.fromWallet) })}</span>
            </div>
          )}
          {r.wallets.map((addr) => {
            const snap = r.histories[addr]?.[cur];
            if (!snap) return null;
            const isOwner = step?.fromWallet === addr;
            return <WalletSnap key={addr} s={snap} diff={isOwner ? step : null} />;
          })}
        </section>

        {/* ── cumulative deny + diagram ── */}
        <aside className="sw-result-deny">
          <div className="sw-deny-head">
            {t("wizard.step4.cumulativeBlocked")} <b>{denies.length}</b>
            <span className="sw-mut"> · S0~S{cur}</span>
          </div>
          {denies.length === 0 ? (
            <div className="sw-empty small">{t("wizard.step4.noBlocksYet")}</div>
          ) : (
            denies.map((d) => <DenyCard key={d.policyId} d={d} />)
          )}
        </aside>
      </div>
    </div>
  );
}

function WalletSnap({ s, diff }: { s: WalletStateView; diff: StepView | null }) {
  const { t } = useTranslation("simulation");
  const deltaBySym = new Map((diff?.diff.tokens ?? []).map((tk) => [tk.symbol, tk]));
  return (
    <div className="sw-statecard">
      <div className="sw-statecard-head">
        <b>{s.name}</b>
        {diff && <span className="sw-pill changed">{t("wizard.step4.changed")}</span>}
      </div>
      <table className="sw-tokens">
        <tbody>
          {s.tokens.map((tk) => {
            const dl = deltaBySym.get(tk.symbol);
            return (
              <tr key={tk.address}>
                <td className="sym">{tk.symbol}</td>
                <td className="bal">{tk.balance}</td>
                <td className={`delta ${dl?.sign ?? ""}`}>{dl ? dl.delta : ""}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {diff?.diff.gas && <div className="sw-mut sw-gas">{t("wizard.step4.gas", { gas: diff.diff.gas })}</div>}
      {diff?.diff.note && <div className="sw-mut sw-note">{diff.diff.note}</div>}
    </div>
  );
}

function DenyCard({ d }: { d: DenyView }) {
  const { t } = useTranslation("simulation");
  return (
    <div className={`sw-denycard ${d.severity}`}>
      <div className="sw-denycard-head">
        <span className={`sw-sev ${d.severity}`}>{t(`wizard.step4.severity.${d.severity}`)}</span>
        <b>{d.policyName}</b>
        <span className="sw-step-badge">S{d.step}</span>
      </div>
      <p className="sw-deny-reason">{d.reason}</p>
      <div className="sw-deny-note">{t("wizard.step4.blockedTraceNote")}</div>
      <div className="sw-deny-diagram">
        <PolicyDiagram ir={d.ir} highlightPaths={d.highlightPaths} humanizeLabel={humanizeAddr} compact />
      </div>
    </div>
  );
}
