/** Step 1 — pick wallets and see each wallet's current state (s0). */
import type { SimController } from "./useSimController";
import type { WalletStateView } from "./types";

const CHAINS: { id: string; label: string }[] = [
  { id: "eip155:1", label: "Ethereum" },
  { id: "eip155:42161", label: "Arbitrum" },
];

export function Step1Wallets({ c }: { c: SimController }) {
  return (
    <div className="sw-step">
      <header className="sw-step-head">
        <h2>① 지갑 선택</h2>
        <p>시뮬레이션할 지갑을 고르면, 해당 지갑들의 현재 상태(state)를 불러옵니다.</p>
      </header>

      <div className="sw-cols">
        <aside className="sw-pick">
          <div className="sw-pick-head">
            <span>지갑</span>
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
            <div className="sw-empty">왼쪽에서 지갑을 선택하세요.</div>
          ) : (
            c.selectedStates.map((s) => <StateCard key={s.address} s={s} />)
          )}
        </section>
      </div>
    </div>
  );
}

function StateCard({ s }: { s: WalletStateView }) {
  return (
    <div className="sw-statecard">
      <div className="sw-statecard-head">
        <b>{s.name}</b>
        <span className="sw-mut">
          {s.address.slice(0, 6)}…{s.address.slice(-4)}
        </span>
        <span className="sw-pills">
          <span className="sw-pill">토큰 {s.tokens.length}</span>
          <span className="sw-pill">포지션 {s.positions.length}</span>
          <span className="sw-pill">승인 {s.approvals.length}</span>
        </span>
      </div>
      <table className="sw-tokens">
        <tbody>
          {s.tokens.map((t) => (
            <tr key={t.address}>
              <td className="sym">{t.symbol}</td>
              <td className="bal">{t.balance}</td>
              <td className="usd">{t.usd ?? ""}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {s.positions.length > 0 && (
        <div className="sw-substate">
          {s.positions.map((p) => (
            <span key={p.id} className="sw-tag pos">
              {p.label}
            </span>
          ))}
        </div>
      )}
      {s.approvals.length > 0 && (
        <div className="sw-substate">
          {s.approvals.map((a) => (
            <span key={a.id} className={`sw-tag appr${a.unlimited ? " unl" : ""}`}>
              {a.token} → {a.spender}
              {a.unlimited ? " (무제한)" : ""}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
