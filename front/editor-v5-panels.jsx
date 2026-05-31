// editor-v5-panels.jsx — Policy test sheet, Cedar live preview sheet, Manifest slideover, OR modal

const { useState: useSP, useEffect: useEP, useRef: useRP, useMemo: useMP } = React;

function tok5(text) {
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

// ─────────── POLICY TEST SHEET ───────────
function V5PolicyTest({
  inline,                     // when true → no chrome (used in Code mode split)
  onClose,
  fixtures, selectedFxId, onSelectFx,
  verdict, state, onFocusGuard, onReevaluate, draftCount,
}) {
  const fx = fixtures.find(f => f.id === selectedFxId) || fixtures[0];
  const isDeny = verdict.decision.kind === 'Deny';

  const inner = (
    <div className="pt-scroll">
      <div className={`pt-verdict ${isDeny ? 'deny' : 'pass'}`}>
        <div className="pt-v-top">
          <span className="pt-v-kw">{verdict.decision.kind}</span>
          <span className="pt-v-sev">{verdict.decision.severity}</span>
          <span style={{ flex: 1 }} />
          <button className="pt-btn" style={{ flex: 'none', padding: '4px 9px' }} onClick={onReevaluate}>
            <V5I.play style={{ width: 12, height: 12 }} /> Re
          </button>
        </div>
        <div className="pt-v-reason">"{verdict.decision.reason}"</div>
        <div className="pt-v-trace-k">매칭된 가드</div>
        {verdict.matchedLeafIds.length === 0 ? (
          <div className="pt-v-none">매칭 없음</div>
        ) : (
          <div className="pt-v-trace-list">
            {verdict.matchedLeafIds.map(gid => {
              const n = state.nodes[gid]; if (!n) return null;
              return (
                <div key={gid} className="pt-v-match" onClick={() => onFocusGuard(gid)}>
                  <span className="gid">{gid}</span>
                  <span className="gt">{n.note || n.label}</span>
                  <V5I.arrowRight style={{ width: 11, height: 11, color: 'var(--slate-400)' }} />
                </div>
              );
            })}
          </div>
        )}
      </div>

      <div>
        <div className="pt-section-h">샘플 트랜잭션</div>
        <div className="pt-fxs">
          {fixtures.map(f => (
            <button key={f.id} className={`pt-fx ${selectedFxId === f.id ? 'on' : ''}`} onClick={() => onSelectFx(f.id)}>
              <span className={`pt-fx-d ${f.id === selectedFxId ? (isDeny ? 'deny' : 'pass') : 'pass'}`} />
              <span>{f.label}</span>
            </button>
          ))}
        </div>
      </div>

      <div>
        <div className="pt-section-h">tx context</div>
        <div className="pt-tx">
          <Row k="from"                v={fx.tx.from} mono />
          <Row k="recipient"           v={fx.tx.recipient} mono warn={fx.tx.recipient !== fx.tx.from} />
          <Row k="swapMode"            v={fx.tx.swapMode} warn={fx.tx.swapMode === 'market'} />
          <Row k="inputAmount"         v={fx.tx.inputAmount} />
          <Row k="outputAmount"        v={fx.tx.outputAmount} />
          <Row k="feeBps"              v={fx.tx.feeBps} />
          <Row k="validityDeltaSec"    v={`${fx.tx.validityDeltaSec} sec`} dashed warn={fx.tx.validityDeltaSec < 30} />
          <Row k="recipientIsContract" v={String(fx.tx.recipientIsContract)} dashed warn={fx.tx.recipientIsContract} />
          <Row k="totalInputUsd"       v={`$${fx.tx.totalInputUsd}`} dashed />
        </div>
      </div>

      {draftCount > 0 && (
        <div style={{
          background: 'var(--warn-50)', border: '1px solid var(--warn-200)',
          borderRadius: 9, padding: '8px 10px',
          display: 'flex', alignItems: 'flex-start', gap: 8,
          fontSize: 11.5, color: 'var(--warn-800)',
        }}>
          <V5I.warn style={{ width: 13, height: 13, color: 'var(--warn-700)', flexShrink: 0, marginTop: 1 }} />
          <span><b>미연결 {draftCount}개</b> — 평가에서 제외됨. 루트 와이어로 연결해야 정책에 포함됩니다.</span>
        </div>
      )}

      <div className="pt-actions">
        <button className="pt-btn" onClick={onReevaluate}><V5I.play style={{ width: 12, height: 12 }} /> Re-evaluate</button>
        <button className="pt-btn"><V5I.edit style={{ width: 12, height: 12 }} /> tx 직접 편집</button>
      </div>
    </div>
  );

  if (inline) {
    return (
      <div className="code-pt">
        <div className="pt-h"><span className="pt-t">Policy test</span></div>
        {inner}
      </div>
    );
  }

  return (
    <div className="sheet sheet-policy">
      <div className="pt-h">
        <span className="pt-t">Policy test</span>
        <button className="pt-x" onClick={onClose} title="닫기 (Esc)"><V5I.x style={{ width: 12, height: 12 }} /></button>
      </div>
      {inner}
    </div>
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

// ─────────── CEDAR LIVE PREVIEW SHEET (resizable) ───────────
function V5CedarSheet({ cedar, onClose, focusedLeafId, matchedLeafIds, lastChangeId, onCopy, height, onHeightChange }) {
  const ref = useRP(null);
  const dragRef = useRP(null);

  // Resize handle (top edge)
  useEP(() => {
    const move = (e) => {
      if (!dragRef.current) return;
      const d = dragRef.current;
      const dy = d.startY - e.clientY; // dragging up grows
      const next = Math.max(120, Math.min(500, d.startH + dy));
      onHeightChange(next);
    };
    const up = () => { dragRef.current = null; document.body.style.cursor = ''; };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    return () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up); };
  }, [onHeightChange]);

  return (
    <div ref={ref} className="sheet sheet-cedar" style={{ height: `${height}px` }}>
      <div className="cedar-resize"
        onPointerDown={(e) => {
          dragRef.current = { startY: e.clientY, startH: height };
          document.body.style.cursor = 'ns-resize';
          e.preventDefault();
        }}
      />
      <div className="cedar-h">
        <span className="ch-lang">CEDAR · live</span>
        <span className="ch-sub">200ms · 캔버스 ↔ 1:1</span>
        <span className="ch-flash"><span className="cf-d" /><span>변경 라인 강조</span></span>
        <div className="ch-r">
          <button className="ch-r-btn" onClick={onCopy}>복사</button>
          <button className="ch-r-btn">.cedar 내보내기</button>
          <button className="ch-x" onClick={onClose} title="닫기 (Esc)"><V5I.x style={{ width: 12, height: 12 }} /></button>
        </div>
      </div>
      <div className="cedar-body">
        {cedar.lines.map(line => {
          const focused = focusedLeafId && line.guardId === focusedLeafId;
          const matched = matchedLeafIds && matchedLeafIds.includes(line.guardId);
          const flash = lastChangeId && line.guardId === lastChangeId;
          return (
            <div key={line.n} className={`cedar-line ${focused ? 'focus' : ''} ${matched ? 'match' : ''} ${flash ? 'flash' : ''}`}>
              <span className="cedar-gut">{line.n}</span>
              <span className="cedar-text">
                {tok5(line.text).map((tk, i) => <span key={i} className={tk.c}>{tk.t}</span>)}
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

// ─────────── CODE MODE PANEL (Cedar full + Policy test split) ───────────
function V5CodeMode({ cedar, focusedLeafId, matchedLeafIds, lastChangeId, fixtures, selectedFxId, onSelectFx, verdict, state, onFocusGuard, onReevaluate, draftCount, onCopy }) {
  return (
    <div className="code-split">
      <div className="code-pane">
        <div className="cedar-h">
          <span className="ch-lang">CEDAR · full</span>
          <span className="ch-sub">읽기전용 · Builder와 실시간 동기화</span>
          <div className="ch-r">
            <button className="ch-r-btn" onClick={onCopy}>복사</button>
            <button className="ch-r-btn">.cedar 내보내기</button>
          </div>
        </div>
        <div className="cedar-body">
          {cedar.lines.map(line => {
            const focused = focusedLeafId && line.guardId === focusedLeafId;
            const matched = matchedLeafIds && matchedLeafIds.includes(line.guardId);
            const flash = lastChangeId && line.guardId === lastChangeId;
            return (
              <div key={line.n} className={`cedar-line ${focused ? 'focus' : ''} ${matched ? 'match' : ''} ${flash ? 'flash' : ''}`}>
                <span className="cedar-gut">{line.n}</span>
                <span className="cedar-text">
                  {tok5(line.text).map((tk, i) => <span key={i} className={tk.c}>{tk.t}</span>)}
                </span>
                {line.kind === 'guard' && line.guardId && (
                  <span className={`cedar-tag ${line.custom ? 'custom' : ''}`}>{line.guardId}</span>
                )}
              </div>
            );
          })}
        </div>
      </div>
      <V5PolicyTest inline
        fixtures={fixtures}
        selectedFxId={selectedFxId}
        onSelectFx={onSelectFx}
        verdict={verdict}
        state={state}
        onFocusGuard={onFocusGuard}
        onReevaluate={onReevaluate}
        draftCount={draftCount}
      />
    </div>
  );
}

// ─────────── OR confirm modal ───────────
function V5ORConfirmModal({ onConfirm, onCancel, dontShowAgain, onSetDontShow }) {
  return (
    <div className="scrim" onClick={onCancel}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-h">
          <div className="m-ic"><V5I.warn style={{ width: 18, height: 18 }} /></div>
          <div className="m-t">OR을 추가하면 괄호 우선순위는 직접 책임집니다</div>
        </div>
        <div className="modal-d">
          OR 묶음을 도입하면 조건 묶음의 <b>괄호 우선순위(parenthesization)</b>를 직접 지정하게 됩니다.
          위치를 잘못 정하면 정책이 <b>조용히 의도와 다르게</b> 평가될 수 있어요.
        </div>
        <div className="modal-eg">
          <span className="ok">A AND (B OR C)</span> &nbsp; vs &nbsp; <span className="bad">(A AND B) OR C</span>
        </div>
        <div className="modal-foot">
          <label><input type="checkbox" checked={dontShowAgain} onChange={(e) => onSetDontShow(e.target.checked)} /><span>이 세션에서는 다시 묻지 않기</span></label>
          <span className="spc" />
          <button className="btn-secondary" onClick={onCancel}>취소</button>
          <button className="btn-primary on" onClick={onConfirm}>이해했어요 · OR 추가</button>
        </div>
      </div>
    </div>
  );
}

// ─────────── MANIFEST SLIDEOVER (compact reuse) ───────────
function V5ManifestSlideover({ open, onClose }) {
  const [tab, setTab] = useSP('requires');
  if (!open) return null;
  return (
    <>
      <div className="so-scrim" onClick={onClose} />
      <div className="so">
        <div className="so-h">
          <div>
            <div className="so-eye">SWAP · MANIFEST</div>
            <div className="so-t">신호 정의 (signal contract)</div>
            <div className="so-d">매니페스트를 편집하면 에디터 팔레트가 바뀝니다. 버전·마이그레이션은 이번 범위 제외.</div>
          </div>
          <button className="so-x" onClick={onClose}><V5I.x style={{ width: 14, height: 14 }} /></button>
        </div>
        <div className="so-meta">
          <span className="so-meta-k">Action</span><span className="so-meta-v">swap</span>
          <span className="so-meta-sep" />
          <span className="so-meta-k">Hash</span><span className="so-meta-v mono">#fc20a91</span>
          <span className="so-meta-sep" />
          <span className="so-meta-k">정책 참조</span><span className="so-meta-v">8개</span>
        </div>
        <div className="so-tabs">
          <button className={`so-tab ${tab === 'requires' ? 'on' : ''}`} onClick={() => setTab('requires')}>Requirements ({V5_SIGS.custom.length})</button>
          <button className={`so-tab ${tab === 'base' ? 'on' : ''}`} onClick={() => setTab('base')}>기본 ({V5_SIGS.base.length})</button>
          <button className={`so-tab ${tab === 'history' ? 'on' : ''}`} onClick={() => setTab('history')}>변경 이력</button>
        </div>
        <div className="so-body">
          {tab === 'base' && <BaseTab />}
          {tab === 'requires' && <RequiresTab />}
          {tab === 'history' && <HistoryTab />}
        </div>
        <div className="so-foot">
          <span className="so-foot-warn"><V5I.warn style={{ width: 13, height: 13 }} />저장하면 hash가 바뀝니다 — 참조 정책 <b>8개</b>에 스큐 경고가 켜져요.</span>
          <span style={{ flex: 1 }} />
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
      <PreviewPanel />
      <div className="so-section-h">calldata 기본 필드</div>
      {V5_SIGS.base.map(s => (
        <div key={s.id} style={{ display: 'grid', gridTemplateColumns: '22px 1fr auto', alignItems: 'center', gap: 10, padding: '10px 4px', borderBottom: '1px dashed var(--hairline-soft)', fontSize: 12 }}>
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
    { h: '#fc20a91', t: '현재', d: 'recipientIsContract enrichment 추가' },
    { h: '#fc1a83c', t: '이전', d: 'validityDeltaSec 단위 수정 후 롤백' },
    { h: '#fc14d50', t: '초기', d: 'swap manifest 최초 생성' },
  ];
  return (
    <>
      <div className="so-section-h">매니페스트 변경 이력</div>
      {rows.map((r, i) => (
        <div key={i} style={{ display: 'flex', gap: 12, padding: '10px 4px', borderBottom: '1px dashed var(--hairline-soft)' }}>
          <span style={{ fontFamily: 'var(--ff-mono)', fontSize: 11.5, fontWeight: 700, color: 'var(--slate-700)', background: 'var(--fog-200)', padding: '2px 7px', borderRadius: 5, height: 'fit-content' }}>{r.h}</span>
          <div style={{ flex: 1, fontSize: 12.5, color: 'var(--slate-900)', fontWeight: 600 }}>{r.d}</div>
          <span style={{ fontFamily: 'var(--ff-mono)', fontSize: 10, fontWeight: 700, color: r.t === '현재' ? 'var(--pass-700)' : 'var(--slate-500)', background: r.t === '현재' ? 'var(--pass-100)' : 'var(--fog-200)', padding: '2px 7px', borderRadius: 4, height: 'fit-content', alignSelf: 'center' }}>{r.t}</span>
        </div>
      ))}
    </>
  );
}
function RequiresTab() {
  const cards = [
    { id: 'r1', method: 'oracle.getDeadlineDelta', optional: false,
      outputs: [{ name: 'validityDeltaSec', type: 'Long', shape: 'pill', required: true, dashed: true }],
      pp: { compat: 19, total: 19, type: 'Long' } },
    { id: 'r2', method: 'chain.isContractAt', optional: true,
      outputs: [{ name: 'recipientIsContract', type: 'Boolean', shape: 'hex', required: false, dashed: true }],
      pp: { compat: 7, total: 12, type: 'Address' } },
  ];
  return (
    <>
      <PreviewPanel />
      <div className="so-section-h">Requirements — 외부 조회</div>
      {cards.map(c => (
        <div key={c.id} className="so-req">
          <div className="so-req-h">
            <span className="so-req-id">{c.id}</span>
            <span className="so-req-method">{c.method}</span>
            <span style={{ flex: 1 }} />
            <span className={`so-req-flag ${c.optional ? 'opt' : ''}`}>요구 · {c.optional ? 'optional' : 'required'}
              <span className="so-req-flag-d">{c.optional ? '없으면 건너뜀' : '실패 시 전체 중단'}</span>
            </span>
          </div>
          <div className="so-req-body">
            <div>
              <div className="so-req-col-t">params</div>
              <div className="so-path-picker">
                <span style={{ color: 'var(--slate-700)' }}>context.recipient</span>
                <span className="so-pp-cnt">{c.pp.compat}/{c.pp.total} · {c.pp.type}</span>
              </div>
            </div>
            <div>
              <div className="so-req-col-t">outputs</div>
              {c.outputs.map(o => (
                <div key={o.name} style={{ marginBottom: 6 }}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                    <span className={`so-out-sw s-${o.shape} ${o.dashed ? 'dashed' : ''}`} />
                    <span style={{ fontFamily: 'var(--ff-mono)', fontSize: 11.5, color: 'var(--slate-900)', fontWeight: 600 }}>{o.name}</span>
                    <span className="so-out-type">{o.type}</span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      ))}
    </>
  );
}
function PreviewPanel() {
  const chips = [...V5_SIGS.base.slice(0, 4), ...V5_SIGS.custom];
  return (
    <div className="so-preview">
      <div className="so-preview-h">
        <span className="so-pv-t">이 매니페스트가 만드는 신호</span>
        <span className="so-pv-d">팔레트에 즉시 반영</span>
        <span className="so-pv-live">live</span>
      </div>
      <div className="so-preview-chips">
        {chips.map(s => (
          <span key={s.id} className="so-pv-chip"><span>{s.label.ko}</span><span className="ct">{s.leafType}</span></span>
        ))}
      </div>
    </div>
  );
}

Object.assign(window, { V5PolicyTest, V5CedarSheet, V5CodeMode, V5ORConfirmModal, V5ManifestSlideover, tok5 });
