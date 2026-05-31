// editor-v2-canvas.jsx
// Adaptive canvas: flat AND form ↔ nested OR canvas based on IR structure.
// Includes palette, guard blocks (with inline editors), OR confirm dialog.

const { useState: useStateCV, useRef: useRefCV, useEffect: useEffectCV } = React;

// ─── Palette (left, 240px) ──────────────────────────────────────────────────
function V2Palette({ onAddSignal, draggingId, onDragStart, onDragEnd, locale }) {
  const [q, setQ] = useStateCV('');
  const [openBase, setOpenBase] = useStateCV(true);
  const [openCustom, setOpenCustom] = useStateCV(true);

  const filter = (items) => {
    if (!q) return items;
    const qq = q.toLowerCase();
    return items.filter(s => {
      const lbl = (s.label[locale] || s.label.en).toLowerCase();
      return lbl.includes(qq) || s.id.toLowerCase().includes(qq);
    });
  };

  return (
    <aside className="ws-pane pal">
      <div className="pal-h">
        <span className="pal-t">Block palette</span>
        <span className="pal-hint">drag · click to add</span>
      </div>
      <div className="pal-search">
        <V2I.search style={{ width: 13, height: 13, color: 'var(--slate-400)' }} />
        <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="필드 · 경로 · 단위" />
        <span className="kbd">/</span>
      </div>

      <div className="pal-scroll">
        <button className="pal-sect-h" onClick={() => setOpenBase(!openBase)}>
          <V2I.caretDown style={{ width: 11, height: 11, transform: openBase ? '' : 'rotate(-90deg)', transition: 'transform 120ms' }} />
          <span className="pal-sect-t">기본 필드 (calldata)</span>
          <span className="pal-sect-c">{V2_SIGNAL_CATALOG.base.length}</span>
        </button>
        {openBase && (
          <div className="pal-items">
            {filter(V2_SIGNAL_CATALOG.base).map(s => (
              <PaletteItem key={s.id} sig={s} locale={locale}
                onAdd={() => onAddSignal(s.id)}
                onDragStart={() => onDragStart(s.id)}
                onDragEnd={onDragEnd}
                dragging={draggingId === s.id} />
            ))}
          </div>
        )}

        <button className="pal-sect-h" onClick={() => setOpenCustom(!openCustom)}>
          <V2I.caretDown style={{ width: 11, height: 11, transform: openCustom ? '' : 'rotate(-90deg)', transition: 'transform 120ms' }} />
          <span className="pal-sect-t">커스텀 (manifest enrichment)</span>
          <span className="pal-sect-c">{V2_SIGNAL_CATALOG.custom.length}</span>
        </button>
        {openCustom && (
          <div className="pal-items">
            {filter(V2_SIGNAL_CATALOG.custom).map(s => (
              <PaletteItem key={s.id} sig={s} locale={locale}
                onAdd={() => onAddSignal(s.id)}
                onDragStart={() => onDragStart(s.id)}
                onDragEnd={onDragEnd}
                dragging={draggingId === s.id} />
            ))}
          </div>
        )}
      </div>

      <div className="pal-foot">
        <div style={{ fontFamily: 'var(--ff-mono)', fontSize: 10, fontWeight: 700, color: 'var(--slate-500)', letterSpacing: '0.06em', textTransform: 'uppercase', marginBottom: 6 }}>
          shape · type
        </div>
        <div className="pal-foot-row"><span className="pal-foot-sw s-pill" /><span>pill · 숫자</span></div>
        <div className="pal-foot-row"><span className="pal-foot-sw s-rect" /><span>rect · 주소/문자열</span></div>
        <div className="pal-foot-row"><span className="pal-foot-sw s-hexagon" /><span>hex · enum/bool</span></div>
        <div className="pal-foot-row"><span className="pal-foot-sw s-rect dashed" /><span>dashed · custom</span></div>
      </div>
    </aside>
  );
}

function PaletteItem({ sig, locale, onAdd, onDragStart, onDragEnd, dragging }) {
  const color = v2ColorFor(sig.id);
  const isCustom = !!sig.custom;
  return (
    <div
      className={`pal-item ${dragging ? 'dragging' : ''}`}
      title={sig.id}
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData('text/v2-sig', sig.id);
        e.dataTransfer.effectAllowed = 'copy';
        onDragStart();
      }}
      onDragEnd={onDragEnd}
      onClick={onAdd}
    >
      <span className={`pal-sw pal-${color} s-${sig.shape} ${isCustom ? 'dashed' : ''}`} />
      <span className="pal-lbl">{sig.label[locale] || sig.label.en}</span>
      <button className="pal-add" onClick={(e) => { e.stopPropagation(); onAdd(); }} title="조건으로 추가">
        <V2I.plus style={{ width: 13, height: 13 }} />
      </button>
    </div>
  );
}

// ─── Trigger pill + Decision pill (canvas chrome) ───────────────────────────
function CanvasTrigger() {
  return (
    <div className="cv-trigger">
      <span className="tr-k">On action</span>
      <span className="tr-eq">==</span>
      <span className="tr-v">swap</span>
      <span className="tr-then">→ 다음 조건 중 하나라도 참이면:</span>
    </div>
  );
}

function CanvasDecision({ decision, onEditReason, triggered }) {
  const isDeny = decision.kind === 'Deny';
  return (
    <div className="cv-dec">
      <span className="dec-arrow">↓</span>
      <span className="dec-then">Then</span>
      <span className={`dec-kw ${isDeny ? 'deny' : 'allow'}`}>{decision.kind}</span>
      <span
        className="dec-reason"
        contentEditable
        suppressContentEditableWarning
        onBlur={(e) => onEditReason(e.target.textContent || '')}
      >"{decision.reason}"</span>
      <span className={`dec-sev ${isDeny ? '' : 'pass'}`}>severity {decision.severity}</span>
      {!triggered && <span className="dec-sev" style={{ color: 'var(--slate-300)' }}>· (현재 fixture 비매칭)</span>}
    </div>
  );
}

// ─── Operator popover ───────────────────────────────────────────────────────
function OpPopover({ leaf, onChange, onClose }) {
  const opts = V2_OPERATORS_BY_TYPE[leaf.leafType] || ['==', '!='];
  const ref = useRefCV(null);
  useEffectCV(() => {
    const h = (e) => { if (ref.current && !ref.current.contains(e.target)) onClose(); };
    setTimeout(() => window.addEventListener('click', h), 0);
    return () => window.removeEventListener('click', h);
  }, [onClose]);
  return (
    <div className="pop" ref={ref}>
      {opts.map(op => (
        <div key={op} className={`pop-item ${leaf.operator === op ? 'on' : ''}`}
             onClick={() => { onChange(op); onClose(); }}>
          {op}
        </div>
      ))}
    </div>
  );
}

// ─── Value popover (typed editor: num/bool/enum/ref) ────────────────────────
function ValuePopover({ leaf, onChange, onClose }) {
  const ref = useRefCV(null);
  const [draft, setDraft] = useStateCV(leaf.value);
  useEffectCV(() => {
    const h = (e) => { if (ref.current && !ref.current.contains(e.target)) commit(); };
    setTimeout(() => window.addEventListener('click', h), 0);
    return () => window.removeEventListener('click', h);
  }, [draft]);

  const commit = () => { onChange(draft); onClose(); };
  const sig = v2SigById(leaf.sigId);

  // For number-ish leaves we offer "literal | ref"
  const supportsRef = ['address', 'tokenNative', 'usd', 'number'].includes(leaf.leafType);

  return (
    <div className="val-pop-card" ref={ref}>
      <div className="vp-h">값 · {leaf.leafType}</div>

      {supportsRef && (
        <div className="vp-tab-row">
          <button className={draft.kind === 'ref' ? '' : 'on'}
                  onClick={() => setDraft({ kind: 'num', text: draft.kind === 'num' ? draft.text : '0', unit: draft.unit })}>
            리터럴
          </button>
          <button className={draft.kind === 'ref' ? 'on' : ''}
                  onClick={() => setDraft({ kind: 'ref', text: draft.kind === 'ref' ? draft.text : 'root.from' })}>
            ref (동적 참조)
          </button>
        </div>
      )}

      {draft.kind === 'bool' && (
        <div className="vp-toggle">
          <button className={draft.text === 'true' ? 'on' : ''} onClick={() => setDraft({ ...draft, text: 'true' })}>true</button>
          <button className={draft.text === 'false' ? 'on' : ''} onClick={() => setDraft({ ...draft, text: 'false' })}>false</button>
        </div>
      )}

      {draft.kind === 'enum' && sig && sig.options && (
        <select className="vp-input" value={draft.text}
                onChange={(e) => setDraft({ ...draft, text: e.target.value })}>
          {sig.options.map(o => <option key={o} value={o}>{o}</option>)}
        </select>
      )}

      {draft.kind === 'num' && (
        <>
          <input className="vp-input" type="text" autoFocus
                 value={draft.text}
                 onChange={(e) => setDraft({ ...draft, text: e.target.value })}
                 onKeyDown={(e) => e.key === 'Enter' && commit()} />
          {draft.unit && <div className="vp-row">단위: <span style={{ fontFamily: 'var(--ff-mono)', color: 'var(--slate-700)' }}>{draft.unit}</span></div>}
        </>
      )}

      {draft.kind === 'ref' && (
        <>
          <input className="vp-input" type="text" autoFocus
                 value={draft.text}
                 onChange={(e) => setDraft({ ...draft, text: e.target.value })}
                 placeholder="root.from / context.foo" />
          <div className="vp-row" style={{ color: 'var(--cyan-700)' }}>
            동적 참조 — 평가 시점에 tx 컨텍스트에서 읽음
          </div>
        </>
      )}

      <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 8 }}>
        <button className="btn-secondary" onClick={commit}>적용</button>
      </div>
    </div>
  );
}

// ─── Guard block (a leaf condition, used by both flat + canvas) ────────────
function GuardBlockRow({ leaf, focused, matched, onFocus, onPatch, onDelete, locale }) {
  const [opOpen, setOpOpen] = useStateCV(false);
  const [valOpen, setValOpen] = useStateCV(false);
  const [noteEdit, setNoteEdit] = useStateCV(false);

  const sig = v2SigById(leaf.sigId);
  const color = v2ColorFor(leaf.sigId);
  const shape = sig ? sig.shape : 'rect';

  return (
    <div
      className={`bk-row ${focused ? 'focus' : ''} ${matched ? 'match' : ''}`}
      onClick={onFocus}
    >
      <div className="bk-grip" title="드래그 (시각만)">
        <span className="grip-d" /><span className="grip-d" />
        <span className="grip-d" /><span className="grip-d" />
        <span className="grip-d" /><span className="grip-d" />
      </div>

      <div className={`bk-shape color-${color} shape-${shape} ${leaf.custom ? 'dashed' : ''}`}
           onClick={(e) => e.stopPropagation()}>
        {/* segment chip — the field */}
        <span className={`seg-chip ${leaf.custom ? 'custom' : ''}`}>
          <span>{leaf.label}</span>
          <V2I.caretDown className="seg-caret" style={{ width: 10, height: 10 }} />
        </span>

        {/* operator */}
        <span className="op-pop">
          <button className="op-btn" onClick={(e) => { e.stopPropagation(); setOpOpen(o => !o); }}>
            <span>{leaf.operator}</span>
            <V2I.caretDown style={{ width: 10, height: 10 }} />
          </button>
          {opOpen && (
            <OpPopover leaf={leaf}
              onChange={(op) => onPatch({ operator: op })}
              onClose={() => setOpOpen(false)} />
          )}
        </span>

        {/* value */}
        <span className="val-pop">
          <button className={`val-chip kind-${leaf.value.kind}`}
                  onClick={(e) => { e.stopPropagation(); setValOpen(o => !o); }}>
            <span>{leaf.value.text}</span>
            {leaf.value.unit && <span className="val-u">{leaf.value.unit}</span>}
          </button>
          {valOpen && (
            <ValuePopover leaf={leaf}
              onChange={(v) => onPatch({ value: v })}
              onClose={() => setValOpen(false)} />
          )}
        </span>

        <span className="bk-meta">
          {leaf.custom && (
            <span className="abs-pill" title="값 부재 시 처리 (custom 신호)">
              <span className="abs-k">absence</span>
              <span className="abs-v">{leaf.absence}</span>
            </span>
          )}
          <span
            className="note-pill"
            contentEditable
            suppressContentEditableWarning
            onBlur={(e) => onPatch({ note: e.target.textContent || null })}
          >{leaf.note || '근거…'}</span>
        </span>
      </div>

      <button className="bk-x" onClick={(e) => { e.stopPropagation(); onDelete(); }} title="삭제">
        <V2I.x style={{ width: 13, height: 13 }} />
      </button>
    </div>
  );
}

// ─── OR / AND container box (canvas mode) ───────────────────────────────────
function ContainerBox({ node, focused, matched, onFocus, onPatch, onDelete,
                       onAddLeaf, onAddOR, onAddAND, dropTargetId, onDropOnContainer,
                       renderChild, depth = 0, orStyle = 'tinted', onRequestOR }) {
  const isOR = node.op === 'OR';
  const cls = isOR ? (orStyle === 'dashed' ? 'ctn ctn-or dashed' : 'ctn ctn-or') : 'ctn ctn-and';

  const handleDragOver = (e) => {
    if (e.dataTransfer.types.includes('text/v2-sig') || e.dataTransfer.types.includes('text/v2-node')) {
      e.preventDefault();
      e.dataTransfer.dropEffect = 'copy';
    }
  };
  const handleDrop = (e) => {
    const sigId = e.dataTransfer.getData('text/v2-sig');
    if (sigId) {
      e.preventDefault();
      e.stopPropagation();
      onDropOnContainer(node.id, sigId);
    }
  };

  return (
    <div className={cls} onDragOver={handleDragOver} onDrop={handleDrop}>
      <div className="ctn-h">
        <span className={`ctn-badge ${isOR ? 'is-or' : ''}`}>
          {isOR ? 'OR · 하나라도 참' : 'AND · 모두 참'}
        </span>
        <span className="ctn-meta">{node.children.length}개 조건</span>
        <span className="ctn-spacer" />
        <button className="ctn-add-btn" onClick={(e) => { e.stopPropagation(); onAddAND(node.id); }}>
          <V2I.plus style={{ width: 11, height: 11 }} /> AND 묶음
        </button>
        <button className="ctn-add-btn" onClick={(e) => { e.stopPropagation(); onRequestOR(node.id); }}>
          <V2I.plus style={{ width: 11, height: 11 }} /> OR 묶음
        </button>
        {depth > 0 && (
          <button className="ctn-x" onClick={(e) => { e.stopPropagation(); onDelete(node.id); }} title="컨테이너 삭제">
            <V2I.x style={{ width: 12, height: 12 }} />
          </button>
        )}
      </div>

      <div className="ctn-body">
        {node.children.length === 0 && (
          <div className="drop-zone">
            팔레트에서 블록을 드래그하거나 클릭으로 추가
          </div>
        )}
        {node.children.map(child => renderChild(child, node.id))}
      </div>
    </div>
  );
}

// ─── OR confirmation modal (A-3) ────────────────────────────────────────────
function ORConfirmModal({ onConfirm, onCancel, dontShowAgain, onSetDontShow }) {
  return (
    <div className="modal-scrim" onClick={onCancel}>
      <div className="or-modal" onClick={(e) => e.stopPropagation()}>
        <div className="or-modal-h">
          <div className="or-ic"><V2I.warn style={{ width: 18, height: 18 }} /></div>
          <div className="or-t">OR을 추가하면 괄호 우선순위는 직접 책임집니다</div>
        </div>
        <div className="or-modal-d">
          OR 컨테이너를 도입하면 조건 묶음의 <b>괄호 우선순위(parenthesization)</b>를
          직접 지정하게 됩니다. 운영자가 위치를 잘못 정하면 정책이 <b>조용히 의도와 다르게</b>
          평가될 수 있어요.
        </div>
        <div className="or-modal-eg">
          <span className="ok">A AND (B OR C)</span>  &nbsp; vs &nbsp;
          <span className="bad">(A AND B) OR C</span>
          <div style={{ fontFamily: 'var(--ff-sans)', fontSize: 11.5, color: 'var(--slate-500)', marginTop: 6 }}>
            같은 조건도 묶는 위치에 따라 통과/차단이 뒤집힙니다.
          </div>
        </div>
        <div className="or-modal-foot">
          <label className="or-skip">
            <input type="checkbox" checked={dontShowAgain} onChange={(e) => onSetDontShow(e.target.checked)} />
            <span>이 세션에서는 다시 묻지 않기</span>
          </label>
          <span className="or-spacer" />
          <button className="btn-secondary" onClick={onCancel}>취소</button>
          <button className="btn-primary on" onClick={onConfirm}>이해했어요 · OR 추가</button>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, {
  V2Palette, CanvasTrigger, CanvasDecision, GuardBlockRow, ContainerBox, ORConfirmModal,
});
