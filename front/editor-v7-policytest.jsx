// editor-v7-policytest.jsx — floating Policy test panel (drag/pin/close),
// 3 sample TX, ALLOW/DENY verdict + per-condition pass/fail + failed highlight.
// Plus Live Cedar floating panel and Code-mode split.

const { useState: useSPT7, useRef: useRPT7, useEffect: useEPT7 } = React;

// draggable floating shell
function V7Float({ title, x, y, w, onClose, pinned, onPin, children }) {
  const [pos, setPos] = useSPT7({ x, y });
  const drag = useRPT7(null);
  const onDown = (e) => {
    if (e.target.closest('.v7-float-ib')) return;
    drag.current = { sx: e.clientX, sy: e.clientY, ox: pos.x, oy: pos.y };
    const move = (ev) => { if (!drag.current) return; setPos({ x: drag.current.ox + (ev.clientX - drag.current.sx), y: drag.current.oy + (ev.clientY - drag.current.sy) }); };
    const up = () => { drag.current = null; window.removeEventListener('mousemove', move); window.removeEventListener('mouseup', up); };
    window.addEventListener('mousemove', move); window.addEventListener('mouseup', up);
  };
  return (
    <div className={`v7-float ${pinned ? 'pinned' : ''}`} style={{ left: pos.x, top: pos.y, width: w }}>
      <div className="v7-float-h" onMouseDown={onDown}>
        <span className="v7-float-t">{title}</span>
        <div className="v7-float-tools">
          {onPin && <button className={`v7-float-ib ${pinned ? 'on' : ''}`} onClick={onPin} title="핀"><V7I.pin /></button>}
          {onClose && <button className="v7-float-ib" onClick={onClose} title="닫기"><V7I.x /></button>}
        </div>
      </div>
      <div className="v7-float-body">{children}</div>
    </div>
  );
}

const TX_ROWS = [
  { k: 'meta.from', src: 'meta', key: 'from', mono: true },
  { k: 'context.recipient', src: 'context', key: 'recipient', mono: true },
  { k: 'context.slippageBp', src: 'context', key: 'slippageBp' },
  { k: 'context.priceImpactBp', src: 'context', key: 'priceImpactBp' },
  { k: 'enrichment.validityDeltaSec', src: 'enrichment', key: 'validityDeltaSec', dashed: true, suffix: ' sec' },
  { k: 'enrichment.recipientIsContract', src: 'enrichment', key: 'recipientIsContract', dashed: true },
];
function shortAddr(s) { return typeof s === 'string' && s.length > 14 ? s.slice(0, 6) + '…' + s.slice(-4) : String(s); }

// the panel content (shared by floating + code split)
function V7PTContent({ doc, fxId, onFx, verdict }) {
  const fx = V7_SAMPLE_TX.find(f => f.id === fxId) || V7_SAMPLE_TX[0];
  const cls = verdict.verdict === 'ALLOW' ? 'allow' : 'deny';
  const root = doc.nodes[doc.rootId];
  const guards = (root.childIds || []).map(id => doc.nodes[id]).filter(Boolean);
  const failedSet = new Set(verdict.failed.map(f => f.id));
  const trippedParams = new Set();
  verdict.failed.forEach(f => {
    const collect = (id) => { const n = doc.nodes[id]; if (!n) return; if (n.type === 'predicate') trippedParams.add(n.param); (n.childIds || []).forEach(collect); if (n.childId) collect(n.childId); };
    collect(f.id);
  });

  return (
    <>
      <div className={`v7-verdict ${cls}`}>
        <div className="v7-verdict-top">
          <span className="v7-vkw">{verdict.verdict}</span>
          <span className="v7-vmatch">permit match: {String(verdict.permitMatch)}</span>
        </div>
        <div className="v7-vmsg">
          {verdict.verdict === 'ALLOW'
            ? <>모든 안전 조건 충족 — 트랜잭션 <b>허용</b>.</>
            : <>안전 조건 미충족 → deny-by-default. <span className="mono">"{doc.denyMessage}"</span></>}
        </div>
      </div>

      <div>
        <div className="v7-sec-h">안전 조건 <span className="sub">when 절 · {guards.length}개</span></div>
        <div className="v7-conds">
          {guards.map(g => {
            const passed = verdict.truth[g.id] !== false;
            const off = g.enabled === false;
            return (
              <div key={g.id} className={`v7-cond ${off ? 'off' : (passed ? '' : 'fail')}`}>
                <span className="v7-cond-id">{g.guardId || ''}</span>
                <span className="v7-cond-lab">{g.label || v7Display(g.param, doc.locale)}</span>
                <span className={`v7-cond-res ${off ? '' : (passed ? 'pass' : 'fail')}`}>{off ? 'off' : (passed ? 'PASS' : 'FAIL')}</span>
              </div>
            );
          })}
        </div>
      </div>

      <div>
        <div className="v7-sec-h">샘플 트랜잭션 <span className="sub">{V7_SAMPLE_TX.length}건</span></div>
        <div className="v7-fxs">
          {V7_SAMPLE_TX.map(f => (
            <button key={f.id} className={`v7-fx ${fxId === f.id ? 'on' : ''}`} onClick={() => onFx(f.id)}>
              <span className={`v7-fx-d ${f.expected.verdict === 'ALLOW' ? 'allow' : 'deny'}`} />
              <span className="v7-fx-lab">{f.label}</span>
              <span className="v7-fx-v">{f.expected.verdict}</span>
            </button>
          ))}
        </div>
      </div>

      <div>
        <div className="v7-sec-h">tx context <span className="sub">스키마 필드</span></div>
        <div className="v7-tx">
          {TX_ROWS.map(r => {
            const raw = fx[r.src] ? fx[r.src][r.key] : undefined;
            if (raw === undefined) return null;
            let val = raw; if (r.suffix) val = String(val) + r.suffix; if (r.mono) val = shortAddr(val);
            return (
              <div key={r.k} className={`v7-tx-row ${r.dashed ? 'dashed' : ''} ${trippedParams.has(r.k) ? 'trip' : ''}`}>
                <span className="v7-tx-k">{r.k}</span>
                <span className={`v7-tx-v ${r.mono ? 'mono' : ''}`}>{String(val)}</span>
              </div>
            );
          })}
        </div>
      </div>
    </>
  );
}

function V7PolicyTest({ doc, fxId, onFx, verdict, onClose, x, y }) {
  const [pinned, setPinned] = useSPT7(false);
  return (
    <V7Float title="Policy test" x={x} y={y} w={340} onClose={onClose} pinned={pinned} onPin={() => setPinned(p => !p)}>
      <div className="v7-pt-panel" style={{ width: '100%', display: 'flex', flexDirection: 'column', gap: 14 }}>
        <V7PTContent doc={doc} fxId={fxId} onFx={onFx} verdict={verdict} />
      </div>
    </V7Float>
  );
}

// Live Cedar floating panel
function tok7(text) {
  const out = [];
  const re = /(\/\/.*$)|(\bpermit\b|\bwhen\b|\bprincipal\b|\baction\b|\bresource\b|\bhas\b|\btrue\b|\bfalse\b)/g;
  let last = 0, m;
  while ((m = re.exec(text)) !== null) {
    if (m.index > last) out.push({ t: text.slice(last, m.index), c: '' });
    out.push({ t: m[0], c: m[1] ? 'cmt' : 'kw' });
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push({ t: text.slice(last), c: '' });
  return out;
}
function V7Cedar({ doc }) {
  const cedar = v7ToCedar(doc);
  return (
    <div className="v7-code-body" style={{ background: 'var(--slate-900)', borderRadius: 10, paddingTop: 10, paddingBottom: 10 }}>
      {cedar.lines.map(l => (
        <div key={l.n} className={`v7-cl ${l.kind === 'guard' ? 'guard' : ''}`}>
          <span className="v7-cl-gut">{l.n}</span>
          <span className="v7-cl-tx">{tok7(l.text).map((tk, i) => <span key={i} className={tk.c}>{tk.t}</span>)}</span>
          {l.kind === 'guard' && l.guardId && <span className="tag">{l.guardId}</span>}
        </div>
      ))}
    </div>
  );
}
function V7LiveCedar({ doc, onClose, x, y }) {
  const [pinned, setPinned] = useSPT7(false);
  return (
    <V7Float title="Live Cedar" x={x} y={y} w={420} onClose={onClose} pinned={pinned} onPin={() => setPinned(p => !p)}>
      <V7Cedar doc={doc} />
    </V7Float>
  );
}

Object.assign(window, { V7Float, V7PTContent, V7PolicyTest, V7Cedar, V7LiveCedar, tok7 });
