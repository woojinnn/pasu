// editor-v3-main.jsx — App composition

const { useState: useSM, useEffect: useEM, useMemo: useMM, useReducer: useRM, useRef: useRfM, useCallback: useCbM } = React;

const V3_TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "navCollapsed": false,
  "leftCollapsed": false,
  "rightCollapsed": false,
  "cedarCollapsed": false,
  "locale": "ko",
  "gridStrength": "medium",
  "theme": "light"
}/*EDITMODE-END*/;

// ─── History reducer ─────────────────────────────────────────────────────────
function makeHistory(seed) {
  const init = v3BuildBaseline();
  return {
    past: [], present: init, future: [],
    saved: JSON.stringify(init),
    lastChangeId: null,
  };
}
function histReduce(state, action) {
  if (action.type === 'UNDO') {
    if (!state.past.length) return state;
    const prev = state.past[state.past.length - 1];
    return { past: state.past.slice(0, -1), present: prev, future: [state.present, ...state.future], saved: state.saved, lastChangeId: null };
  }
  if (action.type === 'REDO') {
    if (!state.future.length) return state;
    const next = state.future[0];
    return { past: [...state.past, state.present], present: next, future: state.future.slice(1), saved: state.saved, lastChangeId: null };
  }
  if (action.type === 'MARK_SAVED') return { ...state, saved: JSON.stringify(state.present) };
  if (action.type === 'RESET') {
    const fresh = v3BuildBaseline();
    return { past: [], present: fresh, future: [], saved: JSON.stringify(fresh), lastChangeId: null };
  }
  const next = v3Reduce(state.present, action);
  if (next === state.present) return state;
  // Don't push history for pure MOVE actions (they happen continuously during drag)
  const isMove = action.type === 'MOVE';
  return {
    past: isMove ? state.past : [...state.past, state.present].slice(-60),
    present: next,
    future: isMove ? state.future : [],
    saved: state.saved,
    lastChangeId: next.lastChangeId || state.lastChangeId,
  };
}

// ─── App ─────────────────────────────────────────────────────────────────────
function V3App() {
  const [t, setTweak] = useTweaks(V3_TWEAK_DEFAULTS);
  const [hist, dispatch] = useRM(histReduce, null, makeHistory);
  const state = hist.present;

  const [mode, setMode] = useSM('editor');
  const [focusedLeafId, setFocusedLeafId] = useSM(null);
  const [selectedFxId, setSelectedFxId] = useSM('fx2');
  const [manifestOpen, setManifestOpen] = useSM(false);
  const [orModalFor, setORModalFor] = useSM(null);
  const [orSeen, setOrSeen] = useSM(false);
  const [evalTick, setEvalTick] = useSM(0);
  const saveBtnRef = useRfM(null);

  // Derived
  const cedar = useMM(() => v3ToCedar(state), [state]);
  const draftIds = useMM(() => v3DraftIds(state), [state]);
  const fixture = V3_FIXTURES.find(f => f.id === selectedFxId) || V3_FIXTURES[0];
  const verdict = useMM(() => v3Evaluate(state, fixture.tx), [state, fixture, evalTick]);
  const matchedLeafIds = verdict.matchedLeafIds;
  const dirty = JSON.stringify(state) !== hist.saved;
  const dirtyCount = useMM(() => {
    if (!dirty) return 0;
    try {
      const saved = JSON.parse(hist.saved);
      const a = Object.keys(saved.nodes || {}).length;
      const b = Object.keys(state.nodes || {}).length;
      return Math.max(1, Math.abs(a - b) + 1);
    } catch { return 1; }
  }, [dirty, state, hist.saved]);

  // Keyboard shortcuts
  useEM(() => {
    const h = (e) => {
      if (e.target && (e.target.tagName === 'INPUT' || e.target.isContentEditable)) return;
      if ((e.metaKey || e.ctrlKey) && e.key === 'z' && !e.shiftKey) { e.preventDefault(); dispatch({ type: 'UNDO' }); }
      else if ((e.metaKey || e.ctrlKey) && ((e.shiftKey && e.key === 'z') || e.key === 'y')) { e.preventDefault(); dispatch({ type: 'REDO' }); }
      else if ((e.metaKey || e.ctrlKey) && e.key === 's') { e.preventDefault(); dispatch({ type: 'MARK_SAVED' }); }
    };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, []);

  const onAimSave = () => {
    if (!saveBtnRef.current) return;
    saveBtnRef.current.classList.add('pulse');
    setTimeout(() => saveBtnRef.current && saveBtnRef.current.classList.remove('pulse'), 2800);
  };

  // OR confirmation
  const requestOR = (parentId) => {
    if (orSeen) { dispatch({ type: 'ADD_CONTAINER', op: 'OR', parentId }); return; }
    // Only show modal if this would introduce OR for the first time in policy
    const hadOR = v3HasOR(state);
    if (!hadOR) setORModalFor(parentId);
    else dispatch({ type: 'ADD_CONTAINER', op: 'OR', parentId });
  };
  const confirmOR = () => {
    if (orModalFor) dispatch({ type: 'ADD_CONTAINER', op: 'OR', parentId: orModalFor });
    setORModalFor(null);
  };

  // Palette drag tracking (visual only)
  const [dragSig, setDragSig] = useSM(null);

  // Layout class
  let wsClass = 'layout-default';
  if (mode === 'code') wsClass = 'layout-code-mode';
  else if (t.leftCollapsed && t.rightCollapsed) wsClass = 'layout-both-col';
  else if (t.leftCollapsed) wsClass = 'layout-palette-col';
  else if (t.rightCollapsed) wsClass = 'layout-policy-col';

  // Add signal helper: add as draft on canvas if root has children else attach to root
  const addSignalFromPaletteClick = (sigId) => {
    // Try to drop in root container directly
    const root = state.nodes[state.rootId];
    if (root && root.childIds.length < 6) {
      dispatch({ type: 'ADD_FROM_PALETTE', sigId, parentId: state.rootId });
    } else {
      dispatch({ type: 'ADD_FROM_PALETTE', sigId, parentId: null, x: 120 + Math.random() * 120, y: 560 + Math.random() * 50 });
    }
  };

  const onCopyCedar = () => {
    try {
      const txt = cedar.lines.map(l => l.text).join('\n');
      navigator.clipboard.writeText(txt);
    } catch {}
  };

  return (
    <>
      <V3NavRail collapsed={t.navCollapsed} />
      <main className={`content ${t.navCollapsed ? 'nv-col' : ''}`}>
        <div className="ed-root">
          <V3Topbar
            mode={mode} onModeChange={setMode}
            dirty={dirty}
            undoEnabled={hist.past.length > 0} redoEnabled={hist.future.length > 0}
            onUndo={() => dispatch({ type: 'UNDO' })}
            onRedo={() => dispatch({ type: 'REDO' })}
            onSave={() => dispatch({ type: 'MARK_SAVED' })}
            saveBtnRef={saveBtnRef}
            category="DEX" action="swap"
            manifestHash={state.manifestHash}
            signalCounts={state.signalCounts}
            onOpenManifest={() => setManifestOpen(true)}
          />

          <div className={`ws ${wsClass}`} style={{ minHeight: 700 }}>
            {mode === 'editor' ? (
              <>
                <V3Palette
                  collapsed={t.leftCollapsed}
                  onToggle={() => setTweak('leftCollapsed', !t.leftCollapsed)}
                  onAddSignal={addSignalFromPaletteClick}
                  locale={t.locale}
                  onDragStart={setDragSig} onDragEnd={() => setDragSig(null)}
                  draggingSig={dragSig}
                />

                <div className="main-col">
                  <div className="pane-canvas" style={{ display: 'flex', minHeight: 0 }}>
                    <V3Canvas
                      state={state}
                      draftIds={draftIds}
                      dispatch={dispatch}
                      focusedLeafId={focusedLeafId}
                      setFocusedLeafId={setFocusedLeafId}
                      matchedLeafIds={matchedLeafIds}
                      onRequestOR={requestOR}
                      onAddANDInside={(parentId) => dispatch({ type: 'ADD_CONTAINER', op: 'AND', parentId })}
                      gridStrength={t.gridStrength}
                    />
                  </div>

                  <div className="pane-cedar">
                    <V3CedarStrip
                      cedar={cedar}
                      collapsed={t.cedarCollapsed}
                      onToggle={() => setTweak('cedarCollapsed', !t.cedarCollapsed)}
                      focusedLeafId={focusedLeafId}
                      matchedLeafIds={matchedLeafIds}
                      lastChangeId={hist.lastChangeId}
                      onCopy={onCopyCedar}
                    />
                  </div>
                </div>

                <V3PolicyTest
                  collapsed={t.rightCollapsed}
                  onToggle={() => setTweak('rightCollapsed', !t.rightCollapsed)}
                  fixtures={V3_FIXTURES}
                  selectedFxId={selectedFxId}
                  onSelectFx={setSelectedFxId}
                  verdict={verdict}
                  state={state}
                  onFocusGuard={setFocusedLeafId}
                  onReevaluate={() => setEvalTick(x => x + 1)}
                  draftCount={draftIds.length}
                />
              </>
            ) : (
              <V3CodeMode cedar={cedar} focusedLeafId={focusedLeafId} matchedLeafIds={matchedLeafIds} />
            )}
          </div>

          <V3SaveBar dirty={dirty} dirtyCount={dirtyCount} onAimSave={onAimSave} />
        </div>
      </main>

      <V3ManifestSlideover open={manifestOpen} onClose={() => setManifestOpen(false)} />

      {orModalFor && (
        <V3ORConfirmModal
          onConfirm={confirmOR}
          onCancel={() => setORModalFor(null)}
          dontShowAgain={orSeen}
          onSetDontShow={setOrSeen}
        />
      )}

      <V3FAB theme={t.theme} onThemeChange={(v) => setTweak('theme', v)} />

      {/* Tweaks panel */}
      <TweaksPanel>
        <TweakSection label="레이아웃" />
        <TweakToggle label="Nav 접기" value={t.navCollapsed} onChange={(v) => setTweak('navCollapsed', v)} />
        <TweakToggle label="좌측 팔레트 접기" value={t.leftCollapsed} onChange={(v) => setTweak('leftCollapsed', v)} />
        <TweakToggle label="우측 Policy test 접기" value={t.rightCollapsed} onChange={(v) => setTweak('rightCollapsed', v)} />
        <TweakToggle label="하단 Cedar preview 접기" value={t.cedarCollapsed} onChange={(v) => setTweak('cedarCollapsed', v)} />

        <TweakSection label="캔버스" />
        <TweakRadio label="모눈 강도" value={t.gridStrength}
          options={['subtle', 'medium', 'strong']}
          onChange={(v) => setTweak('gridStrength', v)} />

        <TweakSection label="언어" />
        <TweakRadio label="locale" value={t.locale}
          options={['ko', 'en']}
          onChange={(v) => setTweak('locale', v)} />

        <TweakSection label="시드" />
        <TweakButton label="정책 리셋 (baseline)" onClick={() => dispatch({ type: 'RESET' })} />
      </TweaksPanel>
    </>
  );
}

const root = ReactDOM.createRoot(document.getElementById('app'));
root.render(<V3App />);
