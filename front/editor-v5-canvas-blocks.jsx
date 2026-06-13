// editor-v5-canvas-blocks.jsx — small leaf components: OpPop, ValPop, ConditionBody

const { useState: useSCB, useRef: useRCB, useEffect: useECB } = React;

// Operator popover
function V5OpPop({ leaf, onChange, onClose }) {
  const opts = V5_OPS[leaf.leafType] || ['==', '!='];
  const ref = useRCB(null);
  useECB(() => {
    const h = (e) => { if (ref.current && !ref.current.contains(e.target)) onClose(); };
    setTimeout(() => window.addEventListener('mousedown', h), 0);
    return () => window.removeEventListener('mousedown', h);
  }, [onClose]);
  return (
    <div className="pop" ref={ref}>
      {opts.map(op => (
        <div key={op} className={`pop-item ${leaf.operator === op ? 'on' : ''}`}
          onClick={() => { onChange(op); onClose(); }}>
          {op}
        </div>
      ))}
    </div>
  );
}

// Value popover
function V5ValPop({ leaf, onChange, onClose }) {
  const ref = useRCB(null);
  const [draft, setDraft] = useSCB(leaf.value);
  const commit = () => { onChange(draft); onClose(); };
  useECB(() => {
    const h = (e) => { if (ref.current && !ref.current.contains(e.target)) commit(); };
    setTimeout(() => window.addEventListener('mousedown', h), 0);
    return () => window.removeEventListener('mousedown', h);
  }, [draft]);
  const sig = v5SigById(leaf.sigId);
  const supportsRef = ['address', 'tokenNative', 'usd', 'number'].includes(leaf.leafType);

  return (
    <div className="val-pop-card" ref={ref}>
      <div className="vp-h">값 · {leaf.leafType}</div>
      {supportsRef && (
        <div className="vp-tab-row">
          <button className={draft.kind === 'ref' ? '' : 'on'}
            onClick={() => setDraft({ kind: 'num', text: draft.kind === 'num' ? draft.text : '0', unit: draft.unit })}>리터럴</button>
          <button className={draft.kind === 'ref' ? 'on' : ''}
            onClick={() => setDraft({ kind: 'ref', text: draft.kind === 'ref' ? draft.text : 'root.from' })}>ref</button>
        </div>
      )}
      {draft.kind === 'bool' && (
        <div className="vp-toggle">
          <button className={draft.text === 'true' ? 'on' : ''} onClick={() => setDraft({ ...draft, text: 'true' })}>true</button>
          <button className={draft.text === 'false' ? 'on' : ''} onClick={() => setDraft({ ...draft, text: 'false' })}>false</button>
        </div>
      )}
      {draft.kind === 'enum' && sig && sig.options && (
        <select className="vp-input" value={draft.text} onChange={(e) => setDraft({ ...draft, text: e.target.value })}>
          {sig.options.map(o => <option key={o} value={o}>{o}</option>)}
        </select>
      )}
      {draft.kind === 'num' && (
        <>
          <input className="vp-input" type="text" autoFocus value={draft.text}
            onChange={(e) => setDraft({ ...draft, text: e.target.value })}
            onKeyDown={(e) => e.key === 'Enter' && commit()} />
          {draft.unit && <div className="vp-row">단위: <span style={{ fontFamily: 'var(--ff-mono)', color: 'var(--slate-700)' }}>{draft.unit}</span></div>}
        </>
      )}
      {draft.kind === 'ref' && (
        <>
          <input className="vp-input" type="text" autoFocus value={draft.text}
            onChange={(e) => setDraft({ ...draft, text: e.target.value })} placeholder="root.from / context.foo" />
          <div className="vp-row" style={{ color: 'var(--cyan-700)' }}>동적 참조 — 평가 시점에 tx에서 읽음</div>
        </>
      )}
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 8 }}>
        <button className="btn-secondary" onClick={commit}>적용</button>
      </div>
    </div>
  );
}

// Condition body content (chips inside a condition node)
function V5ConditionBody({ node, onPatch, onDelete }) {
  const [opOpen, setOpOpen] = useSCB(false);
  const [valOpen, setValOpen] = useSCB(false);
  return (
    <>
      <span className={`seg-chip ${node.custom ? 'custom' : ''}`}>
        <span>{node.label}</span>
        <V5I.caretDown className="sc-c" style={{ width: 10, height: 10 }} />
      </span>
      <span style={{ position: 'relative', display: 'inline-flex' }}>
        <button className="op-btn" onClick={(e) => { e.stopPropagation(); setOpOpen(o => !o); }}>
          <span>{node.operator}</span>
          <V5I.caretDown style={{ width: 10, height: 10 }} />
        </button>
        {opOpen && <V5OpPop leaf={node} onChange={(op) => onPatch({ operator: op })} onClose={() => setOpOpen(false)} />}
      </span>
      <span style={{ position: 'relative', display: 'inline-flex' }}>
        <button className={`val-chip k-${node.value.kind}`} onClick={(e) => { e.stopPropagation(); setValOpen(o => !o); }}>
          <span>{node.value.text}</span>
          {node.value.unit && <span className="val-u">{node.value.unit}</span>}
        </button>
        {valOpen && <V5ValPop leaf={node} onChange={(v) => onPatch({ value: v })} onClose={() => setValOpen(false)} />}
      </span>
      {node.custom && (
        <span className="abs-pill" title="값 부재 시 처리">
          <span className="abs-k">absence</span><span className="abs-v">{node.absence}</span>
        </span>
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

Object.assign(window, { V5OpPop, V5ValPop, V5ConditionBody });
