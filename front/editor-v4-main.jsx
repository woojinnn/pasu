// editor-v4-main.jsx — Layered composition.
// Canvas is full-bleed baseline; palette / policy / cedar are absolute overlays
// on top. Toggling panels never reflows the canvas.
// Nav rail is locked-collapsed; ⌘B opens it as a 256px overlay (also no reflow).

const { useState: useS4, useEffect: useE4, useMemo: useM4, useReducer: useR4, useRef: useRf4 } = React;

const V4_TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "navExpanded": false,
  "leftCollapsed": false,
  "rightCollapsed": false,
  "cedarCollapsed": false,
  "locale": "ko",
  "gridStrength": "medium",
  "theme": "light"
}/*EDITMODE-END*/;

// Reuse the v3 history reducer logic
function makeHistory4() {
  const init = v3BuildBaseline();
  return { past: [], present: init, future: [], saved: JSON.stringify(init), lastChangeId: null };
}
function histReduce4(state, action) {
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
  const isMove = action.type === 'MOVE';
  return {
    past: isMove ? state.past : [...state.past, state.present].slice(-60),
    present: next,
    future: isMove ? state.future : [],
    saved: state.saved,
    lastChangeId: next.lastChangeId || state.lastChangeId,
  };
}

// ─── Edge handle components ─────────────────────────────────────────────
function LeftEdgeHandle({ onClick, label }) {
  return (
    <div className="edge-handle" onClick={onClick}>
      <div className="eh-top">
        <div className="eh-arrow"><V3I.caretRight style={{ width: 12, height: 12 }} /></div>
        <span className="eh-lbl">{label}</span>
      </div>
    </div>
  );
}
function RightEdgeHandle({ onClick, label, warn }) {
  return (
    <div className="edge-handle" onClick={onClick}>
      <div className="eh-top">
        <div className="eh-arrow"><V3I.caretLeft style={{ width: 12, height: 12 }} /></div>
        <span className="eh-lbl">{label}</span>
      </div>
      {warn && <div className="eh-warn" title="현재 평결: Deny" />}
    </div>
  );
}
function BottomEdgeHandle({ onClick, label, meta }) {
  return (
    <div className="edge-handle-h" onClick={onClick}>
      <div className="ehh-arr"><V3I.caretDown style={{ width: 12, height: 12, transform: 'rotate(180deg)' }} /></div>
      <span>{label}</span>
      <span className="ehh-meta">{meta}</span>
    </div>
  );
}

function V4App() {
  const [t, setTweak] = useTweaks(V4_TWEAK_DEFAULTS);
  const [hist, dispatch] = useR4(histReduce4, null, makeHistory4);
  const state = hist.present;

  const [mode, setMode] = useS4('editor');
  const [focusedLeafId, setFocusedLeafId] = useS4(null);
  const [selectedFxId, setSelectedFxId] = useS4('fx2');
  const [manifestOpen, setManifestOpen] = useS4(false);
  const [orModalFor, setORModalFor] = useS4(null);
  const [orSeen, setOrSeen] = useS4(false);
  const [evalTick, setEvalTick] = useS4(0);
  const saveBtnRef = useRf4(null);
  const [dragSig, setDragSig] = useS4(null);

  // Derived
  const cedar = useM4(() => v3ToCedar(state), [state]);
  const draftIds = useM4(() => v3DraftIds(state), [state]);
  const fixture = V3_FIXTURES.find(f => f.id === selectedFxId) || V3_FIXTURES[0];
  const verdict = useM4(() => v3Evaluate(state, fixture.tx), [state, fixture, evalTick]);
  const matchedLeafIds = verdict.matchedLeafIds;
  const dirty = JSON.stringify(state) !== hist.saved;
  const dirtyCount = useM4(() => {
    if (!dirty) return 0;
    try {
      const saved = JSON.parse(hist.saved);
      const a = Object.keys(saved.nodes || {}).length;
      const b = Object.keys(state.nodes || {}).length;
      return Math.max(1, Math.abs(a - b) + 1);
    } catch { return 1; }
  }, [dirty, state, hist.saved]);

  // Keyboard: ⌘Z, ⌘⇧Z, ⌘S, ⌘B (nav toggle)
  useE4(() => {
    const h = (e) => {
      if (e.target && (e.target.tagName === 'INPUT' || e.target.isContentEditable)) return;
      if ((e.metaKey || e.ctrlKey) && e.key === 'z' && !e.shiftKey) { e.preventDefault(); dispatch({ type: 'UNDO' }); }
      else if ((e.metaKey || e.ctrlKey) && ((e.shiftKey && e.key === 'z') || e.key === 'y')) { e.preventDefault(); dispatch({ type: 'REDO' }); }
      else if ((e.metaKey || e.ctrlKey) && e.key === 's') { e.preventDefault(); dispatch({ type: 'MARK_SAVED' }); }
      else if ((e.metaKey || e.ctrlKey) && e.key === 'b') { e.preventDefault(); setTweak('navExpanded', !t.navExpanded); }
    };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, [t.navExpanded]);

  // Close expanded nav on outside click
  useE4(() => {
    if (!t.navExpanded) return;
    const h = (e) => {
      if (!e.target.closest('.nv')) setTweak('navExpanded', false);
    };
    // delay so the keystroke that opened it doesn't immediately close it
    const id = setTimeout(() => window.addEventListener('mousedown', h), 0);
    return () => { clearTimeout(id); window.removeEventListener('mousedown', h); };
  }, [t.navExpanded]);

  const onAimSave = () => {
    if (!saveBtnRef.current) return;
    saveBtnRef.current.classList.add('pulse');
    setTimeout(() => saveBtnRef.current && saveBtnRef.current.classList.remove('pulse'), 2800);
  };

  // OR confirm
  const requestOR = (parentId) => {
    if (orSeen) { dispatch({ type: 'ADD_CONTAINER', op: 'OR', parentId }); return; }
    const hadOR = v3HasOR(state);
    if (!hadOR) setORModalFor(parentId);
    else dispatch({ type: 'ADD_CONTAINER', op: 'OR', parentId });
  };
  const confirmOR = () => {
    if (orModalFor) dispatch({ type: 'ADD_CONTAINER', op: 'OR', parentId: orModalFor });
    setORModalFor(null);
  };

  const addSignalFromPaletteClick = (sigId) => {
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

  // Workspace classes — only affect the bottom Cedar's left/right edges
  // so it can extend toward whichever side panel is collapsed.
  const wsCls = [
    'ws-layer',
    t.leftCollapsed ? 'left-col' : '',
    t.rightCollapsed ? 'right-col' : '',
  ].filter(Boolean).join(' ');
  const cedarOvCls = [
    'ov-pane', 'ov-cedar',
    t.cedarCollapsed ? 'col' : '',
    t.leftCollapsed ? 'left-col' : '',
    t.rightCollapsed ? 'right-col' : '',
  ].filter(Boolean).join(' ');

  return (
    <>
      {/* z:50 — Nav rail (locked-collapsed in editor; ⌘B → overlay expand) */}
      <V3NavRail collapsed={!t.navExpanded} />
      <div className="nv-kbd-hint" style={{
        position: 'absolute', left: 0, width: 64, bottom: 8,
        textAlign: 'center', zIndex: 51, pointerEvents: 'none',
        fontFamily: 'var(--ff-mono)', fontSize: 9.5,
        color: 'var(--slate-400)', letterSpacing: '0.06em',
        display: t.navExpanded ? 'none' : 'block',
      }}>⌘B</div>

      <main className="content">
        <div className="ed-root">
          {/* z:20 — Top chrome */}
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

          {/* z:0 → z:10 — Layered workspace */}
          {mode === 'editor' ? (
            <div className={wsCls}>
              {/* z:0 — canvas baseline (full-bleed under all overlays) */}
              <div className="ws-canvas-base">
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

              {/* z:10 — Palette overlay */}
              <aside className={`ov-pane ov-pal ${t.leftCollapsed ? 'col' : ''}`}>
                {t.leftCollapsed ? (
                  <LeftEdgeHandle onClick={() => setTweak('leftCollapsed', false)} label="Block palette" />
                ) : (
                  <V3Palette
                    collapsed={false}
                    onToggle={() => setTweak('leftCollapsed', true)}
                    onAddSignal={addSignalFromPaletteClick}
                    locale={t.locale}
                    onDragStart={setDragSig} onDragEnd={() => setDragSig(null)}
                    draggingSig={dragSig}
                  />
                )}
              </aside>

              {/* z:10 — Policy test overlay */}
              <aside className={`ov-pane ov-pt ${t.rightCollapsed ? 'col' : ''}`}>
                {t.rightCollapsed ? (
                  <RightEdgeHandle
                    onClick={() => setTweak('rightCollapsed', false)}
                    label={`Policy · ${verdict.decision.kind}`}
                    warn={verdict.decision.kind === 'Deny'}
                  />
                ) : (
                  <V3PolicyTest
                    collapsed={false}
                    onToggle={() => setTweak('rightCollapsed', true)}
                    fixtures={V3_FIXTURES}
                    selectedFxId={selectedFxId}
                    onSelectFx={setSelectedFxId}
                    verdict={verdict}
                    state={state}
                    onFocusGuard={setFocusedLeafId}
                    onReevaluate={() => setEvalTick(x => x + 1)}
                    draftCount={draftIds.length}
                  />
                )}
              </aside>

              {/* z:10 — Cedar bottom overlay */}
              <div className={cedarOvCls}>
                {t.cedarCollapsed ? (
                  <BottomEdgeHandle
                    onClick={() => setTweak('cedarCollapsed', false)}
                    label="Live Cedar preview"
                    meta="펴서 보기 · ⌘E"
                  />
                ) : (
                  <V3CedarStrip
                    cedar={cedar}
                    collapsed={false}
                    onToggle={() => setTweak('cedarCollapsed', true)}
                    focusedLeafId={focusedLeafId}
                    matchedLeafIds={matchedLeafIds}
                    lastChangeId={hist.lastChangeId}
                    onCopy={onCopyCedar}
                  />
                )}
              </div>
            </div>
          ) : (
            /* Code mode — full pane, no overlays */
            <div className="ws-layer" style={{ display: 'flex' }}>
              <V3CodeMode cedar={cedar} focusedLeafId={focusedLeafId} matchedLeafIds={matchedLeafIds} />
            </div>
          )}

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

      <TweaksPanel>
        <TweakSection label="레이어드 패널" />
        <TweakToggle label="좌측 팔레트 펼침" value={!t.leftCollapsed} onChange={(v) => setTweak('leftCollapsed', !v)} />
        <TweakToggle label="우측 Policy test 펼침" value={!t.rightCollapsed} onChange={(v) => setTweak('rightCollapsed', !v)} />
        <TweakToggle label="하단 Cedar preview 펼침" value={!t.cedarCollapsed} onChange={(v) => setTweak('cedarCollapsed', !v)} />
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
root.render(<V4App />);
