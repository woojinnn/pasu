// editor-v3-panels.jsx — Policy test panel, Cedar bottom strip, Code mode, Manifest slideover

const { useState: useSP, useEffect: useEP, useMemo: useMP } = React;

// ─────────── Cedar tokenizer (shared) ───────────
function tok3(text) {
  const pats = [
    { re: /^\/\/[^\n]*/, c: 't-cmt' }, { re: /^"[^"]*"/, c: 't-str' },
    { re: /^\b(forbid|permit|when|unless|principal|action|resource|has|in|like)\b/, c: 't-kw' },
    { re: /^\b(true|false)\b/, c: 't-bool' }, { re: /^\b\d+(\.\d+)?\b/, c: 't-num' },
    { re: /^(==|!=|<=|>=|<|>|&&|\|\||::)/, c: 't-op' },
    { re: /^\b(context|principal|action|resource)\b/, c: 't-ctx' },
    { re: /^\b[a-zA-Z_][a-zA-Z0-9_]*\b/, c: 't-id' },
    { re: /^[(){}[\],;]/, c: 't-punct' }, { re: /^\s+/, c: 't-ws' }, { re: /^./, c: 't-id' },
  ];
  const out = []; let i = 0;
  while (i < text.length) {
    const rest = text.slice(i); let m = false;
    for (const p of pats) { const r = rest.match(p.re); if (r) { out.push({ t: r[0], c: p.c }); i += r[0].length; m = true; break; } }
    if (!m) { out.push({ t: rest[0], c: 't-id' }); i++; }
  }
  return out;
}

// ─────────── POLICY TEST PANEL (right, collapsible) ───────────
function V3PolicyTest({
  collapsed, onToggle,
  fixtures, selectedFxId, onSelectFx,
  verdict, state, onFocusGuard, onReevaluate, draftCount,
}) {
  const fx = fixtures.find(f => f.id === selectedFxId) || fixtures[0];
  const isDeny = verdict.decision.kind === 'Deny';

  if (collapsed) {
    return (
      <aside className="pane">
        <div className="coll-handle" onClick={onToggle}>
          <div className="ch-top">
            <div className="ch-arrow"><V3I.caretLeft style={{ width: 12, height: 12 }} /></div>
            <span className="ch-lbl">Policy test · {fx.label.split(' · ')[0]}</span>
          </div>
          {isDeny && <div className="ch-warn-dot" title="현재 평결: Deny" />}
        </div>
      </aside>
    );
  }

  return (
    <aside className="pane pt-pane">
      <div className="pt-h">
        <span className="pt-t">Policy test</span>
        <button className="pt-fold" onClick={onToggle} title="패널 접기">
          <V3I.caretRight style={{ width: 12, height: 12 }} />
        </button>
      </div>

      <div className="pt-scroll">
        {/* Verdict card */}
        <div className={`pt-verdict ${isDeny ? 'deny' : 'pass'}`}>
          <div className="pt-v-top">
            <span className="pt-v-kw">{verdict.decision.kind}</span>
            <span className="pt-v-sev">{verdict.decision.severity}</span>
            <span style={{ flex: 1 }} />
            <button className="pt-btn" style={{ flex: 'none', padding: '4px 9px' }} onClick={onReevaluate} title="재평가">
              <V3I.play style={{ width: 12, height: 12 }} /> Re
            </button>
          </div>
          <div className="pt-v-reason">"{verdict.decision.reason}"</div>
          <div className="pt-v-trace-k">매칭된 가드</div>
          {verdict.matchedLeafIds.length === 0 ? (
            <div className="pt-v-none">매칭 없음 — 정책 비활성</div>
          ) : (
            <div className="pt-v-trace-list">
              {verdict.matchedLeafIds.map(gid => {
                const node = state.nodes[gid];
                if (!node) return null;
                return (
                  <div key={gid} className="pt-v-match" onClick={() => onFocusGuard(gid)}>
                    <span className="gid">{gid}</span>
                    <span className="gt">{node.note || node.label}</span>
                    <V3I.arrowRight style={{ width: 11, height: 11, color: 'var(--slate-400)' }} />
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* Sample tx selector */}
        <div>
          <div className="pt-section-h">샘플 트랜잭션</div>
          <div className="pt-fxs">
            {fixtures.map(f => (
              <button key={f.id} className={`pt-fx ${selectedFxId === f.id ? 'on' : ''}`}
                onClick={() => onSelectFx(f.id)}>
                <span className={`pt-fx-d ${f.id === selectedFxId ? (isDeny ? 'deny' : 'pass') : 'pass'}`} />
                <span>{f.label}</span>
              </button>
            ))}
          </div>
        </div>

        {/* TX context */}
        <div>
          <div className="pt-section-h">tx context</div>
          <div className="pt-tx">
            <Row k="from"                v={fx.tx.from} mono />
            <Row k="recipient"           v={fx.tx.recipient} mono
                 warn={fx.tx.recipient !== fx.tx.from} />
            <Row k="swapMode"            v={fx.tx.swapMode}
                 warn={fx.tx.swapMode === 'market'} />
            <Row k="inputAmount"         v={fx.tx.inputAmount} />
            <Row k="outputAmount"        v={fx.tx.outputAmount} />
            <Row k="feeBps"              v={fx.tx.feeBps} />
            <Row k="validityDeltaSec"    v={`${fx.tx.validityDeltaSec} sec`} dashed
                 warn={fx.tx.validityDeltaSec < 30} />
            <Row k="recipientIsContract" v={String(fx.tx.recipientIsContract)} dashed
                 warn={fx.tx.recipientIsContract} />
            <Row k="totalInputUsd"       v={`$${fx.tx.totalInputUsd}`} dashed />
          </div>
        </div>

        {/* Draft note */}
        {draftCount > 0 && (
          <div style={{
            background: 'var(--warn-50)', border: '1px solid var(--warn-200)',
            borderRadius: 9, padding: '8px 10px',
            display: 'flex', alignItems: 'flex-start', gap: 8,
            fontSize: 11.5, color: 'var(--warn-800)',
          }}>
            <V3I.warn style={{ width: 13, height: 13, color: 'var(--warn-700)', flexShrink: 0, marginTop: 1 }} />
            <span>
              <b>미연결 {draftCount}개</b> — 평가에서 제외되어 있어요. 캔버스에서
              컨테이너 안으로 끌어 넣으면 정책에 포함됩니다.
            </span>
          </div>
        )}

        {/* Heavy tools */}
        <div className="pt-actions">
          <button className="pt-btn" onClick={onReevaluate}>
            <V3I.play style={{ width: 12, height: 12 }} /> Re-evaluate
          </button>
          <button className="pt-btn">
            <V3I.edit style={{ width: 12, height: 12 }} /> tx 직접 편집
          </button>
        </div>
      </div>
    </aside>
  );
}

function Row({ k, v, mono, dashed, warn }) {
  return (
    <div className={`pt-row ${dashed ? 'dashed' : ''} ${warn ? 'warn' : ''}`}>
      <span className="pt-row-k">{k}</span>
      <span className={`pt-row-v ${mono ? 'mono' : ''}`}>{v}</span>
    </div>
  );
}

// ─────────── LIVE CEDAR PREVIEW (bottom strip, always-on) ───────────
function V3CedarStrip({ cedar, collapsed, onToggle, focusedLeafId, matchedLeafIds, lastChangeId, onCopy }) {
  return (
    <div className={`cedar-strip ${collapsed ? 'col' : ''}`}>
      <div className="cedar-h">
        <span className="ch-lang">CEDAR · live</span>
        <span className="ch-sub">{collapsed ? '접힘 — 클릭으로 펴기' : '200ms debounce · 캔버스와 1:1 동기화'}</span>
        {!collapsed && (
          <span className="ch-flash"><span className="cf-d" /><span>변경 라인 강조</span></span>
        )}
        <div className="ch-r">
          {!collapsed && <button className="ch-r-btn" onClick={onCopy}>복사</button>}
          {!collapsed && <button className="ch-r-btn">.cedar 내보내기</button>}
          <button className="ch-fold" onClick={onToggle} title={collapsed ? '펴기' : '접기'}>
            <V3I.caretDown style={{ width: 12, height: 12, transform: collapsed ? 'rotate(0)' : 'rotate(180deg)', transition: 'transform 160ms' }} />
          </button>
        </div>
      </div>
      {!collapsed && (
        <div className="cedar-body">
          {cedar.lines.map(line => {
            const focused = focusedLeafId && line.guardId === focusedLeafId;
            const matched = matchedLeafIds && matchedLeafIds.includes(line.guardId);
            const flash = lastChangeId && line.guardId === lastChangeId;
            return (
              <div key={line.n} className={`cedar-line ${focused ? 'focus' : ''} ${matched ? 'match' : ''} ${flash ? 'flash' : ''}`}>
                <span className="cedar-gut">{line.n}</span>
                <span className="cedar-text">
                  {tok3(line.text).map((tk, i) => <span key={i} className={tk.c}>{tk.t}</span>)}
                </span>
                {line.kind === 'guard' && line.guardId && (
                  <span className={`cedar-tag ${line.custom ? 'custom' : ''}`}>{line.guardId}</span>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

// ─────────── CODE MODE (full Cedar editor view — read-only) ───────────
function V3CodeMode({ cedar, focusedLeafId, matchedLeafIds }) {
  return (
    <div className="pane" style={{ background: 'var(--slate-800)', border: '1px solid var(--slate-700)' }}>
      <div className="cedar-h" style={{ borderBottomColor: 'var(--slate-700)' }}>
        <span className="ch-lang">CEDAR · full</span>
        <span className="ch-sub">읽기전용 · Editor와 실시간 동기화</span>
        <div className="ch-r">
          <button className="ch-r-btn">복사</button>
          <button className="ch-r-btn">파일 내보내기</button>
        </div>
      </div>
      <div className="cedar-body" style={{ maxHeight: 'none', flex: 1 }}>
        {cedar.lines.map(line => {
          const focused = focusedLeafId && line.guardId === focusedLeafId;
          const matched = matchedLeafIds && matchedLeafIds.includes(line.guardId);
          return (
            <div key={line.n} className={`cedar-line ${focused ? 'focus' : ''} ${matched ? 'match' : ''}`}>
              <span className="cedar-gut">{line.n}</span>
              <span className="cedar-text">
                {tok3(line.text).map((tk, i) => <span key={i} className={tk.c}>{tk.t}</span>)}
              </span>
              {line.kind === 'guard' && line.guardId && (
                <span className={`cedar-tag ${line.custom ? 'custom' : ''}`}>{line.guardId}</span>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ─────────── MANIFEST SLIDEOVER ───────────
function V3ManifestSlideover({ open, onClose }) {
  const [tab, setTab] = useSP('requires');
  if (!open) return null;

  return (
    <>
      <div className="so-scrim" onClick={onClose} />
      <div className="so" data-screen-label="Manifest · /manifests/swap">
        <div className="so-h">
          <div>
            <div className="so-eye">SWAP · MANIFEST</div>
            <div className="so-t">신호 정의 (signal contract)</div>
            <div className="so-d">매니페스트를 편집하면 에디터 팔레트가 바뀝니다. 버전·마이그레이션은 이번 범위 제외 (C-6).</div>
          </div>
          <button className="so-x" onClick={onClose}><V3I.x style={{ width: 14, height: 14 }} /></button>
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
          <button className={`so-tab ${tab === 'requires' ? 'on' : ''}`} onClick={() => setTab('requires')}>Requirements ({V3_SIGS.custom.length})</button>
          <button className={`so-tab ${tab === 'base' ? 'on' : ''}`} onClick={() => setTab('base')}>기본 ({V3_SIGS.base.length})</button>
          <button className={`so-tab ${tab === 'history' ? 'on' : ''}`} onClick={() => setTab('history')}>변경 이력</button>
        </div>

        <div className="so-body">
          {tab === 'requires' && <RequiresTab />}
          {tab === 'base' && <BaseTab />}
          {tab === 'history' && <HistoryTab />}
        </div>

        <div className="so-foot">
          <span className="so-foot-warn">
            <V3I.warn style={{ width: 13, height: 13 }} />
            저장하면 hash가 바뀝니다 — 참조 정책 <b>8개</b>에 스큐 경고가 켜져요.
          </span>
          <span style={{ flex: 1 }} />
          <button className="btn-secondary" onClick={onClose}>취소</button>
          <button className="btn-primary on">manifest 저장</button>
        </div>
      </div>
    </>
  );
}

function ProducedPreview() {
  const chips = [...V3_SIGS.base.slice(0, 4), ...V3_SIGS.custom];
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
            <span className="ct">{s.leafType}</span>
          </span>
        ))}
      </div>
    </div>
  );
}

function BaseTab() {
  return (
    <>
      <ProducedPreview />
      <div className="so-section-h">calldata에서 추출되는 기본 필드</div>
      {V3_SIGS.base.map(s => (
        <div key={s.id} style={{
          display: 'grid', gridTemplateColumns: '22px 1fr auto', alignItems: 'center', gap: 10,
          padding: '10px 4px', borderBottom: '1px dashed var(--hairline-soft)', fontSize: 12,
        }}>
          <span className={`so-out-sw s-${s.shape}`} />
          <span>
            <div style={{ fontWeight: 600, color: 'var(--slate-900)' }}>{s.label.ko}</div>
            <div style={{ fontFamily: 'var(--ff-mono)', fontSize: 11, color: 'var(--slate-500)' }}>context.{s.id}</div>
          </span>
          <span className="so-out-type">{s.leafType}</span>
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
          <span style={{ fontFamily: 'var(--ff-mono)', fontSize: 11.5, fontWeight: 700, color: 'var(--slate-700)', background: 'var(--fog-200)', padding: '2px 7px', borderRadius: 5, height: 'fit-content' }}>{r.h}</span>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: 12.5, color: 'var(--slate-900)', fontWeight: 600 }}>{r.d}</div>
            <div style={{ fontSize: 11, color: 'var(--slate-500)', marginTop: 2 }}>{r.who} · {r.when}</div>
          </div>
          <span style={{ fontFamily: 'var(--ff-mono)', fontSize: 10, fontWeight: 700, color: r.t === '현재' ? 'var(--pass-700)' : 'var(--slate-500)', background: r.t === '현재' ? 'var(--pass-100)' : 'var(--fog-200)', padding: '2px 7px', borderRadius: 4, height: 'fit-content', alignSelf: 'center' }}>{r.t}</span>
        </div>
      ))}
    </>
  );
}

function RequiresTab() {
  const cards = [
    { id: 'r1', method: 'oracle.getDeadlineDelta', optional: false,
      params: [{ name: 'now', type: 'Long', value: 'block.timestamp' }],
      outputs: [{ name: 'validityDeltaSec', type: 'Long', shape: 'pill', required: true, dashed: true }],
      pp: { compat: 19, total: 19, type: 'Long' } },
    { id: 'r2', method: 'chain.isContractAt', optional: true,
      params: [{ name: 'address', type: 'Address', value: 'context.recipient' }],
      outputs: [{ name: 'recipientIsContract', type: 'Boolean', shape: 'hex', required: false, dashed: true }],
      pp: { compat: 7, total: 12, type: 'Address' } },
    { id: 'r3', method: 'oracle.tokenSpotPriceUsd', optional: true,
      params: [
        { name: 'token',  type: 'Address', value: 'context.inputToken.asset.address' },
        { name: 'amount', type: 'Long',    value: 'context.inputAmount' },
      ],
      outputs: [
        { name: 'totalInputUsd',                    type: 'Decimal', shape: 'pill', required: false, dashed: true },
        { name: 'totalInputFractionOfPortfolioBps', type: 'Long',    shape: 'pill', required: false, dashed: true },
      ],
      pp: { compat: 4, total: 12, type: 'Decimal' } },
  ];

  return (
    <>
      <ProducedPreview />
      <div className="so-section-h">Requirements — 외부 조회</div>
      {cards.map(c => <RequireCard key={c.id} card={c} />)}
      <button style={{
        appearance: 'none', background: 'transparent', border: '1px dashed var(--slate-300)',
        color: 'var(--slate-700)', padding: '10px 14px', borderRadius: 9, width: '100%',
        fontWeight: 600, fontSize: 12.5, marginTop: 8,
      }}>+ Requirement 추가</button>
    </>
  );
}

function RequireCard({ card }) {
  return (
    <div className="so-req">
      <div className="so-req-h">
        <span className="so-req-id">{card.id}</span>
        <span className="so-req-method">{card.method}</span>
        <span style={{ flex: 1 }} />
        <span className={`so-req-flag ${card.optional ? 'opt' : ''}`}>
          요구 레벨 · {card.optional ? 'optional' : 'required'}
          <span className="so-req-flag-d">{card.optional ? '데이터 없으면 이 요구는 건너뜀' : '실패 시 정책 평가 전체 중단'}</span>
        </span>
      </div>
      <div className="so-req-body">
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
                <span className="so-pp-cnt">{card.pp.compat}/{card.pp.total} paths · {p.type}</span>
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
                  <span className="so-req-flag-d">{o.required ? '이 출력 없으면 전체 실패' : '데이터 없으면 이 검사는 건너뜀'}</span>
                </span>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { V3PolicyTest, V3CedarStrip, V3CodeMode, V3ManifestSlideover, tok3 });
