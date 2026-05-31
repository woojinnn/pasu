// editor-v7-blocks.jsx — Scratch-style block language for Editor v7.
// Three block types: predicate piece, logic container (AND/OR/NOT), hat (permit header).
// Nesting is by containment (childIds), not wires. Role visuals from V7_ROLES.

const { useState: useS7, useRef: useR7, useEffect: useE7 } = React;

// ─── icons ───
const V7I = {
  hash:   (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" {...p}><path d="M4 9h16M4 15h16M10 3L8 21M16 3l-2 18"/></svg>,
  key:    (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><circle cx="8" cy="8" r="5"/><path d="M11.5 11.5L21 21M17 17l2-2M14 14l2-2"/></svg>,
  token:  (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" {...p}><circle cx="12" cy="12" r="8"/><path d="M9.5 12l1.8 1.8 3.5-3.6" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  switch: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" {...p}><rect x="2" y="7" width="20" height="10" rx="5"/><circle cx="8" cy="12" r="3" fill="currentColor" stroke="none"/></svg>,
  clock:  (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" {...p}><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></svg>,
  dot:    (p) => <svg viewBox="0 0 24 24" fill="currentColor" {...p}><circle cx="12" cy="12" r="4"/></svg>,
  shield: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinejoin="round" {...p}><path d="M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z"/><path d="M9 12l2 2 4-4" strokeLinecap="round"/></svg>,
  check:  (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M5 13l4 4L19 7"/></svg>,
  x:      (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" {...p}><path d="M6 6l12 12M18 6L6 18"/></svg>,
  plus:   (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" {...p}><path d="M12 5v14M5 12h14"/></svg>,
  caret:  (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M9 6l6 6-6 6"/></svg>,
  caretD: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M6 9l6 6 6-6"/></svg>,
  undo:   (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M9 7L4 12l5 5"/><path d="M4 12h11a5 5 0 0 1 0 10h-1"/></svg>,
  redo:   (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M15 7l5 5-5 5"/><path d="M20 12H9a5 5 0 0 0 0 10h1"/></svg>,
  search: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" {...p}><circle cx="11" cy="11" r="7"/><path d="M16 16l5 5"/></svg>,
  pin:    (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M12 17v5M8 4h8l-1 7 3 3H6l3-3z"/></svg>,
  play:   (p) => <svg viewBox="0 0 24 24" fill="currentColor" {...p}><path d="M7 5l12 7-12 7z"/></svg>,
};
const ROLE_ICON = { numeric: V7I.hash, address: V7I.key, ref: V7I.token, enum: V7I.switch, auth: V7I.clock, misc: V7I.dot };

function roleVars(param, fk) {
  const role = v7RoleOf(param, fk);
  return {
    '--r-fill': `var(--role-${role}-fill)`,
    '--r-bd': `var(--role-${role}-bd)`,
    '--r-ink': `var(--role-${role}-ink)`,
  };
}

// ─── operator popover ───
function V7OpPop({ node, onPick, onClose }) {
  const ref = useR7(null);
  useE7(() => {
    const h = (e) => { if (ref.current && !ref.current.contains(e.target)) onClose(); };
    setTimeout(() => window.addEventListener('mousedown', h), 0);
    return () => window.removeEventListener('mousedown', h);
  }, [onClose]);
  const ops = V7_OPS[node.fieldKind] || ['eq', 'neq'];
  return (
    <div className="v7-pop" ref={ref}>
      {ops.map(op => (
        <div key={op} className={`v7-pop-item ${node.op === op ? 'on' : ''}`} onClick={() => { onPick(op); onClose(); }}>
          <span className="sym">{V7_OPSYM[op] || op}</span><span className="nm">{op}</span>
        </div>
      ))}
    </div>
  );
}

// ─── value popover ───
function V7ValPop({ node, onSet, onClose }) {
  const ref = useR7(null);
  const [draft, setDraft] = useS7(node.value || { kind: 'num', text: '' });
  const commit = () => { onSet(draft); onClose(); };
  useE7(() => {
    const h = (e) => { if (ref.current && !ref.current.contains(e.target)) commit(); };
    setTimeout(() => window.addEventListener('mousedown', h), 0);
    return () => window.removeEventListener('mousedown', h);
  }, [draft]);
  const fk = node.fieldKind;
  const isNum = fk === 'primitive.Long' || fk === 'primitive.decimal';
  const isStr = fk === 'primitive.String' || fk === 'ref';
  return (
    <div className="v7-vpop" ref={ref}>
      <div className="h">값 · {fk.replace('primitive.', '')}</div>
      {isStr && (
        <div className="tabrow">
          <button className={draft.kind !== 'ref' ? 'on' : ''} onClick={() => setDraft({ kind: 'str', text: draft.kind === 'str' ? draft.text : '' })}>리터럴</button>
          <button className={draft.kind === 'ref' ? 'on' : ''} onClick={() => setDraft({ kind: 'ref', text: draft.kind === 'ref' ? draft.text : '@meta.from' })}>@ref</button>
        </div>
      )}
      <input autoFocus value={draft.text}
        onChange={(e) => setDraft({ ...draft, kind: isNum ? 'num' : (draft.kind === 'ref' ? 'ref' : 'str'), text: e.target.value })}
        onKeyDown={(e) => e.key === 'Enter' && commit()}
        placeholder={isNum ? '0' : draft.kind === 'ref' ? '@meta.from' : '0x… / USDC'} />
      {draft.kind === 'ref' && <div className="hint">동적 참조 — 평가 시점에 TX에서 읽음</div>}
      {isNum && node.value && node.value.unit && <div className="hint" style={{ color: 'var(--slate-400)' }}>단위: {node.value.unit}</div>}
      <button className="apply" onClick={commit}>적용</button>
    </div>
  );
}

// ─── predicate piece ───
function V7Predicate({ doc, node, dispatch, locale, selectedId, truth, failedSet, draggableHandlers }) {
  const [opOpen, setOpOpen] = useS7(false);
  const [valOpen, setValOpen] = useS7(false);
  const live = v7IsLive(node.param);
  const role = v7RoleOf(node.param, node.fieldKind);
  const Icon = ROLE_ICON[role] || V7I.dot;
  const v = node.value || {};
  const noVal = node.op === 'isTrue' || node.op === 'isFalse' || node.op === 'isEmpty';
  const valText = v.kind === 'ref' ? v.text : (v.text != null ? v.text : '');
  const sel = selectedId === node.id;
  const failed = failedSet && failedSet.has(node.id);
  const cls = ['v7-pred', `role-${role}`, node.enabled === false ? 'off' : '', sel ? 'sel' : '', live ? 'is-live' : '', failed ? 'failed' : '', node.float ? 'float' : ''].filter(Boolean).join(' ');
  const style = { ...roleVars(node.param, node.fieldKind) };
  if (node.float) { style.left = node.x; style.top = node.y; }

  return (
    <div className={cls} style={style} onClick={(e) => { e.stopPropagation(); dispatch({ type: 'SELECT', id: node.id }); }} {...(draggableHandlers || {})}>
      {live && <span className="v7-live-badge">live</span>}
      <button className="v7-nx" onClick={(e) => { e.stopPropagation(); dispatch({ type: 'DELETE', id: node.id }); }}><V7I.x /></button>
      <div className="v7-pred-cap"><Icon /></div>
      <div className="v7-pred-field">
        <span className="nm">{v7Display(node.param, locale)}</span>
        <span className="canon">{node.param}</span>
      </div>
      <span style={{ position: 'relative', display: 'inline-flex' }}>
        <button className="v7-pred-op" onClick={(e) => { e.stopPropagation(); setOpOpen(o => !o); }}>{V7_OPSYM[node.op] || node.op}</button>
        {opOpen && <V7OpPop node={node} onPick={(op) => dispatch({ type: 'PATCH', id: node.id, patch: { op } })} onClose={() => setOpOpen(false)} />}
      </span>
      {!noVal && (
        <span style={{ position: 'relative', display: 'inline-flex' }}>
          <button className={`v7-pred-val ${v.kind === 'ref' ? 'ref' : ''}`} onClick={(e) => { e.stopPropagation(); setValOpen(o => !o); }}>
            {valText}{v.unit && <span className="u">{v.unit}</span>}
          </button>
          {valOpen && <V7ValPop node={node} onSet={(val) => dispatch({ type: 'PATCH', id: node.id, patch: { value: val } })} onClose={() => setValOpen(false)} />}
        </span>
      )}
      {noVal && node.fieldKind !== 'primitive.Bool' && <span className="v7-pred-val bool">∅</span>}
    </div>
  );
}

// ─── logic container (AND / OR / NOT) ───
function V7Logic({ doc, node, dispatch, locale, selectedId, truth, failedSet }) {
  const sel = selectedId === node.id;
  const failed = failedSet && failedSet.has(node.id);
  const kids = (node.childIds || []).map(id => doc.nodes[id]).filter(Boolean);
  const isGuard = !!node.guardId;
  const cls = ['v7-logic', `op-${node.op}`, node.enabled === false ? 'off' : '', sel ? 'sel' : '', failed ? 'failed' : '', node.float ? 'float' : ''].filter(Boolean).join(' ');
  const style = {};
  if (node.float) { style.left = node.x; style.top = node.y; style.position = 'absolute'; }

  return (
    <div className={cls} style={style} onClick={(e) => { e.stopPropagation(); dispatch({ type: 'SELECT', id: node.id }); }}>
      <span className="v7-logic-spine" />
      <button className="v7-nx" onClick={(e) => { e.stopPropagation(); dispatch({ type: 'DELETE', id: node.id }); }}><V7I.x /></button>
      <div className="v7-logic-head">
        <span className="v7-logic-kw">{node.op === 'NOT' ? <><span className="kw-x">NOT</span></> : node.op}</span>
        {isGuard && (
          <span className="v7-logic-guard">
            <span className="v7-guard-id">{node.guardId}</span>
            <span className="v7-guard-lab">{node.label}</span>
          </span>
        )}
        {node.op === 'NOT' && <span style={{ fontSize: 11, color: 'var(--slate-400)' }}>아래 패턴이면 제외</span>}
        <span style={{ flex: 1 }} />
        {isGuard && (
          <button className={`v7-gtoggle ${node.enabled !== false ? 'on' : ''}`} onClick={(e) => { e.stopPropagation(); dispatch({ type: 'TOGGLE', id: node.id }); }}>
            <span className="sw" />{node.enabled !== false ? 'on' : 'off'}
          </button>
        )}
      </div>
      <div className="v7-logic-slot">
        <div className="v7-slot-list">
          {kids.length === 0 && <div className="v7-dropzone">여기에 블록을 끼우세요</div>}
          {kids.map(k => <V7Node key={k.id} doc={doc} node={k} dispatch={dispatch} locale={locale} selectedId={selectedId} truth={truth} failedSet={failedSet} />)}
        </div>
        {node.op !== 'NOT' && (
          <button className="v7-add" onClick={(e) => { e.stopPropagation(); dispatch({ type: 'ADD_CHILD', parentId: node.id }); }}>
            <V7I.plus /> 조건 추가
          </button>
        )}
      </div>
    </div>
  );
}

// ─── node dispatcher ───
function V7Node(props) {
  const { node } = props;
  if (!node) return null;
  if (node.type === 'predicate') return <V7Predicate {...props} />;
  if (node.type === 'logic') return <V7Logic {...props} />;
  return null;
}

// ─── hat (permit header) ───
function V7Hat({ doc, dispatch, locale, selectedId, truth, failedSet }) {
  const hat = doc.nodes[doc.hatId];
  const root = doc.nodes[hat.childId];
  const actLabel = (hat.action || 'Amm::Swap').split('::').pop();
  return (
    <div className="v7-hat" style={{ left: hat.x, top: hat.y }}>
      <div className="v7-hat-head">
        <span className="v7-hat-eff"><V7I.shield /> 허용 · permit</span>
        <span className="v7-hat-sentence">
          <b>지갑</b>이 <b>프로토콜</b>에 대해 <span className="v7-hat-act">{actLabel}</span> 할 때, 다음 안전 조건이 <b>모두 참</b>이면 허용:
        </span>
      </div>
      <div className="v7-hat-body">
        {root && root.childIds && root.childIds.length > 0 ? (
          <div className="v7-slot-list">
            {root.childIds.map(id => doc.nodes[id]).filter(Boolean).map(k => (
              <V7Node key={k.id} doc={doc} node={k} dispatch={dispatch} locale={locale} selectedId={selectedId} truth={truth} failedSet={failedSet} />
            ))}
          </div>
        ) : (
          <div className="v7-dropzone" style={{ marginLeft: 0 }}>안전 조건 블록을 여기에 추가</div>
        )}
        <button className="v7-add" style={{ marginLeft: 0, marginTop: 8 }} onClick={(e) => { e.stopPropagation(); dispatch({ type: 'ADD_CHILD', parentId: root.id }); }}>
          <V7I.plus /> 안전 조건 추가
        </button>
      </div>
    </div>
  );
}

Object.assign(window, { V7I, ROLE_ICON, roleVars, V7OpPop, V7ValPop, V7Predicate, V7Logic, V7Node, V7Hat });
