// editor-v2-main.jsx
// App composition — wires IR reducer, history, evaluator, save state,
// adaptive canvas, code mode, modals, tweaks, FAB, nav rail.

const { useState: useS, useEffect: useE, useMemo: useM, useReducer: useR, useRef: useRf, useCallback: useCb } = React;

// ─── Tweak defaults ─────────────────────────────────────────────────────────
const V2_TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "navCollapsed": false,
  "density": "regular",
  "locale": "ko",
  "gridStrength": "medium",
  "orStyle": "tinted",
  "theme": "light",
  "orConfirmStrength": "modal",
  "seedPolicy": "baseline-or",
  "verdictPosition": "top-right"
}/*EDITMODE-END*/;

// ─── History-aware reducer wrapper ──────────────────────────────────────────
function buildInitialHistory(seed) {
  const initial = seed === 'flat-and' ? v2BuildFlatAND() : v2BuildBaseline();
  return {
    past: [],
    present: initial,
    future: [],
    savedSnapshot: JSON.stringify(initial),
    lastChangedAt: null,
  };
}

function historyReducer(state, action) {
  if (action.type === 'UNDO') {
    if (state.past.length === 0) return state;
    const previous = state.past[state.past.length - 1];
    return {
      past: state.past.slice(0, -1),
      present: previous,
      future: [state.present, ...state.future],
      savedSnapshot: state.savedSnapshot,
      lastChangedAt: { ts: Date.now() },
    };
  }
  if (action.type === 'REDO') {
    if (state.future.length === 0) return state;
    const next = state.future[0];
    return {
      past: [...state.past, state.present],
      present: next,
      future: state.future.slice(1),
      savedSnapshot: state.savedSnapshot,
      lastChangedAt: { ts: Date.now() },
    };
  }
  if (action.type === 'MARK_SAVED') {
    return { ...state, savedSnapshot: JSON.stringify(state.present) };
  }
  if (action.type === 'RESET_SEED') {
    const fresh = action.seed === 'flat-and' ? v2BuildFlatAND() : v2BuildBaseline();
    return {
      past: [], present: fresh, future: [],
      savedSnapshot: JSON.stringify(fresh),
      lastChangedAt: { ts: Date.now() },
    };
  }
  // Mutating actions → run v2Reduce against present
  const next = v2Reduce(state.present, action);
  if (next === state.present) return state;
  return {
    past: [...state.past, state.present].slice(-50),
    present: next,
    future: [],
    savedSnapshot: state.savedSnapshot,
    lastChangedAt: { ts: Date.now(), guardId: action.leafId || action.containerId || state.lastChangedAt?.guardId || null },
  };
}

// ─── App ────────────────────────────────────────────────────────────────────
function V2EditorApp() {
  const [t, setTweak] = useTweaks(V2_TWEAK_DEFAULTS);
  const [hist, dispatch] = useR(historyReducer, t.seedPolicy, buildInitialHistory);
  const state = hist.present;

  // Modes
  const [mode, setMode] = useS('editor');
  const [focusedLeafId, setFocusedLeafId] = useS(null);
  const [selectedFxId, setSelectedFxId] = useS('fx2');
  const [manifestOpen, setManifestOpen] = useS(false);
  const [launcherOpen, setLauncherOpen] = useS(false);
  const [orModalFor, setORModalFor] = useS(null);    // containerId pending OR child
  const [orSeen, setOrSeen] = useS(false);            // session-level suppress
  const [evalTick, setEvalTick] = useS(0);
  const saveBtnRef = useRf(null);

  // Re-seed if tweak toggled
  useE(() => { dispatch({ type: 'RESET_SEED', seed: t.seedPolicy }); }, [t.seedPolicy]);

  // Compute derived: Cedar text, structure flag, verdict
  const cedar = useM(() => v2ToCedar(state), [state]);
  const isFlat = useM(() => v2IsFlatAND(state.root), [state.root]);
  const fixture = V2_TEST_FIXTURES.find(f => f.id === selectedFxId) || V2_TEST_FIXTURES[0];
  const verdict = useM(() => v2Evaluate(state, fixture.tx), [state, fixture, evalTick]);
  const dirty = JSON.stringify(state) !== hist.savedSnapshot;
  const dirtyCount = useM(() => {
    if (!dirty) return 0;
    // Coarse approximation: count leaves difference vs saved
    try {
      const saved = JSON.parse(hist.savedSnapshot);
      return Math.max(1, Math.abs(countLeaves(state.root) - countLeaves(saved.root)) + 1);
    } catch { return 1; }
  }, [dirty, state, hist.savedSnapshot]);

  const onUndo = () => dispatch({ type: 'UNDO' });
  const onRedo = () => dispatch({ type: 'REDO' });
  const onSave = () => dispatch({ type: 'MARK_SAVED' });

  // Keyboard
  useE(() => {
    const h = (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'z' && !e.shiftKey) { e.preventDefault(); onUndo(); }
      if ((e.metaKey || e.ctrlKey) && ((e.shiftKey && e.key === 'z') || e.key === 'y')) { e.preventDefault(); onRedo(); }
      if ((e.metaKey || e.ctrlKey) && e.key === 's') { e.preventDefault(); onSave(); }
    };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, []);

  // Save-button pulse trigger (from save-bar "강조" button)
  const onAimSave = () => {
    if (!saveBtnRef.current) return;
    saveBtnRef.current.classList.add('pulse');
    setTimeout(() => saveBtnRef.current && saveBtnRef.current.classList.remove('pulse'), 2800);
  };

  // ─── Actions on IR ────────────────────────────────────────────────────────
  const addToContainer = (containerId, sigId) => {
    dispatch({ type: 'ADD_LEAF', containerId, sigId });
  };
  const addToRoot = (sigId) => addToContainer(state.root.id, sigId);

  const requestOR = (containerId) => {
    if (orSeen || t.orConfirmStrength === 'none') {
      dispatch({ type: 'ADD_CONTAINER', containerId, op: 'OR' });
      return;
    }
    setORModalFor(containerId);
  };
  const requestAND = (containerId) => {
    dispatch({ type: 'ADD_CONTAINER', containerId, op: 'AND' });
  };
  const confirmOR = () => {
    if (orModalFor) dispatch({ type: 'ADD_CONTAINER', containerId: orModalFor, op: 'OR' });
    setORModalFor(null);
  };

  const patchLeaf = (leafId, patch) => dispatch({ type: 'UPDATE_LEAF', leafId, patch });
  const deleteNode = (id) => dispatch({ type: 'DELETE_NODE', id });
  const editReason = (reason) => dispatch({ type: 'UPDATE_DECISION', patch: { reason } });

  // Palette drag bookkeeping
  const [dragId, setDragId] = useS(null);
  const onDragStartPal = (sigId) => setDragId(sigId);
  const onDragEndPal = () => setDragId(null);

  // ── Render tree (recursive — leaf vs container) ───────────────────────────
  const renderNode = (node, parentContainerId) => {
    if (node.kind === 'leaf') {
      return (
        <GuardBlockRow
          key={node.id}
          leaf={node}
          focused={focusedLeafId === node.id}
          matched={verdict.matchedLeafIds.includes(node.id)}
          onFocus={() => setFocusedLeafId(node.id)}
          onPatch={(p) => patchLeaf(node.id, p)}
          onDelete={() => deleteNode(node.id)}
          locale={t.locale}
        />
      );
    }
    // Nested container
    return (
      <ContainerBox
        key={node.id}
        node={node}
        depth={1}
        orStyle={t.orStyle}
        onDelete={(id) => deleteNode(id)}
        onAddOR={requestOR}
        onAddAND={requestAND}
        onRequestOR={requestOR}
        onAddLeaf={addToContainer}
        onDropOnContainer={addToContainer}
        renderChild={renderNode}
      />
    );
  };

  // ── Adaptive root rendering ───────────────────────────────────────────────
  const renderRoot = () => {
    if (isFlat) {
      return (
        <div className="flat" onDragOver={(e) => {
          if (e.dataTransfer.types.includes('text/v2-sig')) e.preventDefault();
        }} onDrop={(e) => {
          const sig = e.dataTransfer.getData('text/v2-sig');
          if (sig) { e.preventDefault(); addToRoot(sig); }
        }}>
          <div className="flat-h">
            <span className="flat-h-pill">AND · 모두 참</span>
            <span className="flat-h-t">평면(form) 레이아웃 — 구조가 단순해 폼처럼 펼쳐집니다</span>
            <span className="flat-h-c">{state.root.children.length}개 조건 · 빠른 입력</span>
          </div>
          <div className="flat-rows">
            {state.root.children.map(node => renderNode(node, state.root.id))}
            {state.root.children.length === 0 && (
              <div className="drop-zone">
                팔레트에서 블록을 드래그하거나 클릭으로 첫 조건을 추가하세요
              </div>
            )}
          </div>
          <div style={{ display: 'flex', gap: 8, marginTop: 12, paddingTop: 12, borderTop: '1px dashed var(--hairline-soft)' }}>
            <button className="ctn-add-btn" onClick={() => requestAND(state.root.id)}>
              <V2I.plus style={{ width: 11, height: 11 }} /> AND 묶음
            </button>
            <button className="ctn-add-btn" onClick={() => requestOR(state.root.id)}>
              <V2I.plus style={{ width: 11, height: 11 }} /> OR 묶음 → 중첩으로 전환
            </button>
          </div>
        </div>
      );
    }

    // Nested canvas mode — root is OR (or has nesting)
    return (
      <ContainerBox
        node={state.root}
        depth={0}
        orStyle={t.orStyle}
        onDelete={(id) => deleteNode(id)}
        onAddOR={requestOR}
        onAddAND={requestAND}
        onRequestOR={requestOR}
        onAddLeaf={addToContainer}
        onDropOnContainer={addToContainer}
        renderChild={renderNode}
      />
    );
  };

  return (
    <>
      <V2NavRail collapsed={t.navCollapsed} />
      <main className={`content-v2 ${t.navCollapsed ? 'nav-collapsed' : ''}`}>
        <div className={`ed-root density-${t.density} locale-${t.locale}`}>
          <V2Topbar
            mode={mode}
            onModeChange={setMode}
            dirty={dirty}
            undoEnabled={hist.past.length > 0}
            redoEnabled={hist.future.length > 0}
            onUndo={onUndo}
            onRedo={onRedo}
            onSave={onSave}
            category="DEX"
            action="swap"
            manifestHash={state.manifestHash}
            signalCounts={state.signalCounts}
            onOpenManifest={() => setManifestOpen(true)}
            saveButtonRef={saveBtnRef}
            dirtyCount={dirtyCount}
          />

          {/* Workspace */}
          <div className={`ws ${mode === 'code' ? 'no-palette' : ''}`}>
            {mode !== 'code' && (
              <V2Palette
                draggingId={dragId}
                onAddSignal={(sigId) => addToRoot(sigId)}
                onDragStart={onDragStartPal}
                onDragEnd={onDragEndPal}
                locale={t.locale}
              />
            )}

            <div className="ws-pane ws-main" style={mode === 'code' ? { padding: 0, background: 'transparent', border: 0 } : null}>
              {mode === 'editor' ? (
                <div className={`cv grid-${t.gridStrength}`}>
                  <div className="cv-scroll">
                    <CanvasTrigger />
                    {renderRoot()}
                    <CanvasDecision
                      decision={state.decision}
                      onEditReason={(r) => editReason(r.replace(/^"|"$/g, ''))}
                      triggered={verdict.triggered}
                    />
                  </div>

                  <VerdictPill
                    verdict={verdict}
                    fixtures={V2_TEST_FIXTURES}
                    selectedFixtureId={selectedFxId}
                    onSelectFixture={setSelectedFxId}
                    onFocusGuard={setFocusedLeafId}
                    onToggleLauncher={() => setLauncherOpen(o => !o)}
                    launcherOpen={launcherOpen}
                  />

                  <button className="ctx-launcher" onClick={() => setLauncherOpen(o => !o)} title="검증 도구">
                    <span style={{ fontSize: 22, fontWeight: 300 }}>⊕</span>
                  </button>

                  <ContextLauncher
                    open={launcherOpen}
                    onClose={() => setLauncherOpen(false)}
                    fixtures={V2_TEST_FIXTURES}
                    selectedFixtureId={selectedFxId}
                    onSelectFixture={setSelectedFxId}
                    tx={fixture.tx}
                    onReevaluate={() => setEvalTick(x => x + 1)}
                    onExportCedar={() => alert('Cedar 내보내기 — 데모')}
                  />
                </div>
              ) : (
                <V2CodeView
                  cedar={cedar}
                  focusedLeafId={focusedLeafId}
                  matchedLeafIds={verdict.matchedLeafIds}
                  lastChangedAt={hist.lastChangedAt}
                />
              )}
            </div>
          </div>

          <V2SaveBar dirty={dirty} dirtyCount={dirtyCount} onAimSave={onAimSave} />
        </div>
      </main>

      <V2ManifestSlideover open={manifestOpen} onClose={() => setManifestOpen(false)} />

      {orModalFor && (
        <ORConfirmModal
          onConfirm={confirmOR}
          onCancel={() => setORModalFor(null)}
          dontShowAgain={orSeen}
          onSetDontShow={setOrSeen}
        />
      )}

      <V2FAB theme={t.theme} onThemeChange={(v) => setTweak('theme', v)} />

      {/* ─────────── Tweaks panel ─────────── */}
      <TweaksPanel>
        <TweakSection label="레이아웃" />
        <TweakToggle label="Nav 접기" value={t.navCollapsed}
          onChange={(v) => setTweak('navCollapsed', v)} />
        <TweakRadio label="밀도" value={t.density}
          options={['compact', 'regular']}
          onChange={(v) => setTweak('density', v)} />
        <TweakRadio label="언어" value={t.locale}
          options={['ko', 'en']}
          onChange={(v) => setTweak('locale', v)} />

        <TweakSection label="캔버스" />
        <TweakRadio label="모눈 강도" value={t.gridStrength}
          options={['subtle', 'medium', 'strong']}
          onChange={(v) => setTweak('gridStrength', v)} />
        <TweakRadio label="OR 배경" value={t.orStyle}
          options={['tinted', 'dashed']}
          onChange={(v) => setTweak('orStyle', v)} />

        <TweakSection label="A-3 전환 신호" />
        <TweakSelect label="OR 도입 확인 강도" value={t.orConfirmStrength}
          options={[
            { value: 'modal', label: 'Modal (무거움 — 운영자 기본)' },
            { value: 'none',  label: '없음 (디버그용)' },
          ]}
          onChange={(v) => setTweak('orConfirmStrength', v)} />

        <TweakSection label="시연 시드" />
        <TweakSelect label="초기 정책" value={t.seedPolicy}
          options={[
            { value: 'baseline-or', label: 'Swap baseline (OR · 중첩 → 캔버스)' },
            { value: 'flat-and',    label: 'Flat AND 예시 (폼 레이아웃)' },
          ]}
          onChange={(v) => setTweak('seedPolicy', v)} />
      </TweaksPanel>
    </>
  );
}

function countLeaves(node) {
  if (!node) return 0;
  if (node.kind === 'leaf') return 1;
  return node.children.reduce((s, c) => s + countLeaves(c), 0);
}

// Mount
const root = ReactDOM.createRoot(document.getElementById('app'));
root.render(<V2EditorApp />);
