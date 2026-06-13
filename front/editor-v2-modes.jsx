// editor-v2-modes.jsx
// Code mode, Verdict pill, Context launcher (⊕), Manifest slideover.

const { useState: useStateMD, useRef: useRefMD, useEffect: useEffectMD, useMemo: useMemoMD } = React;

// ─── Tiny Cedar tokenizer (visual-only) ─────────────────────────────────────
function tokenize(text) {
  const patterns = [
    { re: /^\/\/[^\n]*/, c: 't-cmt' },
    { re: /^"[^"]*"/,     c: 't-str' },
    { re: /^\b(forbid|permit|when|unless|principal|action|resource|has|in|like)\b/, c: 't-kw' },
    { re: /^\b(true|false)\b/, c: 't-bool' },
    { re: /^\b\d+(\.\d+)?\b/,  c: 't-num' },
    { re: /^(==|!=|<=|>=|<|>|&&|\|\||::)/, c: 't-op' },
    { re: /^\b(context|principal|action|resource)\b/, c: 't-ctx' },
    { re: /^\b[a-zA-Z_][a-zA-Z0-9_]*\b/, c: 't-id' },
    { re: /^[(){}[\],;]/, c: 't-punct' },
    { re: /^\s+/, c: 't-ws' },
    { re: /^./, c: 't-id' },
  ];
  const out = [];
  let i = 0;
  while (i < text.length) {
    const rest = text.slice(i);
    let m = false;
    for (const p of patterns) {
      const r = rest.match(p.re);
      if (r) { out.push({ t: r[0], c: p.c }); i += r[0].length; m = true; break; }
    }
    if (!m) { out.push({ t: rest[0], c: 't-id' }); i++; }
  }
  return out;
}

// ─── Code view ──────────────────────────────────────────────────────────────
function V2CodeView({ cedar, focusedLeafId, matchedLeafIds, lastChangedAt }) {
  return (
    <div className="code-view">
      <div className="code-tb">
        <span className="code-lang">CEDAR</span>
        <span className="code-tb-sep">·</span>
        <span className="code-tb-status">읽기전용 · Editor와 실시간 동기화 (200ms debounce)</span>
        <div className="code-tb-r">
          <button className="code-btn">복사</button>
          <button className="code-btn">파일 내보내기</button>
        </div>
      </div>
      <div className="code-scroll">
        {cedar.lines.map(line => {
          const focused = focusedLeafId && line.guardId === focusedLeafId;
          const matched = matchedLeafIds && matchedLeafIds.includes(line.guardId);
          const flash = lastChangedAt && line.guardId === lastChangedAt.guardId;
          return (
            <div key={line.n} className={`code-line ${focused ? 'focus' : ''} ${matched ? 'match' : ''} ${flash ? 'flash' : ''}`}>
              <span className="code-gut">{line.n}</span>
              <span className="code-text">
                {tokenize(line.text).map((tk, i) => <span key={i} className={tk.c}>{tk.t}</span>)}
              </span>
              {line.kind === 'guard' && line.guardId && (
                <span className={`code-tag ${line.custom ? 'custom' : ''}`}>{line.guardId}</span>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ─── Verdict pill (top-right of canvas) ─────────────────────────────────────
function VerdictPill({ verdict, fixtures, selectedFixtureId, onSelectFixture,
                       onFocusGuard, onToggleLauncher, launcherOpen }) {
  const [menuOpen, setMenuOpen] = useStateMD(false);
  const menuRef = useRefMD(null);
  useEffectMD(() => {
    if (!menuOpen) return;
    const h = (e) => { if (menuRef.current && !menuRef.current.contains(e.target)) setMenuOpen(false); };
    setTimeout(() => window.addEventListener('click', h), 0);
    return () => window.removeEventListener('click', h);
  }, [menuOpen]);

  const isDeny = verdict.decision.kind === 'Deny';
  const selFx = fixtures.find(f => f.id === selectedFixtureId) || fixtures[0];

  return (
    <div className={`verdict ${isDeny ? 'deny' : 'pass'}`} ref={menuRef}>
      <div className="vd-kw">{verdict.decision.kind}</div>
      <div className="vd-body">
        <div className="vd-fx">
          <span>fixture</span>
          <button className="vd-fx-sel" onClick={() => setMenuOpen(o => !o)}>
            {selFx.label}
            <V2I.caretDown style={{ width: 10, height: 10 }} />
          </button>
        </div>
        <div className="vd-trace">
          <span className="vd-trace-k">매칭:</span>
          {verdict.matchedLeafIds.length === 0 ? (
            <span className="vd-gid none">없음</span>
          ) : (
            verdict.matchedLeafIds.map(id => (
              <span key={id} className="vd-gid" onClick={() => onFocusGuard(id)}>{id}</span>
            ))
          )}
        </div>
      </div>
      <button className="vd-launch" onClick={onToggleLauncher} title="검증 도구">
        {launcherOpen ? <V2I.x style={{ width: 14, height: 14 }} /> : <span style={{ fontSize: 18, fontWeight: 300 }}>⊕</span>}
      </button>

      {menuOpen && (
        <div className="fx-menu">
          {fixtures.map(f => (
            <div key={f.id} className={`fx-menu-item ${selectedFixtureId === f.id ? 'on' : ''}`}
                 onClick={() => { onSelectFixture(f.id); setMenuOpen(false); }}>
              <span className={`fx-dot ${f.id === selectedFixtureId ? (isDeny ? 'deny' : 'pass') : 'pass'}`} />
              <span>{f.label}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ─── Context launcher (⊕) — heavy tools tucked away (B-4) ───────────────────
function ContextLauncher({ open, onClose, fixtures, selectedFixtureId, onSelectFixture,
                          tx, onReevaluate, onExportCedar }) {
  if (!open) return null;
  const fx = fixtures.find(f => f.id === selectedFixtureId) || fixtures[0];
  return (
    <div className="ctx-pop" onClick={(e) => e.stopPropagation()}>
      <div className="ctx-pop-h">검증 도구</div>
      <div className="ctx-pop-d">평결(verdict) 라인은 항상 보입니다. 무거운 도구만 여기에 모았어요.</div>
      <div className="ctx-pop-grid">
        <button className="ctx-pop-btn" onClick={onReevaluate}>
          <V2I.play className="cpb-ic" style={{ width: 14, height: 14 }} />
          <span>Re-evaluate</span>
          <span className="cpb-d">현재 fixture로 다시 평가</span>
        </button>
        <button className="ctx-pop-btn">
          <V2I.edit className="cpb-ic" style={{ width: 14, height: 14 }} />
          <span>tx 직접 편집</span>
          <span className="cpb-d">JSON으로 fixture 수정</span>
        </button>
        <button className="ctx-pop-btn" onClick={onExportCedar}>
          <V2I.code className="cpb-ic" style={{ width: 14, height: 14 }} />
          <span>Cedar 내보내기</span>
          <span className="cpb-d">현재 정책을 .cedar로</span>
        </button>
        <button className="ctx-pop-btn">
          <V2I.plus className="cpb-ic" style={{ width: 14, height: 14 }} />
          <span>fixture 추가</span>
          <span className="cpb-d">샘플 tx를 새로 등록</span>
        </button>
      </div>
      <div style={{ marginTop: 12, padding: '10px 0 0', borderTop: '1px solid var(--hairline-soft)' }}>
        <div style={{ fontFamily: 'var(--ff-mono)', fontSize: 10, fontWeight: 700, color: 'var(--slate-400)', letterSpacing: '0.06em', textTransform: 'uppercase', marginBottom: 6 }}>
          현재 tx · {fx.label}
        </div>
        <div style={{ fontFamily: 'var(--ff-mono)', fontSize: 11, color: 'var(--slate-700)', lineHeight: 1.6 }}>
          <div>from: <span style={{ color: 'var(--slate-900)' }}>{tx.from}</span></div>
          <div>recipient: <span style={{ color: 'var(--slate-900)' }}>{tx.recipient}</span></div>
          <div>swapMode: <span style={{ color: 'var(--slate-900)' }}>{tx.swapMode}</span></div>
          <div>validityDeltaSec: <span style={{ color: 'var(--slate-900)' }}>{tx.validityDeltaSec}</span></div>
          <div>recipientIsContract: <span style={{ color: 'var(--slate-900)' }}>{String(tx.recipientIsContract)}</span></div>
        </div>
      </div>
    </div>
  );
}

// ─── Manifest slideover (re-skinned per C-2/C-3/C-5) ────────────────────────
function V2ManifestSlideover({ open, onClose }) {
  const [activeTab, setActiveTab] = useStateMD('base');
  if (!open) return null;

  // Lightweight derived view of the catalog so we can render the
  // "type-aware path picker" and "produced signals preview" panels.
  return (
    <>
      <div className="so-scrim" onClick={onClose} />
      <div className="so" data-screen-label="Manifest settings · /manifests/swap">
        <div className="so-h">
          <div>
            <div className="so-eye">SWAP · MANIFEST</div>
            <div className="so-t">신호 정의 (signal contract)</div>
            <div className="so-d">
              매니페스트를 편집하면 에디터에서 쓸 수 있는 <b>신호 팔레트</b>가 바뀝니다.
              버전·마이그레이션은 이번 범위 제외 (C-6).
            </div>
          </div>
          <button className="so-x" onClick={onClose}><V2I.x style={{ width: 14, height: 14 }} /></button>
        </div>

        <div className="so-meta">
          <span className="so-meta-k">Action</span>
          <span className="so-meta-v">swap</span>
          <span className="so-meta-sep" />
          <span className="so-meta-k">Hash</span>
          <span className="so-meta-v mono">#fc20a91</span>
          <span className="so-meta-sep" />
          <span className="so-meta-k">정책 참조</span>
          <span className="so-meta-v">8개</span>
        </div>

        <div className="so-tabs">
          <button className={`so-tab ${activeTab === 'base' ? 'on' : ''}`} onClick={() => setActiveTab('base')}>
            기본 ({V2_SIGNAL_CATALOG.base.length})
          </button>
          <button className={`so-tab ${activeTab === 'requires' ? 'on' : ''}`} onClick={() => setActiveTab('requires')}>
            Requirements ({V2_SIGNAL_CATALOG.custom.length})
          </button>
          <button className={`so-tab ${activeTab === 'history' ? 'on' : ''}`} onClick={() => setActiveTab('history')}>
            변경 이력
          </button>
        </div>

        <div className="so-body">
          {activeTab === 'requires' && <RequiresTab />}
          {activeTab === 'base' && <BaseTab />}
          {activeTab === 'history' && <HistoryTab />}
        </div>

        <div className="so-foot">
          <span className="so-foot-warn">
            <V2I.warn style={{ width: 13, height: 13 }} />
            저장하면 hash가 바뀝니다 — 참조 정책 <b>8개</b>에 스큐 경고가 켜져요.
          </span>
          <span className="so-foot-spacer" />
          <button className="btn-secondary" onClick={onClose}>취소</button>
          <button className="btn-primary on">manifest 저장</button>
        </div>
      </div>
    </>
  );
}

function BaseTab() {
  return (
    <>
      <ProducedPreview />
      <div className="so-section-h">calldata에서 추출되는 기본 필드</div>
      {V2_SIGNAL_CATALOG.base.map(s => (
        <div key={s.id} className="so-out-row" style={{ padding: '10px 4px', borderBottom: '1px dashed var(--hairline-soft)' }}>
          <span className={`so-out-sw s-${s.shape}`} />
          <span>
            <div className="so-out-name">{s.label.ko}</div>
            <div className="so-out-path">context.{s.id}</div>
          </span>
          <span className="so-out-type">{s.leafType}</span>
          <span style={{ flex: 1 }} />
          {s.optional && <span className="so-req-flag opt">optional</span>}
        </div>
      ))}
    </>
  );
}

function HistoryTab() {
  const rows = [
    { h: '#fc20a91', t: '현재', who: 'Taeyun', when: '오늘 14:22', d: 'recipientIsContract enrichment 추가' },
    { h: '#fc1a83c', t: '이전', who: 'Mina',   when: '3일 전',   d: 'validityDeltaSec 단위 sec → ms 수정 후 롤백' },
    { h: '#fc14d50', t: '초기', who: 'Taeyun', when: '2주 전',   d: 'swap manifest 최초 생성 — 6 base, 4 enrichment' },
  ];
  return (
    <>
      <div className="so-section-h">매니페스트 변경 이력</div>
      {rows.map((r, i) => (
        <div key={i} style={{ display: 'flex', gap: 12, padding: '10px 4px', borderBottom: '1px dashed var(--hairline-soft)' }}>
          <span style={{ fontFamily: 'var(--ff-mono)', fontSize: 11.5, fontWeight: 700, color: 'var(--slate-700)', background: 'var(--fog-200)', padding: '2px 7px', borderRadius: 5, height: 'fit-content' }}>
            {r.h}
          </span>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12.5, color: 'var(--slate-900)', fontWeight: 600 }}>{r.d}</div>
            <div style={{ fontSize: 11, color: 'var(--slate-500)', marginTop: 2 }}>{r.who} · {r.when}</div>
          </div>
          <span style={{ fontFamily: 'var(--ff-mono)', fontSize: 10, fontWeight: 700, color: r.t === '현재' ? 'var(--pass-700)' : 'var(--slate-500)', background: r.t === '현재' ? 'var(--pass-100)' : 'var(--fog-200)', padding: '2px 7px', borderRadius: 4, height: 'fit-content', alignSelf: 'center' }}>
            {r.t}
          </span>
        </div>
      ))}
    </>
  );
}

function RequiresTab() {
  // Mock 3 requires cards to show the IA per the brief (C-2/C-3/C-4).
  const cards = [
    {
      id: 'r1',
      method: 'oracle.getDeadlineDelta',
      optional: false,
      params: [{ name: 'now', type: 'Long', value: 'block.timestamp' }],
      outputs: [{ name: 'validityDeltaSec', type: 'Long', shape: 'pill', required: true,  dashed: true }],
      pathPicker: { compat: 19, total: 19, type: 'Long' },
    },
    {
      id: 'r2',
      method: 'chain.isContractAt',
      optional: true,
      params: [{ name: 'address', type: 'Address', value: 'context.recipient' }],
      outputs: [{ name: 'recipientIsContract', type: 'Boolean', shape: 'hexagon', required: false, dashed: true }],
      pathPicker: { compat: 7, total: 12, type: 'Address' },
    },
    {
      id: 'r3',
      method: 'oracle.tokenSpotPriceUsd',
      optional: true,
      params: [
        { name: 'token',  type: 'Address', value: 'context.inputToken.asset.address' },
        { name: 'amount', type: 'Long',    value: 'context.inputAmount' },
      ],
      outputs: [
        { name: 'totalInputUsd',                    type: 'Decimal', shape: 'pill', required: false, dashed: true },
        { name: 'totalInputFractionOfPortfolioBps', type: 'Long',    shape: 'pill', required: false, dashed: true },
      ],
      pathPicker: { compat: 4, total: 12, type: 'Decimal' },
    },
  ];

  return (
    <>
      <ProducedPreview />
      <div className="so-section-h">Requirements — 외부 조회 (oracle · portfolio · chain · clock · stat_window)</div>
      {cards.map(c => <RequireCard key={c.id} card={c} />)}
      <button style={{
        appearance: 'none', background: 'transparent', border: '1px dashed var(--slate-300)',
        color: 'var(--slate-700)', padding: '10px 14px', borderRadius: 9, width: '100%',
        fontWeight: 600, fontSize: 12.5, marginTop: 8,
      }}>
        + Requirement 추가
      </button>
    </>
  );
}

function RequireCard({ card }) {
  return (
    <div className="so-req">
      <div className="so-req-h">
        <span className="so-req-id">{card.id}</span>
        <span className="so-req-method">{card.method}</span>
        <span className="so-req-spacer" />
        <span className={`so-req-flag ${card.optional ? 'opt' : ''}`}>
          요구 레벨 · {card.optional ? 'optional' : 'required'}
          <span className="so-req-flag-d">
            {card.optional ? '데이터 없으면 이 요구 자체를 건너뜀' : '실패 시 정책 평가 전체가 중단'}
          </span>
        </span>
      </div>

      <div className="so-req-body" style={{ borderTop: '1px dashed var(--hairline-soft)', paddingTop: 10 }}>
        <div>
          <div className="so-req-col-t">params · 입력</div>
          {card.params.map(p => (
            <div key={p.name} style={{ marginBottom: 8 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
                <span style={{ fontFamily: 'var(--ff-mono)', fontSize: 11.5, color: 'var(--slate-900)', fontWeight: 600 }}>{p.name}</span>
                <span className="so-out-type">{p.type}</span>
              </div>
              <div className="so-path-picker">
                <span style={{ color: 'var(--slate-700)' }}>{p.value}</span>
                <span className="so-pp-cnt">{card.pathPicker.compat}/{card.pathPicker.total} paths · {p.type}</span>
              </div>
            </div>
          ))}
        </div>

        <div>
          <div className="so-req-col-t">outputs · 산출</div>
          {card.outputs.map(o => (
            <div key={o.name} style={{ marginBottom: 6 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <span className={`so-out-sw s-${o.shape} ${o.dashed ? 'dashed' : ''}`} />
                <span style={{ fontFamily: 'var(--ff-mono)', fontSize: 11.5, color: 'var(--slate-900)', fontWeight: 600 }}>{o.name}</span>
                <span className="so-out-type">{o.type}</span>
              </div>
              <div style={{ marginTop: 4 }}>
                <span className={`so-req-flag ${o.required ? 'req' : 'opt'}`} style={{ display: 'inline-flex' }}>
                  출력 레벨 · {o.required ? 'required' : 'optional'}
                  <span className="so-req-flag-d">
                    {o.required ? '이 출력 없으면 전체 실패' : '데이터 없으면 이 검사는 건너뜀'}
                  </span>
                </span>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function ProducedPreview() {
  const chips = [...V2_SIGNAL_CATALOG.base.slice(0, 4), ...V2_SIGNAL_CATALOG.custom.slice(0, 4)];
  return (
    <div className="so-preview">
      <div className="so-preview-h">
        <span className="so-pv-t">이 매니페스트가 만드는 신호</span>
        <span className="so-pv-d">에디터 팔레트에 즉시 반영</span>
        <span className="so-pv-live">live</span>
      </div>
      <div className="so-preview-chips">
        {chips.map(s => (
          <span key={s.id} className="so-pv-chip">
            <span>{s.label.ko}</span>
            <span className="so-pv-chip-t">{s.leafType}</span>
          </span>
        ))}
        <span className="so-pv-chip" style={{ borderStyle: 'solid', borderColor: 'var(--slate-300)', color: 'var(--slate-500)' }}>
          +{(V2_SIGNAL_CATALOG.base.length + V2_SIGNAL_CATALOG.custom.length) - chips.length}개 더
        </span>
      </div>
    </div>
  );
}

Object.assign(window, { V2CodeView, VerdictPill, ContextLauncher, V2ManifestSlideover, tokenize });
