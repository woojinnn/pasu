// editor-v6-eval.jsx — schema-driven condition body + operator/value popovers +
// real Policy Test panel (3-state verdict, per-guard, schema TX context) + Code mode.
// Backed by the v6 engine in editor-v6-state.js (V6_OPS / v6Evaluate / v6ToCedar).

const { useState: useSE6, useRef: useRE6, useEffect: useEE6 } = React;

// ─── operator popover (fieldKind-bound) ───
function V6OpPop({ node, onChange, onClose }) {
  const opts = V6_OPS[node.fieldKind] || ['eq', 'neq'];
  const ref = useRE6(null);
  useEE6(() => {
    const h = (e) => { if (ref.current && !ref.current.contains(e.target)) onClose(); };
    setTimeout(() => window.addEventListener('mousedown', h), 0);
    return () => window.removeEventListener('mousedown', h);
  }, [onClose]);
  return (
    <div className="pop" ref={ref}>
      {opts.map(op => (
        <div key={op} className={`pop-item ${node.op === op ? 'on' : ''}`} onClick={() => { onChange(op); onClose(); }}>
          <span style={{ fontFamily: 'var(--ff-mono)', minWidth: 44, display: 'inline-block' }}>{V6_OP_SYM[op] || op}</span>
          <span style={{ color: 'var(--slate-400)', fontSize: 11 }}>{op}</span>
        </div>
      ))}
    </div>
  );
}

// ─── value popover (by fieldKind) ───
function V6ValPop({ node, onChange, onClose }) {
  const ref = useRE6(null);
  const [draft, setDraft] = useSE6(node.value || { kind: 'num', text: '0' });
  const commit = () => { onChange(draft); onClose(); };
  useEE6(() => {
    const h = (e) => { if (ref.current && !ref.current.contains(e.target)) commit(); };
    setTimeout(() => window.addEventListener('mousedown', h), 0);
    return () => window.removeEventListener('mousedown', h);
  }, [draft]);
  const fk = node.fieldKind;
  const isNum = fk === 'primitive.Long' || fk === 'primitive.decimal';
  const isStr = fk === 'primitive.String' || fk === 'ref';

  return (
    <div className="val-pop-card" ref={ref}>
      <div className="vp-h">값 · {fk.replace('primitive.', '')}</div>
      {isStr && (
        <div className="vp-tab-row">
          <button className={draft.kind === 'ref' ? '' : 'on'} onClick={() => setDraft({ kind: 'str', text: draft.kind === 'str' ? draft.text : '' })}>리터럴</button>
          <button className={draft.kind === 'ref' ? 'on' : ''} onClick={() => setDraft({ kind: 'ref', text: draft.kind === 'ref' ? draft.text : '@meta.from' })}>@ref</button>
        </div>
      )}
      {isNum && (
        <>
          <input className="vp-input" type="text" autoFocus value={draft.text}
            onChange={(e) => setDraft({ ...draft, kind: 'num', text: e.target.value })}
            onKeyDown={(e) => e.key === 'Enter' && commit()} />
          {draft.unit && <div className="vp-row">단위: <span style={{ fontFamily: 'var(--ff-mono)', color: 'var(--slate-700)' }}>{draft.unit}</span></div>}
        </>
      )}
      {isStr && draft.kind !== 'ref' && (
        <input className="vp-input" type="text" autoFocus value={draft.text}
          onChange={(e) => setDraft({ ...draft, kind: 'str', text: e.target.value })}
          onKeyDown={(e) => e.key === 'Enter' && commit()} placeholder="0x… / USDC" />
      )}
      {isStr && draft.kind === 'ref' && (
        <>
          <input className="vp-input" type="text" autoFocus value={draft.text}
            onChange={(e) => setDraft({ ...draft, kind: 'ref', text: e.target.value })} placeholder="@meta.from / @context.tokenIn" />
          <div className="vp-row" style={{ color: 'var(--cyan-700)' }}>동적 참조 — 평가 시점에 TX에서 읽음</div>
        </>
      )}
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 8 }}>
        <button className="btn-secondary" onClick={commit}>적용</button>
      </div>
    </div>
  );
}

// ─── condition body (chips inside a condition node) ───
function V6ConditionBody({ node, onPatch, onDelete }) {
  const [opOpen, setOpOpen] = useSE6(false);
  const [valOpen, setValOpen] = useSE6(false);
  const noVal = node.op === 'isTrue' || node.op === 'isFalse' || node.op === 'isEmpty';
  const v = node.value || {};
  const valText = v.kind === 'ref' ? v.text : (v.text != null ? v.text : '');
  const cycleAbsence = () => {
    const order = ['treatAsFalse', 'treatAsTrue', 'skip'];
    const i = order.indexOf(node.absence || 'treatAsFalse');
    onPatch({ absence: order[(i + 1) % order.length] });
  };
  return (
    <>
      <span className={`seg-chip ${node.custom ? 'custom' : ''}`} title={node.param}>
        <span>{node.label}</span>
      </span>
      <span style={{ position: 'relative', display: 'inline-flex' }}>
        <button className="op-btn" onClick={(e) => { e.stopPropagation(); setOpOpen(o => !o); }}>
          <span>{V6_OP_SYM[node.op] || node.op}</span>
          <V5I.caretDown style={{ width: 10, height: 10 }} />
        </button>
        {opOpen && <V6OpPop node={node} onChange={(op) => onPatch({ op })} onClose={() => setOpOpen(false)} />}
      </span>
      {!noVal && (
        <span style={{ position: 'relative', display: 'inline-flex' }}>
          <button className={`val-chip k-${v.kind || 'num'}`} onClick={(e) => { e.stopPropagation(); setValOpen(o => !o); }}>
            <span>{valText}</span>
            {v.unit && <span className="val-u">{v.unit}</span>}
          </button>
          {valOpen && <V6ValPop node={node} onChange={(val) => onPatch({ value: val })} onClose={() => setValOpen(false)} />}
        </span>
      )}
      {node.custom && (
        <button className="abs-pill" title="값 부재 시 처리 — 클릭하여 순환" onClick={(e) => { e.stopPropagation(); cycleAbsence(); }}>
          <span className="abs-k">absence</span><span className="abs-v">{node.absence || 'treatAsFalse'}</span>
        </button>
      )}
      <span className="note-pill" contentEditable suppressContentEditableWarning
        onBlur={(e) => onPatch({ note: e.target.textContent || null })}>
        {node.note || '근거…'}
      </span>
      <button className="node-x" onClick={(e) => { e.stopPropagation(); onDelete(); }} title="삭제">
        <V5I.x style={{ width: 12, height: 12 }} />
      </button>
    </>
  );
}

// ─── TX context rows (schema field names) ───
const V6_TX_ROWS = [
  { k: 'meta.from', src: 'meta', key: 'from', mono: true },
  { k: 'meta.to', src: 'meta', key: 'to', mono: true },
  { k: 'context.recipient', src: 'context', key: 'recipient', mono: true },
  { k: 'context.slippageBp', src: 'context', key: 'slippageBp' },
  { k: 'context.priceImpactBp', src: 'context', key: 'priceImpactBp' },
  { k: 'enrichment.validityDeltaSec', src: 'enrichment', key: 'validityDeltaSec', dashed: true, suffix: ' sec' },
  { k: 'enrichment.recipientIsContract', src: 'enrichment', key: 'recipientIsContract', dashed: true },
  { k: 'enrichment.effectiveRateVsOracleBps', src: 'enrichment', key: 'effectiveRateVsOracleBps', dashed: true },
  { k: 'enrichment.totalInputUsd', src: 'enrichment', key: 'totalInputUsd', dashed: true, prefix: '$' },
];

function shortAddr(s) { return typeof s === 'string' && s.length > 14 ? s.slice(0, 6) + '…' + s.slice(-4) : s; }

// ─── Policy Test panel ───
function V6PolicyTest({ inline, onClose, fixtures, selectedFxId, onSelectFx, verdict, state, onFocusGuard, onReevaluate, draftCount }) {
  const fx = fixtures.find(f => f.id === selectedFxId) || fixtures[0];
  const tone = verdict.verdictTone;                      // deny | warn | normal
  const cls = tone === 'deny' ? 'deny' : tone === 'warn' ? 'warn' : 'pass';
  const trippedParams = new Set(
    verdict.matchedLeafIds.map(id => state.nodes[id]).filter(n => n && n.kind === 'condition').map(n => n.param)
  );
  const trueGuards = verdict.guards.filter(g => g.result);

  const inner = (
    <div className="pt-scroll">
      <div className={`pt-verdict ${cls}`}>
        <div className="pt-v-top">
          <span className="pt-v-kw">{verdict.decision.kind === 'Deny' ? 'DENY' : verdict.decision.kind === 'Warn' ? 'WARN' : 'ALLOW'}</span>
          <span className="pt-v-sev">{verdict.decision.severity}</span>
          <span style={{ flex: 1 }} />
          <button className="pt-btn" style={{ flex: 'none', padding: '4px 9px' }} onClick={onReevaluate}>
            <V5I.play style={{ width: 12, height: 12 }} /> Re
          </button>
        </div>
        <div className="pt-v-reason">"{verdict.decision.reason}"</div>

        <div className="pt-v-trace-k">가드 (deny &gt; warn &gt; normal)</div>
        <div className="pt-guards">
          {verdict.guards.map(g => (
            <button key={g.id} className={`pt-guard ${g.result ? 'on' : ''} t-${g.tone}`} onClick={() => onFocusGuard(g.id)}>
              <span className="pg-id">{g.id}</span>
              <span className="pg-tone">{g.tone === 'deny' ? '차단' : g.tone === 'warn' ? '경고' : '정상'}</span>
              <span className="pg-lab">{g.label}</span>
              <span className={`pg-res ${g.result ? 'fail' : 'pass'}`}>{g.result ? 'TRIP' : 'pass'}</span>
            </button>
          ))}
          {verdict.guards.length === 0 && <div className="pt-v-none">연결된 가드 없음</div>}
        </div>
      </div>

      <div>
        <div className="pt-section-h">샘플 트랜잭션 · {fixtures.length}건</div>
        <div className="pt-fxs">
          {fixtures.map(f => {
            const ev = f.expected && f.expected.verdict;
            const d = ev === 'DENY' ? 'deny' : ev === 'WARN' ? 'warn' : 'pass';
            return (
              <button key={f.id} className={`pt-fx ${selectedFxId === f.id ? 'on' : ''}`} onClick={() => onSelectFx(f.id)}>
                <span className={`pt-fx-d ${d}`} />
                <span>{f.label}</span>
                <span style={{ flex: 1 }} />
                <span className="pt-fx-v">{ev}</span>
              </button>
            );
          })}
        </div>
      </div>

      <div>
        <div className="pt-section-h">tx context · 스키마 필드</div>
        <div className="pt-tx">
          {V6_TX_ROWS.map(r => {
            const raw = fx.tx[r.src] ? fx.tx[r.src][r.key] : undefined;
            if (raw === undefined) return null;
            let val = raw;
            if (r.prefix) val = r.prefix + val;
            if (r.suffix) val = String(val) + r.suffix;
            if (r.mono) val = shortAddr(val);
            return (
              <div key={r.k} className={`pt-row ${r.dashed ? 'dashed' : ''} ${trippedParams.has(r.k) ? 'warn' : ''}`}>
                <span className="pt-row-k">{r.k}</span>
                <span className={`pt-row-v ${r.mono ? 'mono' : ''}`}>{String(val)}</span>
              </div>
            );
          })}
        </div>
      </div>

      {draftCount > 0 && (
        <div className="pt-draftnote">
          <V5I.warn style={{ width: 13, height: 13, color: 'var(--warn-700)', flexShrink: 0, marginTop: 1 }} />
          <span><b>미연결 {draftCount}개</b> — 평가에서 제외됨. 루트 와이어로 연결해야 정책에 포함됩니다.</span>
        </div>
      )}

      <div className="pt-actions">
        <button className="pt-btn" onClick={onReevaluate}><V5I.play style={{ width: 12, height: 12 }} /> Re-evaluate</button>
        <button className="pt-btn"><V5I.edit style={{ width: 12, height: 12 }} /> tx 직접 편집</button>
      </div>
    </div>
  );

  if (inline) return (<div className="code-pt"><div className="pt-h"><span className="pt-t">Policy test</span></div>{inner}</div>);
  return (
    <div className="sheet sheet-policy">
      <div className="pt-h"><span className="pt-t">Policy test</span><button className="pt-x" onClick={onClose} title="닫기 (Esc)"><V5I.x style={{ width: 12, height: 12 }} /></button></div>
      {inner}
    </div>
  );
}

// ─── Code mode (Cedar full + Policy test split) ───
function V6CodeMode({ cedar, focusedLeafId, matchedLeafIds, lastChangeId, fixtures, selectedFxId, onSelectFx, verdict, state, onFocusGuard, onReevaluate, draftCount, onCopy }) {
  return (
    <div className="code-split">
      <div className="code-pane">
        <div className="cedar-h">
          <span className="ch-lang">CEDAR · full</span>
          <span className="ch-sub">읽기전용 · Builder와 실시간 동기화</span>
          <div className="ch-r"><button className="ch-r-btn" onClick={onCopy}>복사</button><button className="ch-r-btn">.cedar 내보내기</button></div>
        </div>
        <div className="cedar-body">
          {cedar.lines.map(line => {
            const focused = focusedLeafId && line.guardId === focusedLeafId;
            const matched = matchedLeafIds && matchedLeafIds.includes(line.guardId);
            const flash = lastChangeId && line.guardId === lastChangeId;
            return (
              <div key={line.n} className={`cedar-line ${focused ? 'focus' : ''} ${matched ? 'match' : ''} ${flash ? 'flash' : ''}`}>
                <span className="cedar-gut">{line.n}</span>
                <span className="cedar-text">{tok5(line.text).map((tk, i) => <span key={i} className={tk.c}>{tk.t}</span>)}</span>
                {line.kind === 'guard' && line.guardId && <span className={`cedar-tag ${line.custom ? 'custom' : ''}`}>{line.guardId}</span>}
              </div>
            );
          })}
        </div>
      </div>
      <V6PolicyTest inline fixtures={fixtures} selectedFxId={selectedFxId} onSelectFx={onSelectFx}
        verdict={verdict} state={state} onFocusGuard={onFocusGuard} onReevaluate={onReevaluate} draftCount={draftCount} />
    </div>
  );
}

Object.assign(window, { V6OpPop, V6ValPop, V6ConditionBody, V6PolicyTest, V6CodeMode });
