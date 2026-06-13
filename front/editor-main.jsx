// editor-main.jsx
// App composition — wires shell, three modes, test pane, slideover, tweaks.

const { useState: useStateMain, useEffect: useEffectMain, useMemo: useMemoMain } = React;

const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "mode": "block",
  "density": "regular",
  "locale": "ko",
  "colorScheme": "io-balanced",
  "orStyle": "tinted",
  "paneRatio": "18-55-27",
  "modeToggle": "segment",
  "showSkew": false,
  "showCedar": true,
  "showManifest": false
}/*EDITMODE-END*/;

function EditorApp() {
  const [t, setTweak] = useTweaks(TWEAK_DEFAULTS);

  const [focusedGuard, setFocusedGuard] = useStateMain(null);
  const [selectedFixture, setSelectedFixture] = useStateMain('fx2');
  const [cedarCollapsed, setCedarCollapsed] = useStateMain(false);
  const [codeEditable, setCodeEditable] = useStateMain(false);

  const colorScheme = COLOR_SCHEMES[t.colorScheme] || COLOR_SCHEMES['io-balanced'];

  const fixture = TEST_FIXTURES.find(f => f.id === selectedFixture) || TEST_FIXTURES[0];
  const matchedGuards = fixture.matches;

  const skewedGuard = t.showSkew ? 'g3' : null;

  // Set pane ratio CSS variables from tweak
  const paneRatios = {
    '18-55-27': ['18%', '55%', '27%'],
    '14-60-26': ['14%', '60%', '26%'],
    'test-toggle': ['18%', '82%', '0%'],
  };
  const [pL, pM, pR] = paneRatios[t.paneRatio] || paneRatios['18-55-27'];

  // Effective mode (can be forced from tweak debug)
  const mode = t.mode;

  return (
    <div className={`editor-root density-${t.density} locale-${t.locale}`}
         style={{
           '--pane-left': pL,
           '--pane-mid': pM,
           '--pane-right': pR,
         }}>
      <EditorTopbar
        mode={mode}
        onModeChange={(m) => setTweak('mode', m)}
        dirty={true}
        undoEnabled={true}
        locale={t.locale}
        onTriggerSave={() => {}}
      />

      <ContextBar policy={BASELINE_POLICY}
                  onOpenManifest={() => setTweak('showManifest', true)} />

      {t.showSkew && (
        <div className="skew-banner">
          <I.warn style={{ width: 14, height: 14 }} />
          <div className="skew-b-body">
            <div className="skew-b-t"><b>신호 정의가 변경되었습니다.</b> 이 정책은 이전 manifest <span className="mono">#fc20a91</span> 기준입니다. 현재 <span className="mono">#fc214d3</span>과 다릅니다.</div>
            <div className="skew-b-d">영향을 받는 가드: <span className="mono">g3</span> · <span className="mono">validityDeltaSec</span></div>
          </div>
          <button className="skew-b-cta">변경 확인 →</button>
          <button className="skew-b-cta-2">새 hash로 갱신</button>
        </div>
      )}

      <div className="editor-3pane" data-screen-label={`Editor · ${mode} mode`}>
        {/* Left palette — only in Block mode */}
        <div className="pane-left">
          {mode === 'block' ? (
            <BlockPalette colorScheme={colorScheme} locale={t.locale} />
          ) : (
            <div className="pane-left-empty">
              <div className="ple-icon"><I.block style={{ width: 22, height: 22, color: 'var(--slate-300)' }} /></div>
              <div className="ple-t">Block 모드 전용</div>
              <div className="ple-d">팔레트는 Block 모드에서만 사용합니다. 현재 모드의 편집 UI는 가운데 패널에 있습니다.</div>
              <button className="ple-cta" onClick={() => setTweak('mode', 'block')}>Block 모드로 →</button>
            </div>
          )}
        </div>

        {/* Center — mode-specific editing surface */}
        <div className="pane-mid">
          <div className="pane-mid-scroll">
            {mode === 'block' && (
              <BlockCanvas
                policy={BASELINE_POLICY}
                colorScheme={colorScheme}
                locale={t.locale}
                orStyle={t.orStyle}
                skewedGuard={skewedGuard}
                matchedGuards={matchedGuards}
                focusedGuard={focusedGuard}
                onFocusGuard={setFocusedGuard}
              />
            )}
            {mode === 'builder' && (
              <BuilderView
                policy={BASELINE_POLICY}
                colorScheme={colorScheme}
                locale={t.locale}
                focusedGuard={focusedGuard}
                onFocusGuard={setFocusedGuard}
                matchedGuards={matchedGuards}
                skewedGuard={skewedGuard}
              />
            )}
            {mode === 'code' && (
              <CodeView
                policy={BASELINE_POLICY}
                focusedGuard={focusedGuard}
                matchedGuards={matchedGuards}
                skewedGuard={skewedGuard}
                editable={codeEditable}
                onToggleEditable={setCodeEditable}
              />
            )}
          </div>

          {t.showCedar && mode !== 'code' && (
            <CedarPreview
              collapsed={cedarCollapsed}
              onToggle={() => setCedarCollapsed(c => !c)}
              highlightGuard={focusedGuard}
            />
          )}

          <SaveBar dirty={true} decision={BASELINE_POLICY.decision} onSave={() => {}} />
        </div>

        {/* Right — Policy Test pane */}
        {pR !== '0%' && (
          <div className="pane-right">
            <TestPane
              fixtures={TEST_FIXTURES}
              selectedFixture={selectedFixture}
              onSelectFixture={setSelectedFixture}
              policy={BASELINE_POLICY}
              colorScheme={colorScheme}
            />
          </div>
        )}
      </div>

      <ManifestSlideover open={t.showManifest} onClose={() => setTweak('showManifest', false)} />

      {/* ── Tweaks panel ────────────────────────────────────────────── */}
      <TweaksPanel>
        <TweakSection label="Layout" />
        <TweakRadio label="Density" value={t.density}
          options={['compact', 'regular']}
          onChange={(v) => setTweak('density', v)} />
        <TweakRadio label="Locale" value={t.locale}
          options={['ko', 'en', 'mix']}
          onChange={(v) => setTweak('locale', v)} />
        <TweakSelect label="3-pane ratio" value={t.paneRatio}
          options={[
            { value: '18-55-27', label: '18 / 55 / 27 (브리프 기본)' },
            { value: '14-60-26', label: '14 / 60 / 26 (좌측 더 좁게)' },
            { value: 'test-toggle', label: 'Test pane 토글 (숨김)' },
          ]}
          onChange={(v) => setTweak('paneRatio', v)} />

        <TweakSection label="Modes & visuals" />
        <TweakSelect label="Mode (debug 강제)" value={t.mode}
          options={[
            { value: 'block', label: 'Block · 시각' },
            { value: 'builder', label: 'Builder · 폼' },
            { value: 'code', label: 'Code · Cedar' },
          ]}
          onChange={(v) => setTweak('mode', v)} />
        <TweakSelect label="Block 색 배정" value={t.colorScheme}
          options={[
            { value: 'io-balanced', label: 'I/O 균형 (in=Sage · out·param=Slate · meta=Cyan)' },
            { value: 'calldata-vs-meta', label: 'Calldata vs Meta (cd=Sage · ctrl=Slate · enrich=Cyan)' },
            { value: 'asset-flow', label: 'Asset → Destination (asset=Sage · recv=Slate · param=Cyan)' },
          ]}
          onChange={(v) => setTweak('colorScheme', v)} />
        <TweakRadio label="OR 컨테이너 배경" value={t.orStyle}
          options={['tinted', 'dashed']}
          onChange={(v) => setTweak('orStyle', v)} />
        <TweakToggle label="Cedar live preview" value={t.showCedar}
          onChange={(v) => setTweak('showCedar', v)} />

        <TweakSection label="상태 시뮬레이션" />
        <TweakToggle label="신호 스큐 (g3 unknown_field)" value={t.showSkew}
          onChange={(v) => setTweak('showSkew', v)} />
        <TweakToggle label="Manifest slideover 열기" value={t.showManifest}
          onChange={(v) => setTweak('showManifest', v)} />
      </TweaksPanel>
    </div>
  );
}

// Mount
const root = ReactDOM.createRoot(document.getElementById('app'));
root.render(<EditorApp />);
