// editor-v6-main.jsx — App composition for v6 (docked resizable palette,
// focus mode, footer panel toggles, minimal header).

const { useState: useSM6, useEffect: useEM6, useMemo: useMM6, useReducer: useRM6, useRef: useRfM6, useCallback: useCbM6 } = React;

const V6_TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "showPolicy": true,
  "showCedar": false,
  "navExpanded": false,
  "locale": "ko",
  "gridStrength": "medium"
}/*EDITMODE-END*/;

function makeHistory6() {
  const init = v6BuildBaseline();
  return { past: [], present: init, future: [], saved: JSON.stringify(init), lastChangeId: null };
}
function histReduce6(state, action) {
  if (action.type === 'UNDO') {
    if (!state.past.length) return state;
    return { past: state.past.slice(0, -1), present: state.past[state.past.length - 1], future: [state.present, ...state.future], saved: state.saved, lastChangeId: null };
  }
  if (action.type === 'REDO') {
    if (!state.future.length) return state;
    return { past: [...state.past, state.present], present: state.future[0], future: state.future.slice(1), saved: state.saved, lastChangeId: null };
  }
  if (action.type === 'MARK_SAVED') return { ...state, saved: JSON.stringify(state.present) };
  if (action.type === 'RESET') {
    const fresh = v6BuildBaseline();
    return { past: [], present: fresh, future: [], saved: JSON.stringify(fresh), lastChangeId: null };
  }
  const next = v6Reduce(state.present, action);
  if (next === state.present) return state;
  const isMove = action.type === 'MOVE';
  const noHist = isMove || action.type === 'TOGGLE_CUSTOM_FOLDER' || action.type === 'SET_BREADCRUMB';
  return {
    past: noHist ? state.past : [...state.past, state.present].slice(-80),
    present: next, future: noHist ? state.future : [],
    saved: state.saved, lastChangeId: next.lastChangeId || state.lastChangeId,
  };
}

function V6App() {
  const [t, setTweak] = useTweaks(V6_TWEAK_DEFAULTS);
  const [hist, dispatch] = useRM6(histReduce6, null, makeHistory6);
  const state = hist.present;

  const [mode, setMode] = useSM6('editor');
  const [focusedLeafId, setFocusedLeafId] = useSM6(null);
  const [selectedId, setSelectedId] = useSM6(null);
  const [selectedFxId, setSelectedFxId] = useSM6('tx-market-expiry');
  const [orModalFor, setORModalFor] = useSM6(null);
  const [orSeen, setOrSeen] = useSM6(false);
  const [evalTick, setEvalTick] = useSM6(0);
  const [zoom, setZoom] = useSM6(1);
  const [cedarH, setCedarH] = useSM6(220);
  const [schemaModal, setSchemaModal] = useSM6(null);
  const [folderModal, setFolderModal] = useSM6(null);
  const [focusPath, setFocusPath] = useSM6(null);
  const [paletteW, setPaletteW] = useSM6(312);
  const [toast, setToast] = useSM6(null);
  const saveBtnRef = useRfM6(null);
  const resizeRef = useRfM6(null);

  const cedar = useMM6(() => v6ToCedar(state), [state]);
  const draftIds = useMM6(() => v5DraftIds(state), [state]);
  const fixture = V6_FIXTURES.find(f => f.id === selectedFxId) || V6_FIXTURES[0];
  const verdict = useMM6(() => v6Evaluate(state, fixture.tx), [state, fixture, evalTick]);
  const matchedLeafIds = verdict.matchedLeafIds;
  const dirty = JSON.stringify(state) !== hist.saved;
  const dirtyCount = useMM6(() => {
    if (!dirty) return 0;
    try { const saved = JSON.parse(hist.saved);
      return Math.max(1, Math.abs(Object.keys(saved.nodes || {}).length - Object.keys(state.nodes || {}).length) + 1);
    } catch { return 1; }
  }, [dirty, state, hist.saved]);

  const selectedNode = selectedId ? state.nodes[selectedId] : null;
  const showToast = (msg, mono) => { setToast({ msg, mono }); setTimeout(() => setToast(null), 2200); };

  // palette resize
  const startResize = (e) => {
    resizeRef.current = { startX: e.clientX, startW: paletteW };
    document.body.style.cursor = 'ew-resize';
    e.preventDefault();
  };
  useEM6(() => {
    const move = (e) => { if (!resizeRef.current) return; const d = resizeRef.current; setPaletteW(Math.max(280, Math.min(600, d.startW + (e.clientX - d.startX)))); };
    const up = () => { if (resizeRef.current) { resizeRef.current = null; document.body.style.cursor = ''; } };
    window.addEventListener('pointermove', move); window.addEventListener('pointerup', up);
    return () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up); };
  }, [paletteW]);

  // keyboard
  useEM6(() => {
    const h = (e) => {
      if (e.target && (e.target.tagName === 'INPUT' || e.target.tagName === 'SELECT' || e.target.isContentEditable)) { if (e.key === 'Escape') e.target.blur(); return; }
      if ((e.metaKey || e.ctrlKey) && e.key === 'z' && !e.shiftKey) { e.preventDefault(); dispatch({ type: 'UNDO' }); }
      else if ((e.metaKey || e.ctrlKey) && ((e.shiftKey && e.key === 'z') || e.key === 'y')) { e.preventDefault(); dispatch({ type: 'REDO' }); }
      else if ((e.metaKey || e.ctrlKey) && e.key === 's') { e.preventDefault(); dispatch({ type: 'MARK_SAVED' }); }
      else if ((e.metaKey || e.ctrlKey) && e.key === 'b') { e.preventDefault(); setTweak('navExpanded', !t.navExpanded); }
      else if (e.key === 'Escape') {
        if (schemaModal) setSchemaModal(null);
        else if (folderModal) setFolderModal(null);
        else if (orModalFor) setORModalFor(null);
        else if (selectedId) setSelectedId(null);
        else if (focusPath) setFocusPath(null);
      }
    };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, [t.navExpanded, orModalFor, selectedId, schemaModal, folderModal, focusPath]);

  useEM6(() => {
    if (!t.navExpanded) return;
    const h = (e) => { if (!e.target.closest('.nav-rail')) setTweak('navExpanded', false); };
    const id = setTimeout(() => window.addEventListener('mousedown', h), 0);
    return () => { clearTimeout(id); window.removeEventListener('mousedown', h); };
  }, [t.navExpanded]);

  const onAimSave = () => { if (!saveBtnRef.current) return; saveBtnRef.current.classList.add('pulse'); setTimeout(() => saveBtnRef.current && saveBtnRef.current.classList.remove('pulse'), 2800); };

  const requestORFromCanvas = (opts) => { if (orSeen) { dispatch({ type: 'ADD_GROUP', combinator: 'OR', ...opts }); return; } setORModalFor(opts); };
  const confirmOR = () => { if (orModalFor) dispatch({ type: 'ADD_GROUP', combinator: 'OR', ...orModalFor }); setORModalFor(null); };

  const addBlockFromPalette = (def) => { if (!def) return; dispatch({ type: 'ADD_SCHEMA_BLOCK', def, parentId: null, x: 360 + Math.random() * 120, y: 360 + Math.random() * 70 }); };

  // logic groups added from the footer toolbar — drop near a visible canvas spot
  const addLogic = (kind) => {
    const x = 360 + Math.random() * 120, y = 320 + Math.random() * 90;
    if (kind === 'OR') requestORFromCanvas({ x, y, parentId: null });
    else dispatch({ type: 'ADD_GROUP', combinator: kind, parentId: null, x, y });
  };

  // header breadcrumb → drive palette focus
  const onCrumbClick = (seg, i) => {
    const crumb = state.breadcrumb || [];
    const isFile = /\.cedarschema$/.test(seg);
    if (i === 0) { setFocusPath(null); return; }          // actions root → exit focus
    if (isFile) setFocusPath(crumb.slice(0, i).join('/')); // focus parent folder
    else setFocusPath(crumb.slice(0, i + 1).join('/'));
  };

  const onCopyCedar = () => { try { navigator.clipboard.writeText(cedar.lines.map(l => l.text).join('\n')); } catch {} };
  const setZoomClamped = (val) => setZoom(Math.max(0.25, Math.min(4, val)));
  const zoomSteps = [0.25, 0.5, 0.75, 1, 1.5, 2, 3, 4];
  const zoomOut = () => { const i = zoomSteps.findIndex(s => s >= zoom); setZoom(zoomSteps[Math.max(0, i <= 0 ? 0 : i - 1)]); };
  const zoomIn = () => { const i = zoomSteps.findIndex(s => s > zoom); setZoom(i === -1 ? zoomSteps[zoomSteps.length - 1] : zoomSteps[i]); };

  const rightDockOpen = mode === 'editor' && (selectedNode || t.showPolicy);

  return (
    <>
      <V5NavRail locked={false} forceExpanded={t.navExpanded} />

      <main className="content">
        <div className="ed-root">
          <V6Topbar
            mode={mode} onModeChange={setMode} dirty={dirty} dirtyCount={dirtyCount}
            undoEnabled={hist.past.length > 0} redoEnabled={hist.future.length > 0}
            onUndo={() => dispatch({ type: 'UNDO' })} onRedo={() => dispatch({ type: 'REDO' })}
            onSave={() => dispatch({ type: 'MARK_SAVED' })} saveBtnRef={saveBtnRef} onAimSave={onAimSave}
            breadcrumb={state.breadcrumb} onCrumbClick={onCrumbClick}
            onAddLogic={addLogic}
            showPolicy={t.showPolicy} showCedar={t.showCedar}
            onTogglePolicy={() => setTweak('showPolicy', !t.showPolicy)}
            onToggleCedar={() => setTweak('showCedar', !t.showCedar)}
          />

          {mode === 'editor' ? (
            <div className="ws-layer ws-row">
              {/* left palette dock (resizable) */}
              <div className="pal-dock" style={{ width: paletteW }}>
                <V6Palette
                  onAddBlock={addBlockFromPalette}
                  customTree={state.customTree} dispatch={dispatch}
                  onOpenFileModal={(parentId) => setSchemaModal({ parentId })}
                  onOpenFolderModal={(parentId) => setFolderModal({ parentId })}
                  onSync={() => showToast('manifest 소스 재동기화 완료', '#fc20a91')}
                  focusPath={focusPath} setFocusPath={setFocusPath}
                  revealPath={selectedNode ? selectedNode.sigId : null}
                />
                <div className="pal-resize" onPointerDown={startResize} title="드래그하여 폭 조절 (280–600)"><span className="pal-resize-grip" /></div>
              </div>

              {/* canvas column */}
              <div className="ws-canvas-col">
                <div className="ws-canvas-base">
                  <V6Canvas
                    state={state} dispatch={dispatch}
                    focusedLeafId={focusedLeafId} setFocusedLeafId={setFocusedLeafId}
                    matchedLeafIds={matchedLeafIds}
                    selectedId={selectedId} setSelectedId={setSelectedId}
                    onOpenInspector={(id) => setSelectedId(id)}
                    zoom={zoom} setZoom={setZoomClamped}
                    gridStrength={t.gridStrength}
                    onRequestOR={requestORFromCanvas}
                  />
                </div>

                <div className="top-strip">
                  <div className={`ts-chip draft ${draftIds.length ? 'has' : ''}`}>
                    <span className="ts-dot" /><span className="ts-k">미연결</span><span className="ts-n">{draftIds.length}개</span>
                  </div>
                  <div className="ts-chip">
                    <span className="ts-k">IN 정책</span>
                    <span className="ts-n">{Object.values(state.nodes).filter(n => n.kind === 'condition' && v5Included(state, n.id)).length}</span>
                  </div>
                  <span className="ts-spacer" />
                  <div className="ts-zoom">
                    <button onClick={zoomOut} title="Zoom out">−</button>
                    <span className="ts-zoom-pct">{Math.round(zoom * 100)}%</span>
                    <button onClick={zoomIn} title="Zoom in">+</button>
                    <button className="ts-zoom-fit" onClick={() => setZoom(1)} title="Fit">fit</button>
                  </div>
                </div>

                {t.showCedar && (
                  <V5CedarSheet
                    cedar={cedar} onClose={() => setTweak('showCedar', false)}
                    focusedLeafId={focusedLeafId} matchedLeafIds={matchedLeafIds}
                    lastChangeId={hist.lastChangeId} onCopy={onCopyCedar}
                    height={cedarH} onHeightChange={setCedarH}
                  />
                )}
              </div>

              {/* right dock — inspector takes priority over policy test */}
              {rightDockOpen && (
                <div className="ws-right-dock">
                  {selectedNode ? (
                    <V6BlockInspector node={selectedNode} dispatch={dispatch} onClose={() => setSelectedId(null)} />
                  ) : (
                    <V6PolicyTest
                      onClose={() => setTweak('showPolicy', false)}
                      fixtures={V6_FIXTURES} selectedFxId={selectedFxId} onSelectFx={setSelectedFxId}
                      verdict={verdict} state={state} onFocusGuard={setFocusedLeafId}
                      onReevaluate={() => setEvalTick(x => x + 1)} draftCount={draftIds.length}
                    />
                  )}
                </div>
              )}
            </div>
          ) : (
            <div className="ws-layer" style={{ background: 'transparent', border: 0, padding: 0 }}>
              <V6CodeMode
                cedar={cedar} focusedLeafId={focusedLeafId} matchedLeafIds={matchedLeafIds}
                lastChangeId={hist.lastChangeId} fixtures={V6_FIXTURES}
                selectedFxId={selectedFxId} onSelectFx={setSelectedFxId}
                verdict={verdict} state={state} onFocusGuard={setFocusedLeafId}
                onReevaluate={() => setEvalTick(x => x + 1)} draftCount={draftIds.length} onCopy={onCopyCedar}
              />
            </div>
          )}

          {/* footer removed in v6 — logic groups, panel toggles, shortcuts and
              the unsaved indicator now live in the 2-row topbar. */}

          {toast && <div className="v6-toast"><V5I.check /><span>{toast.msg}{toast.mono && <> · <span className="mono">{toast.mono}</span></>}</span></div>}
        </div>
      </main>

      {orModalFor && <V5ORConfirmModal onConfirm={confirmOR} onCancel={() => setORModalFor(null)} dontShowAgain={orSeen} onSetDontShow={setOrSeen} />}

      {schemaModal && (
        <V6SchemaModal customTree={state.customTree} defaultParent={schemaModal.parentId}
          onClose={() => setSchemaModal(null)}
          onSubmit={({ file, parentId }) => { dispatch({ type: 'ADD_CUSTOM_FILE', file, parentId }); setSchemaModal(null); showToast('커스텀 블록 추가됨', file.name); }} />
      )}
      {folderModal && (
        <V6FolderModal customTree={state.customTree} defaultParent={folderModal.parentId}
          onClose={() => setFolderModal(null)}
          onSubmit={({ name, parentId }) => { dispatch({ type: 'ADD_CUSTOM_FOLDER', name, parentId }); setFolderModal(null); showToast('폴더 추가됨', name + '/'); }} />
      )}

      <TweaksPanel>
        <TweakSection label="우측 패널 (푸터에서도 토글)" />
        <TweakToggle label="Policy test" value={t.showPolicy} onChange={(v) => setTweak('showPolicy', v)} />
        <TweakToggle label="Live Cedar" value={t.showCedar} onChange={(v) => setTweak('showCedar', v)} />
        <TweakToggle label="Nav 펼침 (⌘B)" value={t.navExpanded} onChange={(v) => setTweak('navExpanded', v)} />

        <TweakSection label="캔버스" />
        <TweakRadio label="모눈 강도" value={t.gridStrength} options={['subtle', 'medium', 'strong']} onChange={(v) => setTweak('gridStrength', v)} />

        <TweakSection label="언어" />
        <TweakRadio label="locale" value={t.locale} options={['ko', 'en']} onChange={(v) => setTweak('locale', v)} />

        <TweakSection label="시드" />
        <TweakButton label="정책 리셋 (baseline)" onClick={() => { dispatch({ type: 'RESET' }); setSelectedId(null); setFocusPath(null); }} />
      </TweaksPanel>
    </>
  );
}

const root = ReactDOM.createRoot(document.getElementById('app'));
root.render(<V6App />);
