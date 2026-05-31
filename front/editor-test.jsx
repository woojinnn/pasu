// editor-test.jsx
// Right 27% — Policy Test pane, bottom Cedar preview, and Manifest slideover.

const { useState: useStateT, useEffect: useEffectT } = React;

// ─── Right pane: Policy Test ────────────────────────────────────────────────
function TestPane({ fixtures, onSelectFixture, selectedFixture, policy, colorScheme }) {
  const fx = fixtures.find(f => f.id === selectedFixture) || fixtures[0];
  const deny = fx.matches.length > 0;

  return (
    <aside className="test-pane" data-screen-label="Policy test pane">
      <div className="tp-head">
        <span className="tp-title">Policy test</span>
        <span className="tp-sub">샘플 tx로 정책 평가</span>
      </div>

      <div className="tp-section">
        <div className="tp-section-h">샘플 트랜잭션</div>
        <div className="tp-fxs">
          {fixtures.map(f => (
            <button key={f.id}
              className={`tp-fx ${selectedFixture === f.id ? 'tp-fx-on' : ''}`}
              onClick={() => onSelectFixture(f.id)}>
              <span className={`tp-fx-d ${f.matches.length ? 'tp-fx-d-deny' : 'tp-fx-d-pass'}`} />
              <span className="tp-fx-t">{f.label}</span>
            </button>
          ))}
        </div>
      </div>

      <div className="tp-section">
        <div className="tp-section-h">tx context</div>
        <div className="tp-tx">
          <Row k="from"               v={fx.tx.from} mono />
          <Row k="recipient"          v={fx.tx.recipient} mono
            warn={fx.tx.recipient !== fx.tx.from} />
          <Row k="swapMode"           v={fx.tx.swapMode}
            warn={fx.tx.swapMode === 'market'} />
          <Row k="inputAmount"        v={fx.tx.inputAmount} />
          <Row k="outputAmount"       v={fx.tx.outputAmount} />
          <Row k="validityDeltaSec"   v={`${fx.tx.validityDeltaSec} sec`} dashed
            warn={fx.tx.validityDeltaSec < 30} />
          <Row k="recipientIsContract" v={String(fx.tx.recipientIsContract)} dashed
            warn={fx.tx.recipientIsContract} />
        </div>
      </div>

      <div className="tp-section">
        <div className="tp-section-h">평가 결과</div>
        <div className={`tp-result tp-result-${deny ? 'deny' : 'pass'}`}>
          <div className="tp-r-top">
            <span className="tp-r-deci">{deny ? 'Deny' : 'Allow'}</span>
            <span className="tp-r-sev">{deny ? 'FAIL' : 'PASS'}</span>
          </div>
          <div className="tp-r-reason">
            {deny
              ? '"swap baseline violated"'
              : '정책에 일치하는 가드 없음'}
          </div>
          <div className="tp-r-trace">
            <span className="tp-r-trace-k">매칭된 가드</span>
            {fx.matches.length === 0 ? (
              <span className="tp-r-trace-none">없음</span>
            ) : (
              <div className="tp-r-trace-list">
                {fx.matches.map(mid => {
                  const g = policy.root.children.find(c => c.id === mid);
                  if (!g) return null;
                  return (
                    <div key={mid} className="tp-r-match">
                      <span className="tp-r-match-id">{mid}</span>
                      <span className="tp-r-match-t">{g.note}</span>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </div>
      </div>

      <div className="tp-actions">
        <button className="tp-btn"><I.play style={{ width: 12, height: 12 }} /> Re-evaluate</button>
        <button className="tp-btn">tx 직접 편집</button>
      </div>
    </aside>
  );
}

function Row({ k, v, mono, dashed, warn }) {
  return (
    <div className={`tp-row ${dashed ? 'tp-row-dashed' : ''} ${warn ? 'tp-row-warn' : ''}`}>
      <span className="tp-row-k">{k}</span>
      <span className={`tp-row-v ${mono ? 'mono' : ''}`}>{v}</span>
    </div>
  );
}

// ─── Bottom: live Cedar preview (collapsible) ───────────────────────────────
function CedarPreview({ collapsed, onToggle, highlightGuard }) {
  return (
    <div className={`cedar-preview ${collapsed ? 'cp-coll' : ''}`}>
      <button className="cp-head" onClick={onToggle}>
        <I.caretDown style={{ width: 12, height: 12, transform: collapsed ? 'rotate(-90deg)' : 'rotate(0)', transition: 'transform 120ms' }} />
        <span className="cp-title">Live Cedar preview</span>
        <span className="cp-sub">블록 변경 시 200ms debounce</span>
        <span style={{ flex: 1 }} />
        {!collapsed && <span className="cp-hint">변경 라인 Sage Leaf 1s flash</span>}
      </button>
      {!collapsed && (
        <div className="cp-body">
          {CEDAR_TEXT.lines.slice(7, 14).map(line => (
            <div key={line.n} className={`cp-line ${highlightGuard && line.guardId === highlightGuard ? 'cp-line-flash' : ''}`}>
              <span className="cp-gutter">{line.n}</span>
              <span className="cp-text">
                {tokenizeCedarLine(line.text).map((tk, i) => <span key={i} className={tk.c}>{tk.t}</span>)}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ─── Manifest slideover (right-edge slide-in) ───────────────────────────────
function ManifestSlideover({ open, onClose }) {
  if (!open) return null;
  return (
    <>
      <div className="slideover-scrim" onClick={onClose} />
      <div className="slideover" data-screen-label="Manifest editor">
        <div className="so-head">
          <div>
            <div className="so-eyebrow">SWAP · MANIFEST</div>
            <div className="so-title">신호 정의 편집</div>
            <div className="so-sub">action별 공유 자산 · 정책마다 다른 게 아님</div>
          </div>
          <button className="so-close" onClick={onClose}><I.x style={{ width: 14, height: 14 }} /></button>
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
          <button className="so-tab so-tab-on">기본 (6)</button>
          <button className="so-tab">Enrichment (9)</button>
          <button className="so-tab">변경 이력</button>
        </div>

        <div className="so-body">
          <div className="so-section-h">calldata에서 추출되는 기본 필드</div>

          {SIGNAL_CATALOG.base.map(s => (
            <div key={s.id} className="so-sig">
              <div className="so-sig-head">
                <span className={`sw-base sw-${s.shape || 'rect'}`} />
                <span className="so-sig-name">{s.label.ko}</span>
                <span className="so-sig-path mono">context.{s.id}</span>
                {s.kind === 'group' && <span className="so-sig-tag">group · cascade {s.cascade.length}</span>}
                {s.optional && <span className="so-sig-tag so-sig-tag-opt">optional</span>}
              </div>
              {s.kind === 'group' && (
                <div className="so-sig-cascade">
                  {(CASCADE_LEAVES[s.id] || []).map((leaf, i) => (
                    <div key={i} className={`so-leaf ${leaf.custom ? 'so-leaf-custom' : ''}`}>
                      <span className={`sw-${leaf.shape} ${leaf.custom ? 'sw-dashed' : ''}`} />
                      <span className="so-leaf-l">{leaf.label}</span>
                      <span className="so-leaf-p mono">{leaf.path}</span>
                      {leaf.custom && (
                        <span className="so-leaf-tag">enrichment {leaf.reparentedFrom ? `← ${leaf.reparentedFrom}` : ''}</span>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>

        <div className="so-foot">
          <span className="so-foot-warn">
            <I.warn style={{ width: 13, height: 13 }} />
            저장하면 hash가 바뀝니다 — 이 신호를 참조하는 정책 8개에 <b>스큐 경고</b>가 켜집니다.
          </span>
          <span style={{ flex: 1 }} />
          <button className="btn-secondary">취소</button>
          <button className="btn-primary on">manifest 저장</button>
        </div>
      </div>
    </>
  );
}

Object.assign(window, { TestPane, CedarPreview, ManifestSlideover });
