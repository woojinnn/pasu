// editor-options.jsx
// Side-by-side variation explorer for the Editor design — uses design_canvas.

const { useState: useStateOpts } = React;

// ─── Mini block primitive ───────────────────────────────────────────────────
// Same visual rules as the real GuardBlock but smaller for canvas comparison.
function MiniBlock({ color, shape, dashed, segments, op, val, note, skew }) {
  const fill = `var(--${color}-100)`;
  const stroke = `var(--${color}-${color === 'slate' ? '400' : '500'})`;
  const paths = {
    pill: 'M20 1 H80 A19 19 0 0 1 80 39 H20 A19 19 0 0 1 20 1 Z',
    rect: 'M3 1 H97 A2 2 0 0 1 99 3 V37 A2 2 0 0 1 97 39 H3 A2 2 0 0 1 1 37 V3 A2 2 0 0 1 3 1 Z',
    hexagon: 'M10 1 H90 L99 20 L90 39 H10 L1 20 Z',
  };
  return (
    <div className="mini-bk-wrap">
      <div className="mini-bk" style={{ position: 'relative' }}>
        {skew && <span className="skew-pill-flag" style={{ position: 'absolute', top: -7, left: 8, zIndex: 2 }}>⚠ skew</span>}
        <svg className="mbg" viewBox="0 0 100 40" preserveAspectRatio="none">
          <path d={paths[shape] || paths.rect} fill={fill} stroke={stroke} strokeWidth="1.4"
                strokeDasharray={dashed ? '5 4' : null} vectorEffect="non-scaling-stroke" />
        </svg>
        <div className="mb-body">
          {segments.map((s, i) => (
            <span key={i} className={`mb-seg ${dashed && i === segments.length - 1 ? 'dashed' : ''}`}>
              {s}<span style={{ color: 'var(--slate-400)', marginLeft: 2 }}>▾</span>
            </span>
          ))}
          <span className="mb-op">{op}</span>
          <span className="mb-val">{val}</span>
        </div>
      </div>
      {note && <span className="mini-bk-note">{note}</span>}
    </div>
  );
}

// ─── Baseline guards as visual primitives (shared across artboards) ─────────
// We render them differently per color scheme below.
const GUARDS_VISUAL = [
  { id: 'g1', segments: ['Recipient'],          op: '!=', val: 'root.from', shape: 'rect',    dashed: false, dom: 'recipient',          note: 'swap-and-send' },
  { id: 'g2', segments: ['Swap dir.'],          op: '==', val: 'market',    shape: 'hexagon', dashed: false, dom: 'swapMode',           note: '시장가 차단' },
  { id: 'g3', segments: ['만료 (sec)'],          op: '<',  val: '30',        shape: 'pill',    dashed: true,  dom: 'validityDeltaSec',   note: '만료 임박' },
  { id: 'g4', segments: ['수신자 컨트랙트'],     op: '==', val: 'true',      shape: 'hexagon', dashed: true,  dom: 'recipientIsContract',note: '컨트랙트 수신자' },
];

const COLOR_FOR = (schemeId, dom) => COLOR_SCHEMES[schemeId].map[dom] || 'cyan';

// Compact label dictionary for domain names in the legend.
const DOM_LABELS = {
  inputToken: 'Input', inputAmountNano: 'Input amount',
  outputToken: 'Output', outputAmountNano: 'Output amount',
  recipient: 'Recipient', recipientIsContract: '수신자=컨트랙트',
  swapMode: 'Swap dir.', feeBps: 'Fee', validity: 'Validity',
  validityDeltaSec: '만료 (sec)', effectiveRateVsOracleBps: 'Slippage',
  totalInputUsd: 'Input USD', totalMinOutputUsd: 'Min out USD',
  totalInputFractionOfPortfolioBps: 'Input/Portfolio', windowStats: '24h stats',
};
function domainsByColor(scheme, color) {
  return Object.entries(scheme.map)
    .filter(([, v]) => v === color)
    .map(([k]) => DOM_LABELS[k] || k)
    .join(' · ');
}

// ─── Variation artboards ────────────────────────────────────────────────────

// A1–A3 · 3-pane ratio
function ABThreePane({ ratio, label, accent }) {
  const [pl, pm, pr] = ratio.split('-').map(Number);
  const total = pl + pm + pr;
  const cols = `${pl}fr ${pm}fr ${pr || 0.001}fr`;
  return (
    <div className="ab">
      <div className="ab-h">
        <div className="ab-h-t">{label}</div>
        <div className="ab-h-s">{ratio.replace(/-/g, ' / ')}</div>
        {accent && <div className="ab-h-tag">{accent}</div>}
      </div>
      <div className="ab-body">
        <div className="mini-3" style={{ gridTemplateColumns: cols }}>
          <div className="mp">
            <div className="mp-t">{pl < 16 ? '팔레트 (좁음)' : '팔레트'}</div>
            <div className="mp-rows">
              <div className="mp-row"><span className="mp-pill mp-pill-sage" /></div>
              <div className="mp-row"><span className="mp-pill mp-pill-slate" /></div>
              <div className="mp-row"><span className="mp-pill mp-pill-cyan" /></div>
              <div className="mp-row"><span className="mp-pill mp-pill-cyan" /></div>
              <div className="mp-row"><span className="mp-pill mp-pill-sage" /></div>
              <div className="mp-row"><span className="mp-pill mp-pill-slate" /></div>
            </div>
          </div>
          <div className="mp mp-mid">
            <div className="mp-t">캔버스 · OR · 4 guards</div>
            <div style={{ background: 'var(--slate-50)', border: '1px solid var(--slate-200)', borderRadius: 8, padding: 8, display: 'flex', flexDirection: 'column', gap: 6 }}>
              {GUARDS_VISUAL.map(g => (
                <MiniBlock key={g.id} {...g} color={COLOR_FOR('io-balanced', g.dom)} />
              ))}
            </div>
          </div>
          {pr ? (
            <div className="mp">
              <div className="mp-t">Test pane</div>
              <div style={{ background: 'var(--fail-50)', border: '1px solid var(--fail-200)', borderRadius: 6, padding: 6, marginBottom: 6 }}>
                <div style={{ fontSize: 10, fontWeight: 700, color: 'var(--fail-700)' }}>Deny · FAIL</div>
                <div style={{ fontSize: 10, color: 'var(--slate-700)', fontStyle: 'italic' }}>swap baseline violated</div>
              </div>
              <div className="mp-rows">
                <div className="mp-row" style={{ fontSize: 10, color: 'var(--slate-500)' }}>swapMode · market</div>
                <div className="mp-row" style={{ fontSize: 10, color: 'var(--slate-500)' }}>validity · 18s</div>
              </div>
            </div>
          ) : (
            <div className="mp" style={{ background: 'var(--fog-300)', borderStyle: 'dashed', display: 'grid', placeItems: 'center', textAlign: 'center', padding: 8 }}>
              <div>
                <div style={{ fontSize: 10, fontWeight: 700, color: 'var(--slate-700)', marginBottom: 4 }}>Test pane</div>
                <div style={{ fontSize: 10, color: 'var(--slate-500)' }}>토글식 — 평소 숨김</div>
                <button style={{ marginTop: 8, appearance: 'none', border: '1px solid var(--slate-300)', background: 'var(--surface)', padding: '3px 8px', borderRadius: 999, fontSize: 10, color: 'var(--slate-700)', fontWeight: 600 }}>▸ 열기</button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// B1–B3 · mode toggle styles
function ABModeToggle({ kind }) {
  if (kind === 'segment') {
    return (
      <div className="ab">
        <div className="ab-h"><div className="ab-h-t">Segment · 현재</div><div className="ab-h-s">집중도 ↑, dark-on-light contrast</div><div className="ab-h-tag">A</div></div>
        <div className="ab-body">
          <div className="var-toggle-row">
            <span className="vt-cap">상단 중앙 segment control</span>
            <div className="vt-row">
              <div className="mode-toggle" style={{ transform: 'scale(1)' }}>
                <button className="mode-tab on"><span className="mt-lbl">Block</span> <span className="mt-sub">시각</span></button>
                <button className="mode-tab"><span className="mt-lbl">Builder</span> <span className="mt-sub">폼</span></button>
                <button className="mode-tab"><span className="mt-lbl">Code</span> <span className="mt-sub">Cedar</span></button>
              </div>
            </div>
            <div className="pros-cons">
              <div className="pro">+ 활성 모드 강조 명확 (slate-700 fill)</div>
              <div className="con">− 폭 차지, topbar 공간 필요</div>
            </div>
          </div>
        </div>
      </div>
    );
  }
  if (kind === 'tabs') {
    return (
      <div className="ab">
        <div className="ab-h"><div className="ab-h-t">Tab · 하단 underline</div><div className="ab-h-s">에디터 헤더 하단에 붙음</div><div className="ab-h-tag">B</div></div>
        <div className="ab-body">
          <div className="var-toggle-row">
            <span className="vt-cap">canvas 영역 상단에 inline tab</span>
            <div className="vt-tabs">
              <button className="on">Block <span style={{ fontFamily: 'var(--ff-mono)', fontSize: 10, color: 'var(--slate-400)', marginLeft: 4 }}>시각</span></button>
              <button>Builder</button>
              <button>Code</button>
            </div>
            <div className="pros-cons">
              <div className="pro">+ topbar 가벼움, 컨텍스트 가까이</div>
              <div className="con">− 활성 신호 약함 (underline만)</div>
            </div>
          </div>
        </div>
      </div>
    );
  }
  return (
    <div className="ab">
      <div className="ab-h"><div className="ab-h-t">Side · 좌측 세그먼트</div><div className="ab-h-s">palette 위에 모드 picker</div><div className="ab-h-tag">C</div></div>
      <div className="ab-body">
        <div className="var-toggle-row">
          <span className="vt-cap">좌측 18% 상단에 세로 segment</span>
          <div className="vt-row">
            <div className="vt-side">
              <button className="on"><span className="dot" />Block</button>
              <button><span className="dot" />Builder</button>
              <button><span className="dot" />Code</button>
            </div>
            <div style={{ fontSize: 11.5, color: 'var(--slate-600)', flex: 1 }}>
              palette와 시각적으로 같은 영역 → palette ↔ block 매칭이 직관적. 단 Builder/Code일 땐 palette가 비어서 어색.
            </div>
          </div>
          <div className="pros-cons">
            <div className="pro">+ Block 모드와 palette 일체감</div>
            <div className="con">− 다른 모드에선 좌측이 어색</div>
          </div>
        </div>
      </div>
    </div>
  );
}

// C1–C3 · color scheme
function ABColorScheme({ schemeId }) {
  const scheme = COLOR_SCHEMES[schemeId];
  return (
    <div className="ab">
      <div className="ab-h">
        <div className="ab-h-t">{scheme.label}</div>
        <div className="ab-h-s">{scheme.desc}</div>
        <div className="ab-h-tag">{schemeId}</div>
      </div>
      <div className="ab-body">
        <div className="mini-canvas">
          <div className="mc-h">
            <span style={{ color: 'var(--slate-500)', fontSize: 10, textTransform: 'uppercase', letterSpacing: 0.06 }}>on action</span>
            <span style={{ color: 'var(--slate-300)' }}>=</span>
            <span className="mc-mono">swap</span>
          </div>
          <div className="mini-ctn or-tinted">
            <span className="mc-badge">OR · 하나라도 참</span>
            {GUARDS_VISUAL.map(g => (
              <MiniBlock key={g.id} {...g} color={COLOR_FOR(schemeId, g.dom)} />
            ))}
          </div>
          <div style={{ display: 'inline-flex', alignSelf: 'flex-start', gap: 8, background: 'var(--slate-700)', color: 'var(--fog-50)', padding: '4px 10px', borderRadius: 999, fontSize: 10.5 }}>
            <span style={{ background: 'var(--fail-500)', padding: '1px 7px', borderRadius: 999, fontWeight: 700 }}>Deny</span>
            <span style={{ fontStyle: 'italic' }}>"swap baseline violated"</span>
          </div>
        </div>
        <div className="dom-legend">
          <div className="dom-legend-row">
            <span className="dom-legend-sw sage" />
            <span className="dom-legend-k">Sage</span>
            <span className="dom-legend-v">{domainsByColor(scheme, 'sage') || '—'}</span>
          </div>
          <div className="dom-legend-row">
            <span className="dom-legend-sw slate" />
            <span className="dom-legend-k">Slate</span>
            <span className="dom-legend-v">{domainsByColor(scheme, 'slate') || '—'}</span>
          </div>
          <div className="dom-legend-row">
            <span className="dom-legend-sw cyan" />
            <span className="dom-legend-k">Cyan</span>
            <span className="dom-legend-v">{domainsByColor(scheme, 'cyan') || '—'}</span>
          </div>
        </div>
      </div>
    </div>
  );
}

// D1–D2 · cascade dropdown
function ABCascade({ kind }) {
  if (kind === 'separated') {
    return (
      <div className="ab">
        <div className="ab-h"><div className="ab-h-t">각 세그먼트 분리</div><div className="ab-h-s">개별 dropdown · 현재 안</div><div className="ab-h-tag">A</div></div>
        <div className="ab-body">
          <div className="casc-row">
            <span className="cs">Input <span className="ca">▾</span></span>
            <span className="cs">Amount <span className="ca">▾</span></span>
            <span className="cs dashed">Value <span className="ca">▾</span></span>
            <span style={{ flex: 1 }} />
            <span style={{ fontFamily: 'var(--ff-mono)', fontSize: 10, color: 'var(--slate-400)' }}>{`input.amount.value`}</span>
          </div>
          <div className="casc-row">
            <span className="cs">Validity <span className="ca">▾</span></span>
            <span className="cs">Deadline <span className="ca">▾</span></span>
          </div>
          <div className="pros-cons">
            <div className="pro">+ 각 단계에서 옵션 보임. 발견성 ↑</div>
            <div className="con">− 가로 폭 ↑. 한국어 라벨 길면 wrap</div>
          </div>
        </div>
      </div>
    );
  }
  return (
    <div className="ab">
      <div className="ab-h"><div className="ab-h-t">Breadcrumb 한 줄</div><div className="ab-h-s">선택된 경로 inline · 클릭 시 dropdown</div><div className="ab-h-tag">B</div></div>
      <div className="ab-body">
        <div className="casc-bread">
          <span className="crumb-piece">Input</span>
          <span className="sep-tri">›</span>
          <span className="crumb-piece">Amount</span>
          <span className="sep-tri">›</span>
          <span className="crumb-piece tail dashed">Value</span>
          <span style={{ marginLeft: 'auto', fontFamily: 'var(--ff-mono)', fontSize: 10, color: 'var(--slate-400)' }}>enrichment</span>
        </div>
        <div className="casc-bread">
          <span className="crumb-piece">Validity</span>
          <span className="sep-tri">›</span>
          <span className="crumb-piece tail">Deadline</span>
        </div>
        <div className="pros-cons">
          <div className="pro">+ 컴팩트, 정렬 안정적, 점선 path 표시 자연스러움</div>
          <div className="con">− 현재 단계 변경하려면 dropdown 열어야</div>
        </div>
      </div>
    </div>
  );
}

// E1–E2 · OR container background
function ABOrBg({ kind }) {
  return (
    <div className="ab">
      <div className="ab-h"><div className="ab-h-t">{kind === 'tinted' ? 'Slate 50 tinted (현재)' : 'No-fill dashed'}</div><div className="ab-h-s">{kind === 'tinted' ? 'OR 영역 면적감 ↑' : '면적 없음, hairline만'}</div><div className="ab-h-tag">{kind === 'tinted' ? 'A' : 'B'}</div></div>
      <div className="ab-body">
        <div className="mini-canvas">
          <div className={`mini-ctn ${kind === 'tinted' ? 'or-tinted' : 'or-dashed'}`}>
            <span className="mc-badge">OR · 하나라도 참</span>
            {GUARDS_VISUAL.map(g => (
              <MiniBlock key={g.id} {...g} color={COLOR_FOR('io-balanced', g.dom)} />
            ))}
          </div>
        </div>
        <div className="ab-note">
          {kind === 'tinted'
            ? <><b>Slate 50 tinted</b>: OR 묶음이 시각적으로 한 단위. 중첩 시 안쪽 OR도 같은 fill 반복되면 평평해질 수 있어 안쪽은 Slate 100으로 한 단 올리는 변형 고려.</>
            : <><b>No-fill dashed</b>: 블록 색이 더 잘 살아남. 단 중첩 시 dashed가 여러 단 겹치면 어수선해짐.</>}
        </div>
      </div>
    </div>
  );
}

// F1–F2 · skew warning
function ABSkew({ kind }) {
  if (kind === 'block-flag') {
    return (
      <div className="ab">
        <div className="ab-h"><div className="ab-h-t">Block 좌상단 skew flag</div><div className="ab-h-s">영향받은 가드에만</div><div className="ab-h-tag">A</div></div>
        <div className="ab-body">
          <div className="mini-canvas">
            <div className="mini-ctn or-tinted">
              <span className="mc-badge">OR · 하나라도 참</span>
              <MiniBlock {...GUARDS_VISUAL[0]} color="cyan" />
              <MiniBlock {...GUARDS_VISUAL[1]} color="slate" />
              <MiniBlock {...GUARDS_VISUAL[2]} color="slate" skew />
              <MiniBlock {...GUARDS_VISUAL[3]} color="cyan" />
            </div>
          </div>
          <div className="ab-note">
            <b>Block-level skew flag</b>: 신호가 사라진/바뀐 가드만 좌상단 skew pill + Fail 테두리. 빠른 시각적 발견.
          </div>
        </div>
      </div>
    );
  }
  return (
    <div className="ab">
      <div className="ab-h"><div className="ab-h-t">상단 page banner</div><div className="ab-h-s">전체 정책 영향 요약</div><div className="ab-h-tag">B</div></div>
      <div className="ab-body">
        <div className="skew-banner-2">
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style={{ color: 'var(--fail-700)', fontWeight: 700 }}>⚠ 신호 정의가 변경되었습니다</span>
          </div>
          <div style={{ marginTop: 4, color: 'var(--slate-700)' }}>
            이 정책은 <span style={{ fontFamily: 'var(--ff-mono)' }}>#fc20a91</span> 기준 · 현재 <span style={{ fontFamily: 'var(--ff-mono)' }}>#fc214d3</span>과 다름.
          </div>
          <div style={{ marginTop: 4, color: 'var(--slate-600)', fontSize: 11 }}>
            영향 받는 가드 <b style={{ color: 'var(--slate-900)' }}>g3</b> · <span style={{ fontFamily: 'var(--ff-mono)' }}>validityDeltaSec</span>
          </div>
          <div style={{ display: 'flex', gap: 6, marginTop: 8 }}>
            <button style={{ appearance: 'none', border: '1px solid var(--fail-300)', background: 'var(--surface)', color: 'var(--fail-700)', padding: '4px 10px', borderRadius: 999, fontSize: 11, fontWeight: 600 }}>변경 확인 →</button>
            <button style={{ appearance: 'none', border: 0, background: 'var(--fail-700)', color: 'var(--fog-50)', padding: '4px 10px', borderRadius: 999, fontSize: 11, fontWeight: 600 }}>새 hash로 갱신</button>
          </div>
        </div>
        <div className="ab-note">
          <b>Page banner</b>: 전체 맥락 + 액션. Block flag와 <b>같이 사용</b>: 배너 = 페이지 진입 시 알림 · flag = 블록 위치 핀포인트.
        </div>
      </div>
    </div>
  );
}

// ─── Compose canvas ─────────────────────────────────────────────────────────
function OptionsApp() {
  return (
    <DesignCanvas
      title="Scopeball · Editor Options"
      subtitle="브리프 §5.2 + handoff §9. 변경 가능한 6개 축 — 인터랙티브 Editor는 별도 (Editor.html)"
    >
      <DCSection id="pane-ratio" title="① 3-pane 비율" subtitle="palette / canvas / test 너비 분배">
        <DCArtboard id="r1" label="A · 18/55/27 (브리프 기본)"  width={580} height={460}>
          <ABThreePane ratio="18-55-27" label="18 / 55 / 27" accent="기본" />
        </DCArtboard>
        <DCArtboard id="r2" label="B · 14/60/26 (좌측 좁게)"  width={580} height={460}>
          <ABThreePane ratio="14-60-26" label="14 / 60 / 26" accent="좌측 ↓" />
        </DCArtboard>
        <DCArtboard id="r3" label="C · Test 토글 18/82" width={580} height={460}>
          <ABThreePane ratio="18-82-0" label="18 / 82 · test 토글" accent="test 숨김" />
        </DCArtboard>
      </DCSection>

      <DCSection id="mode-toggle" title="② 모드 토글 모양" subtitle="Block / Builder / Code 전환">
        <DCArtboard id="t1" label="A · Segment (현재)" width={560} height={340}>
          <ABModeToggle kind="segment" />
        </DCArtboard>
        <DCArtboard id="t2" label="B · Tab underline" width={560} height={340}>
          <ABModeToggle kind="tabs" />
        </DCArtboard>
        <DCArtboard id="t3" label="C · Side segment" width={560} height={340}>
          <ABModeToggle kind="side" />
        </DCArtboard>
      </DCSection>

      <DCSection id="color-scheme" title="③ 블록 색 배정 (6 도메인 → Sage/Slate/Cyan)" subtitle="status 색 금지. 캐스케이드 바뀌어도 top 도메인 색 고정.">
        <DCArtboard id="c1" label="A · I/O 균형" width={580} height={460}>
          <ABColorScheme schemeId="io-balanced" />
        </DCArtboard>
        <DCArtboard id="c2" label="B · Calldata vs Meta" width={580} height={460}>
          <ABColorScheme schemeId="calldata-vs-meta" />
        </DCArtboard>
        <DCArtboard id="c3" label="C · Asset → Destination" width={580} height={460}>
          <ABColorScheme schemeId="asset-flow" />
        </DCArtboard>
      </DCSection>

      <DCSection id="cascade" title="④ 캐스케이드 드롭다운 표현" subtitle="`Input → Amount → Value` 같은 경로 표시 방식">
        <DCArtboard id="d1" label="A · 분리 dropdown" width={680} height={300}>
          <ABCascade kind="separated" />
        </DCArtboard>
        <DCArtboard id="d2" label="B · Breadcrumb 한 줄" width={680} height={300}>
          <ABCascade kind="breadcrumb" />
        </DCArtboard>
      </DCSection>

      <DCSection id="or-bg" title="⑤ OR 컨테이너 배경" subtitle="필드 간 OR 묶음의 시각적 단위감 — 디자이너 재량 (handoff §8.질문2)">
        <DCArtboard id="o1" label="A · Slate 50 tinted" width={540} height={420}>
          <ABOrBg kind="tinted" />
        </DCArtboard>
        <DCArtboard id="o2" label="B · No-fill dashed" width={540} height={420}>
          <ABOrBg kind="dashed" />
        </DCArtboard>
      </DCSection>

      <DCSection id="skew" title="⑥ 신호 스큐(unknown_field) 경고 표현" subtitle="§9.8-1 신규 상태 — Block flag와 Page banner는 같이 사용 권장">
        <DCArtboard id="s1" label="A · Block 좌상단 flag" width={540} height={400}>
          <ABSkew kind="block-flag" />
        </DCArtboard>
        <DCArtboard id="s2" label="B · 상단 page banner" width={540} height={400}>
          <ABSkew kind="banner" />
        </DCArtboard>
      </DCSection>
    </DesignCanvas>
  );
}

ReactDOM.createRoot(document.getElementById('app')).render(<OptionsApp />);
