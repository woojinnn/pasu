/**
 * Step 4 — results. Left: the s0 → s1 → s2 … state sequence with per-step diffs.
 * Right: the deny list, accumulated up to the current step, each showing WHERE
 * in the policy it was blocked (red-traced diagram, like History).
 */
import { PolicyDiagram } from "../../cedar/diagram/PolicyDiagram";

import { humanizeAddr } from "./mock-data";
import type { SimController } from "./useSimController";
import type { DenyView, StepView, WalletStateView } from "./types";

export function Step4Results({ c }: { c: SimController }) {
  const r = c.result;
  if (c.running || !r) {
    return (
      <div className="sw-step">
        <div className="sw-empty">{c.running ? "시뮬레이션 실행 중…" : "아직 실행하지 않았습니다."}</div>
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
        <h2>④ 결과</h2>
        <p>각 트랜잭션 적용 후 상태(s0 → s{total})와, 그때까지 누적된 차단을 봅니다.</p>
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
        <span className="sw-mut">{cur === 0 ? "초기 상태" : `TX ${cur} 적용 후 (총 ${total})`}</span>
      </div>

      <div className="sw-cols result">
        {/* ── state sequence + diff ── */}
        <section className="sw-result-state">
          {step && (
            <div className={`sw-stepbar ${step.verdict}`}>
              <span className="sw-stepbar-v">{verdictKo(step.verdict)}</span>
              <b>{step.label}</b>
              <span className="sw-mut">{name(step.fromWallet)} 실행</span>
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
            누적 차단 <b>{denies.length}</b>
            <span className="sw-mut"> · S0~S{cur}</span>
          </div>
          {denies.length === 0 ? (
            <div className="sw-empty small">이 지점까지 차단된 정책이 없습니다.</div>
          ) : (
            denies.map((d) => <DenyCard key={d.policyId} d={d} />)
          )}
        </aside>
      </div>
    </div>
  );
}

function verdictKo(v: StepView["verdict"]): string {
  return v === "fail" ? "차단" : v === "warn" ? "경고" : "통과";
}

function WalletSnap({ s, diff }: { s: WalletStateView; diff: StepView | null }) {
  const deltaBySym = new Map((diff?.diff.tokens ?? []).map((t) => [t.symbol, t]));
  return (
    <div className="sw-statecard">
      <div className="sw-statecard-head">
        <b>{s.name}</b>
        {diff && <span className="sw-pill changed">변경됨</span>}
      </div>
      <table className="sw-tokens">
        <tbody>
          {s.tokens.map((t) => {
            const dl = deltaBySym.get(t.symbol);
            return (
              <tr key={t.address}>
                <td className="sym">{t.symbol}</td>
                <td className="bal">{t.balance}</td>
                <td className={`delta ${dl?.sign ?? ""}`}>{dl ? dl.delta : ""}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
      {diff?.diff.gas && <div className="sw-mut sw-gas">가스 {diff.diff.gas}</div>}
      {diff?.diff.note && <div className="sw-mut sw-note">{diff.diff.note}</div>}
    </div>
  );
}

function DenyCard({ d }: { d: DenyView }) {
  return (
    <div className={`sw-denycard ${d.severity}`}>
      <div className="sw-denycard-head">
        <span className={`sw-sev ${d.severity}`}>{d.severity === "deny" ? "차단" : "경고"}</span>
        <b>{d.policyName}</b>
        <span className="sw-step-badge">S{d.step}</span>
      </div>
      <p className="sw-deny-reason">{d.reason}</p>
      <div className="sw-deny-note">차단 조건을 빨갛게 표시했어요 — 어디서 막혔는지</div>
      <div className="sw-deny-diagram">
        <PolicyDiagram ir={d.ir} highlightPaths={d.highlightPaths} humanizeLabel={humanizeAddr} compact />
      </div>
    </div>
  );
}
