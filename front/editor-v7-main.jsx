// editor-v7-main.jsx — Editor v7 app shell: 2-row topbar, infinite canvas (pan/zoom),
// state reducer + undo/redo, palette wiring, floating panels, publish modal, toast.

const { useReducer: useRd7, useState: useSM7, useRef: useRM7, useEffect: useEM7, useMemo: useMM7 } = React;

function cloneDoc(d) { return JSON.parse(JSON.stringify(d)); }

function reducer(state, a) {
  const { doc } = state;
  const push = (nextDoc, dirty = true) => ({ doc: nextDoc, past: [...state.past, doc].slice(-50), future: [], dirty: dirty ? true : state.dirty });
  switch (a.type) {
    case 'SELECT': return { ...state, selectedId: a.id };
    case 'PATCH': {
      const d = cloneDoc(doc); if (d.nodes[a.id]) Object.assign(d.nodes[a.id], a.patch); return { ...push(d), selectedId: a.id };
    }
    case 'TOGGLE': {
      const d = cloneDoc(doc); const n = d.nodes[a.id]; if (n) n.enabled = n.enabled === false ? true : false; return { ...push(d), selectedId: state.selectedId };
    }
    case 'DELETE': {
      const d = cloneDoc(doc); const n = d.nodes[a.id]; if (!n || a.id === d.hatId || a.id === d.rootId) return state;
      // detach from parent
      if (n.parentId && d.nodes[n.parentId]) { const p = d.nodes[n.parentId]; if (p.childIds) p.childIds = p.childIds.filter(x => x !== a.id); }
      d.drafts = (d.drafts || []).filter(x => x !== a.id);
      // remove subtree
      const rm = (id) => { const m = d.nodes[id]; if (!m) return; (m.childIds || []).forEach(rm); if (m.childId) rm(m.childId); delete d.nodes[id]; };
      rm(a.id);
      return { ...push(d), selectedId: null };
    }
    case 'ADD_CHILD': {
      const d = cloneDoc(doc); const p = d.nodes[a.parentId]; if (!p) return state;
      const id = v7Id('p');
      d.nodes[id] = { id, type: 'predicate', param: 'context.slippageBp', fieldKind: 'primitive.Long', op: 'lt', value: { kind: 'num', text: '100', unit: 'bp' }, absence: null, parentId: a.parentId };
      p.childIds = [...(p.childIds || []), id];
      return { ...push(d), selectedId: id };
    }
    case 'ADD_BLOCK': {
      const d = cloneDoc(doc); const def = a.def; const id = v7Id('p');
      const node = { id, type: 'predicate', param: def.param, fieldKind: def.fieldKind || 'primitive.Long',
        op: def.op || 'eq', value: def.value ? JSON.parse(JSON.stringify(def.value)) : { kind: 'num', text: '0' },
        absence: /^enrichment\./.test(def.param) ? 'treatAsFalse' : null, parentId: a.parentId || null };
      if (a.parentId && d.nodes[a.parentId]) { d.nodes[a.parentId].childIds = [...(d.nodes[a.parentId].childIds || []), id]; }
      else { node.float = true; node.x = a.x ?? 560; node.y = a.y ?? 360; d.drafts = [...(d.drafts || []), id]; }
      return { ...push(d), selectedId: id };
    }
    case 'ADD_LOGIC': {
      const d = cloneDoc(doc); const id = v7Id('L');
      d.nodes[id] = { id, type: 'logic', op: a.op, childIds: [], parentId: null, float: true, x: a.x ?? 560, y: a.y ?? 420 };
      d.drafts = [...(d.drafts || []), id];
      return { ...push(d), selectedId: id };
    }
    case 'SET_LOCALE': { const d = cloneDoc(doc); d.locale = a.locale; return { ...state, doc: d }; }
    case 'PANZOOM': { const d = cloneDoc(doc); d.pan = a.pan ?? d.pan; d.zoom = a.zoom ?? d.zoom; return { ...state, doc: d }; }
    case 'MARK_SAVED': return { ...state, dirty: false, saved: Date.now() };
    case 'UNDO': { if (!state.past.length) return state; const prev = state.past[state.past.length - 1]; return { ...state, doc: prev, past: state.past.slice(0, -1), future: [doc, ...state.future] }; }
    case 'REDO': { if (!state.future.length) return state; const nxt = state.future[0]; return { ...state, doc: nxt, past: [...state.past, doc], future: state.future.slice(1) }; }
    default: return state;
  }
}

function Topbar({ doc, mode, setMode, dirty, dirtyCount, canUndo, canRedo, dispatch, showPalette, showPT, showCedar, onTogglePalette, onTogglePT, onToggleCedar, onSave, onPublish, onAimSave }) {
  return (
    <div className="v7-tb">
      <div className="v7-tb-row top">
        <div className="v7-ident"><span className="v7-mark"><V7I.shield /></span><span className="v7-name">Scopeball <span className="sub">· Policy Editor</span></span></div>
        <div className="v7-modes"><button className={`v7-mode ${mode === 'builder' ? 'on' : ''}`} onClick={() => setMode('builder')}>Builder</button><button className={`v7-mode ${mode === 'code' ? 'on' : ''}`} onClick={() => setMode('code')}>Code</button></div>
        <span className="v7-spc" />
        <button className="v7-ib" disabled={!canUndo} onClick={() => dispatch({ type: 'UNDO' })} title="Undo"><V7I.undo /></button>
        <button className="v7-ib" disabled={!canRedo} onClick={() => dispatch({ type: 'REDO' })} title="Redo"><V7I.redo /></button>
        <span className="v7-vbar" />
        {dirty && <button className="v7-unsaved" onClick={onAimSave}><span className="dot" />미저장 {dirtyCount}</button>}
        <button className="v7-save" onClick={onSave}>저장<span className="sub">draft</span></button>
        <button className="v7-publish" onClick={onPublish}><V7I.shield />발행</button>
      </div>
      <div className="v7-tb-row bot">
        <nav className="v7-crumb"><span className="seg">actions</span><span className="sep">/</span><span className="seg">amm</span><span className="sep">/</span><span className="seg leaf">swap.cedarschema</span></nav>
        <span className="v7-spc" />
        <div className="v7-logic-tb">
          <span className="v7-logic-lab">논리 묶음</span>
          <button className="v7-lt" onClick={() => dispatch({ type: 'ADD_LOGIC', op: 'AND' })}>AND</button>
          <button className="v7-lt or" onClick={() => dispatch({ type: 'ADD_LOGIC', op: 'OR' })}>OR</button>
          <button className="v7-lt not" onClick={() => dispatch({ type: 'ADD_LOGIC', op: 'NOT' })}>NOT</button>
        </div>
        <span className="v7-vbar" />
        <div className="v7-ptoggles"><button className={`v7-pt ${showPalette ? 'on' : ''}`} onClick={onTogglePalette}>블록 팔레트</button><button className={`v7-pt ${showPT ? 'on' : ''}`} onClick={onTogglePT}>Policy test</button><button className={`v7-pt ${showCedar ? 'on' : ''}`} onClick={onToggleCedar}>Live Cedar</button></div>
        <div className="v7-loc"><button className={doc.locale === 'en' ? 'on' : ''} onClick={() => dispatch({ type: 'SET_LOCALE', locale: 'en' })}>EN</button><button className={doc.locale === 'ko' ? 'on' : ''} onClick={() => dispatch({ type: 'SET_LOCALE', locale: 'ko' })}>KO</button></div>
        <div className="v7-help">
          <button className="v7-help-btn">?</button>
          <div className="v7-help-pop"><div className="t">단축키</div><ul><li><kbd>⌘S</kbd><span>저장</span></li><li><kbd>⌘Z</kbd><span>되돌리기</span></li><li><kbd>휠</kbd><span>줌</span></li><li><kbd>Space+드래그</kbd><span>팬</span></li><li><kbd>Esc</kbd><span>선택 해제</span></li></ul></div>
        </div>
      </div>
    </div>
  );
}

const NAV_SVG = {
  home: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M3 11.5 12 4l9 7.5"/><path d="M5 10v10h14V10"/></svg>,
  editor: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/></svg>,
  sim: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="9"/><path d="m10 8.5 5 3.5-5 3.5z"/></svg>,
  mon: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M3 12h4l3 8 4-16 3 8h4"/></svg>,
  hist: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M3 3v18h18"/><path d="m7 14 4-4 4 3 5-7"/></svg>,
  set: <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15c.1.4.3.8.6 1L19 17l-1-1c-.3-.3-.6-.5-1-.6"/><path d="M4.6 9c.1-.4.3-.8.6-1L4 7l1-1c.3.3.6.5 1 .6"/></svg>,
};
function NavRail() {
  return (
    <nav className="nav-rail">
      <div className="nav-logo"><div className="mark"><V7I.shield /></div><div className="word">Scopeball</div></div>
      <div className="nav-cta">
        <div className="main"><svg className="plus" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><path d="M12 5v14M5 12h14"/></svg><span className="label">새 정책</span></div>
        <div className="caret"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round"><path d="m6 9 6 6 6-6"/></svg></div>
      </div>
      <div className="nav-ws"><span className="ws-av">A</span><div className="ws-label">Acme<span className="sub">메인 지갑 · 0xA1c4···7e29</span></div><svg className="ws-caret" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="m6 9 6 6 6-6"/></svg></div>
      <div className="nav-divider" />
      <div className="nav-group">
        <a className="nav-item"><span className="icon">{NAV_SVG.home}</span><span className="label">Home</span></a>
        <a className="nav-item active"><span className="icon">{NAV_SVG.editor}</span><span className="label">Editor</span></a>
        <a className="nav-item"><span className="icon">{NAV_SVG.sim}</span><span className="label">Simulation</span></a>
        <a className="nav-item"><span className="icon">{NAV_SVG.mon}</span><span className="label">Monitoring</span></a>
      </div>
      <div className="nav-divider" />
      <div className="nav-group">
        <a className="nav-item"><span className="icon">{NAV_SVG.hist}</span><span className="label">History</span><span className="badge">12</span><span className="dot-badge" /></a>
        <a className="nav-item"><span className="icon">{NAV_SVG.set}</span><span className="label">Settings</span></a>
      </div>
      <div className="nav-bottom"><div className="nav-user"><span className="av">TY</span><div className="meta"><div className="nm">Taeyoon Kim</div><div className="em">ty@scopeball.co</div></div></div></div>
    </nav>
  );
}

function FloatingPalette({ locale, onAdd, onClose }) {
  const [pos, setPos] = useSM7({ x: 16, y: 16 });
  const drag = useRM7(null);
  const onDown = (e) => {
    if (e.target.closest('button, input')) return;
    drag.current = { sx: e.clientX, sy: e.clientY, ox: pos.x, oy: pos.y };
    const mv = (ev) => { if (!drag.current) return; setPos({ x: drag.current.ox + (ev.clientX - drag.current.sx), y: drag.current.oy + (ev.clientY - drag.current.sy) }); };
    const up = () => { drag.current = null; window.removeEventListener('mousemove', mv); window.removeEventListener('mouseup', up); };
    window.addEventListener('mousemove', mv); window.addEventListener('mouseup', up);
  };
  return (
    <div className="v7-palette-float" style={{ left: pos.x, top: pos.y }}>
      <div className="v7-palette-drag" onMouseDown={onDown}>
        <span className="v7-float-t">블록 팔레트</span>
        <button className="v7-float-ib" onClick={onClose} title="닫기"><V7I.x /></button>
      </div>
      <V7Palette locale={locale} onAdd={onAdd} />
    </div>
  );
}

function App() {
  const [state, dispatch] = useRd7(reducer, null, () => ({ doc: v7BuildDoc(), past: [], future: [], dirty: false, selectedId: null, saved: null }));
  const { doc } = state;
  const [mode, setMode] = useSM7('builder');
  const [showPalette, setShowPalette] = useSM7(true);
  const [showPT, setShowPT] = useSM7(true);
  const [showCedar, setShowCedar] = useSM7(false);
  const [fxId, setFxId] = useSM7('tx-market-expiry');
  const [modal, setModal] = useSM7(false);
  const [toast, setToast] = useSM7(null);
  const saveRef = useRM7(null);
  const wrapRef = useRM7(null);

  const fx = V7_SAMPLE_TX.find(f => f.id === fxId) || V7_SAMPLE_TX[0];
  const verdict = useMM7(() => v7Evaluate(doc, fx.tx ? fx.tx : { meta: fx.meta, context: fx.context, enrichment: fx.enrichment }), [doc, fx]);
  const failedSet = useMM7(() => { const s = new Set(); Object.keys(verdict.truth).forEach(id => { if (verdict.truth[id] === false) s.add(id); }); return s; }, [verdict]);
  const dirtyCount = state.past.length;

  const showToast = (msg, mono) => { setToast({ msg, mono }); setTimeout(() => setToast(null), 2600); };
  const onSave = () => { dispatch({ type: 'MARK_SAVED' }); showToast('에디터 문서 저장됨', 'draft'); };
  const onAimSave = () => { if (saveRef.current) { saveRef.current.style.transition = 'box-shadow 120ms'; saveRef.current.style.boxShadow = '0 0 0 4px var(--warn-300)'; setTimeout(() => { if (saveRef.current) saveRef.current.style.boxShadow = ''; }, 900); } };

  // keyboard
  useEM7(() => {
    const h = (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's') { e.preventDefault(); onSave(); }
      else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'z' && !e.shiftKey) { e.preventDefault(); dispatch({ type: 'UNDO' }); }
      else if ((e.metaKey || e.ctrlKey) && (e.key.toLowerCase() === 'y' || (e.shiftKey && e.key.toLowerCase() === 'z'))) { e.preventDefault(); dispatch({ type: 'REDO' }); }
      else if (e.key === 'Escape') dispatch({ type: 'SELECT', id: null });
    };
    window.addEventListener('keydown', h); return () => window.removeEventListener('keydown', h);
  }, [state]);

  // pan + zoom
  const pan = useRM7(null);
  const onCanvasDown = (e) => {
    if (e.target.closest('.v7-pred, .v7-logic, .v7-hat, .v7-float, .v7-czoom')) return;
    if (e.button !== 0 && e.button !== 1 && !e.spaceKey) { /* allow */ }
    pan.current = { sx: e.clientX, sy: e.clientY, ox: doc.pan.x, oy: doc.pan.y };
    const move = (ev) => { if (!pan.current) return; dispatch({ type: 'PANZOOM', pan: { x: pan.current.ox + (ev.clientX - pan.current.sx), y: pan.current.oy + (ev.clientY - pan.current.sy) } }); };
    const up = () => { pan.current = null; window.removeEventListener('mousemove', move); window.removeEventListener('mouseup', up); };
    window.addEventListener('mousemove', move); window.addEventListener('mouseup', up);
  };
  const onWheel = (e) => {
    if (!e.ctrlKey && !e.metaKey) { dispatch({ type: 'PANZOOM', pan: { x: doc.pan.x - e.deltaX, y: doc.pan.y - e.deltaY } }); return; }
    e.preventDefault();
    const z = Math.min(2, Math.max(0.4, doc.zoom * (e.deltaY < 0 ? 1.1 : 0.9)));
    dispatch({ type: 'PANZOOM', zoom: z });
  };
  const fit = () => dispatch({ type: 'PANZOOM', pan: { x: 0, y: 0 }, zoom: 1 });

  // palette add
  const onAdd = (def) => {
    const sel = state.selectedId && doc.nodes[state.selectedId];
    if (sel && sel.type === 'logic') { dispatch({ type: 'ADD_BLOCK', def, parentId: sel.id }); showToast('컨테이너에 블록 추가'); }
    else { dispatch({ type: 'ADD_BLOCK', def }); showToast('초안 영역에 블록 추가 — 컨테이너로 끼우세요'); }
  };
  const onDrop = (e) => {
    const id = e.dataTransfer.getData('text/v7block'); if (!id) return;
    const def = (window.V6_BLOCKS || {})[id]; if (!def) return;
    const rect = wrapRef.current.getBoundingClientRect();
    const x = (e.clientX - rect.left - doc.pan.x) / doc.zoom;
    const y = (e.clientY - rect.top - doc.pan.y) / doc.zoom;
    dispatch({ type: 'ADD_BLOCK', def, x, y });
  };

  const drafts = (doc.drafts || []).map(id => doc.nodes[id]).filter(Boolean);

  return (
    <div className="v7-shell">
      <NavRail />
      <div className="v7-root" onClick={() => {}}>
      <div className="v7-top">
        <Topbar doc={doc} mode={mode} setMode={setMode} dirty={state.dirty} dirtyCount={dirtyCount}
          canUndo={state.past.length > 0} canRedo={state.future.length > 0} dispatch={dispatch}
          showPalette={showPalette} showPT={showPT} showCedar={showCedar}
          onTogglePalette={() => setShowPalette(v => !v)} onTogglePT={() => setShowPT(v => !v)} onToggleCedar={() => setShowCedar(v => !v)}
          onSave={onSave} onPublish={() => setModal(true)} onAimSave={onAimSave} />
      </div>
      <div className="v7-body">
        {mode === 'builder' ? (
          <div className="v7-canvas-wrap" ref={wrapRef} onMouseDown={onCanvasDown} onWheel={onWheel}
            onDragOver={(e) => e.preventDefault()} onDrop={onDrop}>
            <div className="v7-canvas" style={{ transform: `translate(${doc.pan.x}px, ${doc.pan.y}px) scale(${doc.zoom})` }}>
              <div className="v7-stage">
                <div className="v7-lane-tag" style={{ left: 80, top: 78 }}><V7I.shield style={{ width: 13, height: 13 }} /> 허용 레인 · permit</div>
                <V7Hat doc={doc} dispatch={dispatch} locale={doc.locale} selectedId={state.selectedId} truth={verdict.truth} failedSet={failedSet} />
                {drafts.length > 0 && <div className="v7-draft-tag" style={{ left: 120, top: 680 }}>초안 · 컴파일 제외</div>}
                {drafts.map(n => <V7Node key={n.id} doc={doc} node={n} dispatch={dispatch} locale={doc.locale} selectedId={state.selectedId} truth={verdict.truth} failedSet={failedSet} />)}
              </div>
            </div>

            <div className="v7-czoom">
              <button onClick={() => dispatch({ type: 'PANZOOM', zoom: Math.max(0.4, doc.zoom - 0.1) })}>−</button>
              <span className="pct">{Math.round(doc.zoom * 100)}%</span>
              <button onClick={() => dispatch({ type: 'PANZOOM', zoom: Math.min(2, doc.zoom + 0.1) })}>+</button>
              <button className="fit" onClick={fit}>fit</button>
            </div>

            {showPalette && <FloatingPalette locale={doc.locale} onAdd={onAdd} onClose={() => setShowPalette(false)} />}
            {showPT && <V7PolicyTest doc={doc} fxId={fxId} onFx={setFxId} verdict={verdict} onClose={() => setShowPT(false)} x={700} y={16} />}
            {showCedar && <V7LiveCedar doc={doc} onClose={() => setShowCedar(false)} x={120} y={300} />}
            {toast && <div className="v7-toast"><V7I.check /><span>{toast.msg}{toast.mono && <> · <span className="mono">{toast.mono}</span></>}</span></div>}
          </div>
        ) : (
          <div className="v7-canvas-wrap" style={{ display: 'flex', padding: 12, gap: 12, background: 'var(--fog-200)' }}>
            <div className="v7-code"><div className="v7-code-h"><span className="v7-code-lang">CEDAR · permit</span><span className="v7-code-sub">Builder와 실시간 동기화 · 읽기전용</span></div><V7Cedar doc={doc} /></div>
            <div className="v7-float" style={{ position: 'static', width: 340, maxHeight: 'none' }}>
              <div className="v7-float-h" style={{ cursor: 'default' }}><span className="v7-float-t">Policy test</span></div>
              <div className="v7-float-body"><V7PTContent doc={doc} fxId={fxId} onFx={setFxId} verdict={verdict} /></div>
            </div>
          </div>
        )}
      </div>

      {modal && <PublishModal doc={doc} verdict={verdict} onClose={() => setModal(false)} onConfirm={() => { setModal(false); dispatch({ type: 'MARK_SAVED' }); showToast('정책 발행됨 — 엔진 반영 + Audit 기록', 'permit'); }} />}
      </div>
    </div>
  );
}

function PublishModal({ doc, verdict, onClose, onConfirm }) {
  const root = doc.nodes[doc.rootId];
  const active = (root.childIds || []).filter(id => { const n = doc.nodes[id]; return n && n.enabled !== false; }).length;
  const off = (root.childIds || []).filter(id => { const n = doc.nodes[id]; return n && n.enabled === false; }).length;
  const drafts = (doc.drafts || []).length;
  return (
    <div className="v7-modal-bd" onClick={onClose}>
      <div className="v7-modal" onClick={(e) => e.stopPropagation()}>
        <div className="v7-modal-h"><div className="t">정책 발행</div><div className="s">활성 + 연결된 안전 조건만 Cedar <span style={{ fontFamily: 'var(--ff-mono)' }}>permit</span> 문장으로 컴파일됩니다.</div></div>
        <div className="v7-modal-body">
          <div className="v7-modal-stat"><span className="k">정책 이름</span><span className="v">{doc.policyName}</span></div>
          <div className="v7-modal-stat"><span className="k">액션</span><span className="v">{doc.action}</span></div>
          <div className="v7-modal-stat"><span className="k">활성 안전 조건</span><span className="v">{active}개</span></div>
          <div className="v7-modal-stat"><span className="k">비활성 · 미연결 (제외)</span><span className="v">{off + drafts}개</span></div>
        </div>
        <div className="v7-modal-foot"><button className="v7-btn-ghost" onClick={onClose}>취소</button><button className="v7-btn-go" onClick={onConfirm}>발행 확정</button></div>
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById('root')).render(<App />);
