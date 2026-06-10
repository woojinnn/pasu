/** Step 3 — build the transaction queue (raw calldata rows, in order). */
import type { SimController } from "./useSimController";

export function Step3TxQueue({ c }: { c: SimController }) {
  const walletName = (addr: string) => c.wallets.find((w) => w.address === addr)?.name ?? "";
  return (
    <div className="sw-step">
      <header className="sw-step-head">
        <h2>③ 트랜잭션 큐</h2>
        <p>순서대로 실행할 트랜잭션을 추가하세요. 위에서 아래로 누적 적용됩니다.</p>
      </header>

      <div className="sw-txlist">
        {c.txRows.map((r, i) => (
          <div key={r.id} className="sw-tx">
            <div className="sw-tx-head">
              <span className="sw-tx-num">{String(i + 1).padStart(2, "0")}</span>
              <input
                className="sw-tx-label"
                value={r.label}
                placeholder="라벨"
                onChange={(e) => c.updateRow(r.id, { label: e.target.value })}
              />
              <button
                type="button"
                className="sw-iconbtn danger"
                onClick={() => c.removeRow(r.id)}
                disabled={c.txRows.length <= 1}
                title="삭제"
              >
                ✕
              </button>
            </div>
            <div className="sw-tx-grid">
              <label>
                <span>from (지갑)</span>
                <input
                  list="sw-wallets"
                  value={r.fromWallet}
                  onChange={(e) => c.updateRow(r.id, { fromWallet: e.target.value })}
                  placeholder="0x…"
                />
                {r.fromWallet && <em className="sw-mut">{walletName(r.fromWallet)}</em>}
              </label>
              <label>
                <span>to (컨트랙트)</span>
                <input value={r.to} onChange={(e) => c.updateRow(r.id, { to: e.target.value })} placeholder="0x…" />
              </label>
              <label>
                <span>value (wei)</span>
                <input value={r.value} onChange={(e) => c.updateRow(r.id, { value: e.target.value })} placeholder="0" />
              </label>
              <label className="wide">
                <span>calldata</span>
                <input
                  value={r.calldata}
                  onChange={(e) => c.updateRow(r.id, { calldata: e.target.value })}
                  placeholder="0x…"
                />
              </label>
            </div>
          </div>
        ))}
      </div>

      <datalist id="sw-wallets">
        {[...c.selected].map((addr) => (
          <option key={addr} value={addr}>
            {walletName(addr)}
          </option>
        ))}
      </datalist>

      <button type="button" className="sw-btn ghost add" onClick={c.addRow}>
        + 트랜잭션 추가
      </button>
    </div>
  );
}
