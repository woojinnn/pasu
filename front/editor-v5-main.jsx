// editor-v5-main.jsx — App composition

const { useState: useSM5, useEffect: useEM5, useMemo: useMM5, useReducer: useRM5, useRef: useRfM5, useCallback: useCbM5 } = React;

const V5_TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "showPalette": true,
  "showPolicy": true,
  "showCedar": true,
  "navExpanded": false,
  "locale": "ko",
  "gridStrength": "medium",
  "theme": "light"
}/*EDITMODE-END*/;

function makeHistory5() {
  const init = v5BuildBaseline();
  return { past: [], present: init, future: [], saved: JSON.stringify(init), lastChangeId: null };
}
function histReduce5(state, action) {
  if (action.type === 'UNDO') {
    if (!state.past.length) return state;
    return { past: state.past.slice(0, -1), present: state.past[state.past.length - 1],
      future: [state.present, ...state.future], saved: state.saved, lastChangeId: null };
  }
  if (action.type === 'REDO') {
    if (!state.future.length) return state;
    return { past: [...state.past, state.present], present: state.future[0],
      future: state.future.slice(1), saved: state.saved, lastChangeId: null };
  }
  if (action.type === 'MARK_SAVED') return { ...state, saved: JSON.stringify(state.present) };
  if (action.type === 'RESET') {
    const fresh = v5BuildBaseline();
    return { past: [], present: fresh, future: [], saved: JSON.stringify(fresh), lastChangeId: null };
  }
  const next = v5Reduce(state.present, action);
  if (next === state.present) return state;
  const isMove = action.type === 'MOVE';
  return {
    past: isMove ? state.past : [...state.past, state.present].slice(-80),
    present: next,
    future: isMove ? state.future : [],
    saved: state.saved,
    lastChangeId: next.lastChangeId || state.lastChangeId,
  };
}

function V5App() {
  const [t, setTweak] = useTweaks(V5_TWEAK_DEFAULTS);
  const [hist, dispatch] = useRM5(histReduce5, null, makeHistory5);
  const state = hist.present;

  const [mode, setMode] = useSM5('editor');
  const [focusedLeafId, setFocusedLeafId] = useSM5(null);
  const [selectedFxId, setSelectedFxId] = useSM5('fx2');
  const [manifestOpen, setManifestOpen] = useSM5(false);
  const [orModalFor, setORModalFor] = useSM5(null);  // { parentId, x, y }
  const [orSeen, setOrSeen] = useSM5(false);
  const [evalTick, setEvalTick] = useSM5(0);
  const [zoom, setZoom] = useSM5(1);
  const [cedarH, setCedarH] = useSM5(240);
  const saveBtnRef = useRfM5(null);

  const cedar = useMM5(() => v5ToCedar(state), [state]);
  const draftIds = useMM5(() => v5DraftIds(state), [state]);
  const fixture = V5_FIXTURES.find(f => f.id === selectedFxId) || V5_FIXTURES[0];
  const verdict = useMM5(() => v5Evaluate(state, fixture.tx), [state, fixture, evalTick]);
  const matchedLeafIds = verdict.matchedLeafIds;
  const dirty = JSON.stringify(state) !== hist.saved;
  const dirtyCount = useMM5(() => {
    if (!dirty) return 0;
    try {
      const saved = JSON.parse(hist.saved);
      const a = Object.keys(saved.nodes || {}).length;
      const b = Object.keys(state.nodes || {}).length;
      return Math.max(1, Math.abs(a - b) + 1);
    } catch { return 1; }
  }, [dirty, state, hist.saved]);

  // Keyboard
  useEM5(() => {
    const h = (e) => {
      if (e.target && (e.target.tagName === 'INPUT' || e.target.isContentEditable)) return;
      if ((e.metaKey || e.ctrlKey) && e.key === 'z' && !e.shiftKey) { e.preventDefault(); dispatch({ type: 'UNDO' }); }
      else if ((e.metaKey || e.ctrlKey) && ((e.shiftKey && e.key === 'z') || e.key === 'y')) { e.preventDefault(); dispatch({ type: 'REDO' }); }
      else if ((e.metaKey || e.ctrlKey) && e.key === 's') { e.preventDefault(); dispatch({ type: 'MARK_SAVED' }); }
      else if ((e.metaKey || e.ctrlKey) && e.key === 'b') { e.preventDefault(); setTweak('navExpanded', !t.navExpanded); }
      else if (e.key === 'Escape') {
        if (manifestOpen) setManifestOpen(false);
        else if (orModalFor) setORModalFor(null);
      } else if (e.shiftKey && e.key === '!') {
        // Shift+1 → reset to 100% (the keyboard sends '!' here on some layouts)
        setZoom(1);
      }
    };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, [t.navExpanded, manifestOpen, orModalFor]);

  // Close expanded nav on outside click
  useEM5(() => {
    if (!t.navExpanded) return;
    const h = (e) => { if (!e.target.closest('.nv')) setTweak('navExpanded', false); };
    const id = setTimeout(() => window.addEventListener('mousedown', h), 0);
    return () => { clearTimeout(id); window.removeEventListener('mousedown', h); };
  }, [t.navExpanded]);

  const onAimSave = () => {
    if (!saveBtnRef.current) return;
    saveBtnRef.current.classList.add('pulse');
    setTimeout(() => saveBtnRef.current && saveBtnRef.current.classList.remove('pulse'), 2800);
  };

  // OR confirm — palette drop or programmatic add
  const requestORFromCanvas = (opts) => {
    // opts: { x, y, parentId }
    if (orSeen) { dispatch({ type: 'ADD_GROUP', combinator: 'OR', ...opts }); return; }
    setORModalFor(opts);
  };
  const confirmOR = () => {
    if (orModalFor) dispatch({ type: 'ADD_GROUP', combinator: 'OR', ...orModalFor });
    setORModalFor(null);
  };

  // Add from palette CLICK (no drop position): drop near (300, 280)
  const addCondFromPalette = (sigId) => {
    dispatch({ type: 'ADD_CONDITION', sigId, parentId: null, x: 300 + Math.random() * 100, y: 280 + Math.random() * 80 });
  };
  const addGroupFromPalette = (kind) => {
    if (kind === 'OR') return requestORFromCanvas({ x: 300, y: 320, parentId: null });
    dispatch({ type: 'ADD_GROUP', combinator: kind, parentId: null, x: 300, y: 320 });
  };

  const onCopyCedar = () => { try { navigator.clipboard.writeText(cedar.lines.map(l => l.text).join('\n')); } catch {} };

  // Zoom helpers
  const setZoomClamped = (val) => setZoom(Math.max(0.25, Math.min(4, val)));
  const zoomSteps = [0.25, 0.5, 0.75, 1, 1.5, 2, 3, 4];
  const zoomOut = () => {
    const i = zoomSteps.findIndex(s => s >= zoom);
    const ni = Math.max(0, (i <= 0 ? 0 : i - 1));
    setZoom(zoomSteps[ni]);
  };
  const zoomIn = () => {
    const i = zoomSteps.findIndex(s => s > zoom);
    setZoom(i === -1 ? zoomSteps[zoomSteps.length - 1] : zoomSteps[i]);
  };
  const fitContent = () => setZoom(1);

  return (
    <>
      <V5NavRail locked={false} forceExpanded={t.navExpanded} />

      <main className="content">
        <div className="ed-root">
          <V5Topbar
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

          {mode === 'editor' ? (
            <div className="ws-layer">
              {/* Canvas */}
              <div className="ws-canvas-base">
                <V5Canvas
                  state={state} dispatch={dispatch}
                  focusedLeafId={focusedLeafId} setFocusedLeafId={setFocusedLeafId}
                  matchedLeafIds={matchedLeafIds}
                  zoom={zoom} setZoom={setZoomClamped}
                  gridStrength={t.gridStrength}
                  onRequestOR={requestORFromCanvas}
                />
              </div>

              {/* Top strip with chips + panel toggles + zoom */}
              <div className="top-strip">
                <div className={`ts-chip draft ${draftIds.length ? 'has' : ''}`}>
                  <span className="ts-dot" />
                  <span className="ts-k">미연결</span>
                  <span className="ts-n">{draftIds.length}개</span>
                </div>
                <div className="ts-chip">
                  <span className="ts-k">IN 정책</span>
                  <span className="ts-n">{Object.values(state.nodes).filter(n => n.kind === 'condition' && v5Included(state, n.id)).length}</span>
                </div>

                <span className="ts-spacer" />

                <button className={`ts-toggle ${t.showPalette ? 'open' : ''}`} onClick={() => setTweak('showPalette', !t.showPalette)}>
                  <span>Block palette</span>
                  <V5I.caretDown className="ts-tg-arr" style={{ width: 11, height: 11 }} />
                </button>
                <button className={`ts-toggle ${t.showPolicy ? 'open' : ''}`} onClick={() => setTweak('showPolicy', !t.showPolicy)}>
                  <span>Policy test</span>
                  <V5I.caretDown className="ts-tg-arr" style={{ width: 11, height: 11 }} />
                  {!t.showPolicy && verdict.decision.kind === 'Deny' && <span className="ts-tg-dot" title="Deny" />}
                </button>
                <button className={`ts-toggle ${t.showCedar ? 'open' : ''}`} onClick={() => setTweak('showCedar', !t.showCedar)}>
                  <span>Live Cedar</span>
                  <V5I.caretDown className="ts-tg-arr" style={{ width: 11, height: 11 }} />
                </button>

                <div className="ts-zoom">
                  <button onClick={zoomOut} title="Zoom out">−</button>
                  <span className="ts-zoom-pct">{Math.round(zoom * 100)}%</span>
                  <button onClick={zoomIn} title="Zoom in">+</button>
                  <button className="ts-zoom-fit" onClick={fitContent} title="Fit">fit</button>
                </div>
              </div>

              {/* Pull-down sheets */}
              {t.showPalette && (
                <V5Palette
                  onClose={() => setTweak('showPalette', false)}
                  onAddSignal={addCondFromPalette}
                  onAddGroup={addGroupFromPalette}
                  locale={t.locale}
                />
              )}
              {t.showPolicy && (
                <V5PolicyTest
                  onClose={() => setTweak('showPolicy', false)}
                  fixtures={V5_FIXTURES}
                  selectedFxId={selectedFxId}
                  onSelectFx={setSelectedFxId}
                  verdict={verdict}
                  state={state}
                  onFocusGuard={setFocusedLeafId}
                  onReevaluate={() => setEvalTick(x => x + 1)}
                  draftCount={draftIds.length}
                />
              )}
              {t.showCedar && (
                <V5CedarSheet
                  cedar={cedar}
                  onClose={() => setTweak('showCedar', false)}
                  focusedLeafId={focusedLeafId}
                  matchedLeafIds={matchedLeafIds}
                  lastChangeId={hist.lastChangeId}
                  onCopy={onCopyCedar}
                  height={cedarH}
                  onHeightChange={setCedarH}
                />
              )}
            </div>
          ) : (
            <div className="ws-layer" style={{ background: 'transparent', border: 0, padding: 0 }}>
              <V5CodeMode
                cedar={cedar}
                focusedLeafId={focusedLeafId}
                matchedLeafIds={matchedLeafIds}
                lastChangeId={hist.lastChangeId}
                fixtures={V5_FIXTURES}
                selectedFxId={selectedFxId}
                onSelectFx={setSelectedFxId}
                verdict={verdict}
                state={state}
                onFocusGuard={setFocusedLeafId}
                onReevaluate={() => setEvalTick(x => x + 1)}
                draftCount={draftIds.length}
                onCopy={onCopyCedar}
              />
            </div>
          )}

          <V5SaveBar dirty={dirty} dirtyCount={dirtyCount} onAimSave={onAimSave} />
        </div>
      </main>

      <V5ManifestSlideover open={manifestOpen} onClose={() => setManifestOpen(false)} />

      {orModalFor && (
        <V5ORConfirmModal
          onConfirm={confirmOR}
          onCancel={() => setORModalFor(null)}
          dontShowAgain={orSeen}
          onSetDontShow={setOrSeen}
        />
      )}

      <TweaksPanel>
        <TweakSection label="패널 (상단 토글에서도 토글 가능)" />
        <TweakToggle label="Block palette" value={t.showPalette} onChange={(v) => setTweak('showPalette', v)} />
        <TweakToggle label="Policy test" value={t.showPolicy} onChange={(v) => setTweak('showPolicy', v)} />
        <TweakToggle label="Live Cedar" value={t.showCedar} onChange={(v) => setTweak('showCedar', v)} />
        <TweakToggle label="Nav 펼침 (⌘B)" value={t.navExpanded} onChange={(v) => setTweak('navExpanded', v)} />

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
root.render(<V5App />);
