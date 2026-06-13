// editor-v3-canvas.jsx — Free canvas + Scratch-style blocks + drag system

const { useState: useS3, useEffect: useE3, useRef: useR3, useMemo: useM3, useCallback: useCb3, useLayoutEffect: useL3 } = React;

// ─────────── PALETTE ───────────
function V3Palette({ collapsed, onToggle, onAddSignal, locale, onDragStart, onDragEnd, draggingSig }) {
  const [q, setQ] = useS3('');
  const [openBase, setOpenBase] = useS3(true);
  const [openCustom, setOpenCustom] = useS3(true);

  if (collapsed) {
    return (
      <aside className="pane">
        <div className="coll-handle" onClick={onToggle}>
          <div className="ch-top">
            <div className="ch-arrow"><V3I.caretRight style={{ width: 12, height: 12 }} /></div>
            <span className="ch-lbl">Block palette</span>
          </div>
        </div>
      </aside>
    );
  }

  const filter = (items) => {
    if (!q) return items;
    const qq = q.toLowerCase();
    return items.filter(s => ((s.label[locale] || s.label.en).toLowerCase().includes(qq)) || s.id.toLowerCase().includes(qq));
  };

  return (
    <aside className="pane">
      <div className="pal-h">
        <span className="pal-t">Block palette</span>
        <button className="pal-fold" onClick={onToggle} title="패널 접기">
          <V3I.caretLeft style={{ width: 12, height: 12 }} />
        </button>
      </div>
      <div className="pal-search">
        <V3I.search style={{ width: 13, height: 13, color: 'var(--slate-400)' }} />
        <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="필드 · 경로 · 단위" />
        <span className="pal-kbd">/</span>
      </div>
      <div className="pal-scroll">
        <button className="pal-sect" onClick={() => setOpenBase(!openBase)}>
          <V3I.caretDown style={{ width: 11, height: 11, transform: openBase ? '' : 'rotate(-90deg)', transition: 'transform 120ms' }} />
          <span className="pal-sect-t">기본 필드 (calldata)</span>
          <span className="pal-sect-c">{V3_SIGS.base.length}</span>
        </button>
        {openBase && (
          <div className="pal-items">
            {filter(V3_SIGS.base).map(s => (
              <PalItem key={s.id} sig={s} locale={locale}
                onAdd={() => onAddSignal(s.id)}
                onDragStart={onDragStart} onDragEnd={onDragEnd} dragging={draggingSig === s.id} />
            ))}
          </div>
        )}
        <button className="pal-sect" onClick={() => setOpenCustom(!openCustom)}>
          <V3I.caretDown style={{ width: 11, height: 11, transform: openCustom ? '' : 'rotate(-90deg)', transition: 'transform 120ms' }} />
          <span className="pal-sect-t">커스텀 (manifest enrichment)</span>
          <span className="pal-sect-c">{V3_SIGS.custom.length}</span>
        </button>
        {openCustom && (
          <div className="pal-items">
            {filter(V3_SIGS.custom).map(s => (
              <PalItem key={s.id} sig={s} locale={locale}
                onAdd={() => onAddSignal(s.id)}
                onDragStart={onDragStart} onDragEnd={onDragEnd} dragging={draggingSig === s.id} />
            ))}
          </div>
        )}
      </div>
      <div className="pal-foot">
        <div style={{ fontFamily: 'var(--ff-mono)', fontSize: 10, fontWeight: 700, color: 'var(--slate-500)', letterSpacing: '0.06em', textTransform: 'uppercase', marginBottom: 6 }}>shape · type</div>
        <div className="pal-foot-row"><span className="pal-foot-sw s-pill" /><span>pill · 숫자</span></div>
        <div className="pal-foot-row"><span className="pal-foot-sw s-rect" /><span>rect · 주소/문자열</span></div>
        <div className="pal-foot-row"><span className="pal-foot-sw s-hex" /><span>hex · enum/bool</span></div>
        <div className="pal-foot-row"><span className="pal-foot-sw s-rect dashed" /><span>dashed · custom</span></div>
      </div>
    </aside>
  );
}

function PalItem({ sig, locale, onAdd, onDragStart, onDragEnd, dragging }) {
  const color = v3ColorFor(sig.id);
  const isCustom = !!sig.custom;
  return (
    <div className={`pal-it ${dragging ? 'drag' : ''}`} title={sig.id}
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData('text/v3-sig', sig.id);
        e.dataTransfer.effectAllowed = 'copy';
        onDragStart(sig.id);
      }}
      onDragEnd={onDragEnd}
      onClick={onAdd}
    >
      <span className={`pal-sw ${color} s-${sig.shape} ${isCustom ? 'dashed' : ''}`} />
      <span className="pal-lbl">{sig.label[locale] || sig.label.en}</span>
      <button className="pal-add" onClick={(e) => { e.stopPropagation(); onAdd(); }}>
        <V3I.plus style={{ width: 13, height: 13 }} />
      </button>
    </div>
  );
}

// ─────────── Operator popover ───────────
function OpPop({ leaf, onChange, onClose }) {
  const opts = V3_OPS[leaf.leafType] || ['==', '!='];
  const ref = useR3(null);
  useE3(() => {
    const h = (e) => { if (ref.current && !ref.current.contains(e.target)) onClose(); };
    setTimeout(() => window.addEventListener('mousedown', h), 0);
    return () => window.removeEventListener('mousedown', h);
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

// ─────────── Value popover ───────────
function ValPop({ leaf, onChange, onClose }) {
  const ref = useR3(null);
  const [draft, setDraft] = useS3(leaf.value);
  const commit = () => { onChange(draft); onClose(); };
  useE3(() => {
    const h = (e) => { if (ref.current && !ref.current.contains(e.target)) commit(); };
    setTimeout(() => window.addEventListener('mousedown', h), 0);
    return () => window.removeEventListener('mousedown', h);
  }, [draft]);
  const sig = v3SigById(leaf.sigId);
  const supportsRef = ['address', 'tokenNative', 'usd', 'number'].includes(leaf.leafType);

  return (
    <div className="val-pop-card" ref={ref}>
      <div className="vp-h">값 · {leaf.leafType}</div>
      {supportsRef && (
        <div className="vp-tab-row">
          <button className={draft.kind === 'ref' ? '' : 'on'}
            onClick={() => setDraft({ kind: 'num', text: draft.kind === 'num' ? draft.text : '0', unit: draft.unit })}>리터럴</button>
          <button className={draft.kind === 'ref' ? 'on' : ''}
            onClick={() => setDraft({ kind: 'ref', text: draft.kind === 'ref' ? draft.text : 'root.from' })}>ref (동적 참조)</button>
        </div>
      )}
      {draft.kind === 'bool' && (
        <div className="vp-toggle">
          <button className={draft.text === 'true' ? 'on' : ''} onClick={() => setDraft({ ...draft, text: 'true' })}>true</button>
          <button className={draft.text === 'false' ? 'on' : ''} onClick={() => setDraft({ ...draft, text: 'false' })}>false</button>
        </div>
      )}
      {draft.kind === 'enum' && sig && sig.options && (
        <select className="vp-input" value={draft.text} onChange={(e) => setDraft({ ...draft, text: e.target.value })}>
          {sig.options.map(o => <option key={o} value={o}>{o}</option>)}
        </select>
      )}
      {draft.kind === 'num' && (
        <>
          <input className="vp-input" type="text" autoFocus value={draft.text}
            onChange={(e) => setDraft({ ...draft, text: e.target.value })}
            onKeyDown={(e) => e.key === 'Enter' && commit()} />
          {draft.unit && <div className="vp-row">단위: <span style={{ fontFamily: 'var(--ff-mono)', color: 'var(--slate-700)' }}>{draft.unit}</span></div>}
        </>
      )}
      {draft.kind === 'ref' && (
        <>
          <input className="vp-input" type="text" autoFocus value={draft.text}
            onChange={(e) => setDraft({ ...draft, text: e.target.value })} placeholder="root.from / context.foo" />
          <div className="vp-row" style={{ color: 'var(--cyan-700)' }}>동적 참조 — 평가 시점에 tx 컨텍스트에서 읽음</div>
        </>
      )}
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 8 }}>
        <button className="btn-secondary" onClick={commit}>적용</button>
      </div>
    </div>
  );
}

// ─────────── LEAF BLOCK ───────────
// Used both inside a container and free-floating on canvas.
// `inContainer` true → no absolute positioning; parent controls layout.
function LeafBlock({ node, focused, matched, isDraft, inContainer,
                     onFocus, onPatch, onDelete, onPointerDown }) {
  const [opOpen, setOpOpen] = useS3(false);
  const [valOpen, setValOpen] = useS3(false);

  const sig = v3SigById(node.sigId);
  const color = v3ColorFor(node.sigId);
  const shape = sig ? sig.shape : 'rect';

  const positionStyle = inContainer ? {} : { left: node.x, top: node.y };

  return (
    <div
      className={`bk ${isDraft ? 'draft' : ''}`}
      style={positionStyle}
      onPointerDown={(e) => {
        // Only start drag on shape body (not on inner editable chips)
        if (e.target.closest('.op-btn,.val-chip,.note-pill,.bk-x,.pop,.val-pop-card')) return;
        onPointerDown(e, node.id);
      }}
      onClick={(e) => { e.stopPropagation(); onFocus(); }}
    >
      <div
        className={`bk-shape color-${color} s-${shape} ${node.custom ? 'dashed' : ''} ${focused ? 'focus' : ''} ${matched ? 'match' : ''}`}
      >
        <span className={`seg-chip ${node.custom ? 'custom' : ''}`}>
          <span>{node.label}</span>
          <V3I.caretDown className="sc-c" style={{ width: 10, height: 10 }} />
        </span>

        <span style={{ position: 'relative', display: 'inline-flex' }}>
          <button className="op-btn" onClick={(e) => { e.stopPropagation(); setOpOpen(o => !o); }}>
            <span>{node.operator}</span>
            <V3I.caretDown style={{ width: 10, height: 10 }} />
          </button>
          {opOpen && <OpPop leaf={node} onChange={(op) => onPatch({ operator: op })} onClose={() => setOpOpen(false)} />}
        </span>

        <span style={{ position: 'relative', display: 'inline-flex' }}>
          <button className={`val-chip k-${node.value.kind}`} onClick={(e) => { e.stopPropagation(); setValOpen(o => !o); }}>
            <span>{node.value.text}</span>
            {node.value.unit && <span className="val-u">{node.value.unit}</span>}
          </button>
          {valOpen && <ValPop leaf={node} onChange={(v) => onPatch({ value: v })} onClose={() => setValOpen(false)} />}
        </span>

        <span className="bk-meta">
          {node.custom && (
            <span className="abs-pill" title="값 부재 시 처리"><span className="abs-k">absence</span><span className="abs-v">{node.absence}</span></span>
          )}
          <span className="note-pill" contentEditable suppressContentEditableWarning
            onBlur={(e) => onPatch({ note: e.target.textContent || null })}>
            {node.note || '근거…'}
          </span>
          <button className="bk-x" onClick={(e) => { e.stopPropagation(); onDelete(); }} title="삭제">
            <V3I.x style={{ width: 12, height: 12 }} />
          </button>
        </span>
      </div>
    </div>
  );
}

// ─────────── CONTAINER BLOCK ───────────
// Root container is rendered with this too (with `isRoot` true → no delete, drag header still works).
function ContainerBlock({ node, isRoot, dropTarget, focusedLeafId, matchedLeafIds,
                         renderChild, onFocusLeaf, onPatchLeaf, onDeleteNode, onPointerDownNode,
                         onPointerDownChild, onPatchCont, onRequestOR, onAddANDInside }) {
  const isOR = node.op === 'OR';
  return (
    <div
      className={`bk bk-container ${isOR ? 'is-or' : 'is-and'}`}
      style={isRoot ? { left: node.x, top: node.y, width: node.w || 760 } : {}}
      data-cont-id={node.id}
      onClick={(e) => e.stopPropagation()}
    >
      <div className="ctn-hd" onPointerDown={(e) => onPointerDownNode(e, node.id)}>
        <span className="ctn-op">{isOR ? 'OR · 하나라도 참' : 'AND · 모두 참'}</span>
        <span className="ctn-d">{node.childIds.length}개 조건 {isRoot ? '· 정책 루트' : ''}</span>
        <span className="ctn-spc" />
        <button className="ctn-flip" onClick={(e) => { e.stopPropagation(); onPatchCont(node.id, { op: isOR ? 'AND' : 'OR' }); }} title="조합자 전환">
          → {isOR ? 'AND' : 'OR'}
        </button>
        {!isRoot && (
          <button className="ctn-x" onClick={(e) => { e.stopPropagation(); onDeleteNode(node.id); }} title="컨테이너 삭제">
            <V3I.x style={{ width: 12, height: 12 }} />
          </button>
        )}
      </div>
      <div className={`ctn-body ${dropTarget ? 'drop-target' : ''}`}
           data-drop-cont={node.id}>
        {node.childIds.length === 0 && (
          <div style={{
            padding: '14px', textAlign: 'center', fontSize: 11.5,
            color: 'var(--slate-400)', fontFamily: 'var(--ff-mono)',
          }}>
            여기에 블록을 끌어다 놓으세요 (자석처럼 붙습니다)
          </div>
        )}
        {node.childIds.map(cid => renderChild(cid))}
        {isRoot && (
          <div style={{ display: 'flex', gap: 6, marginTop: 4 }}>
            <button className="cv-add-btn" onClick={(e) => { e.stopPropagation(); onAddANDInside(node.id); }}>
              <V3I.plus style={{ width: 11, height: 11 }} /> AND 묶음
            </button>
            <button className="cv-add-btn or" onClick={(e) => { e.stopPropagation(); onRequestOR(node.id); }}>
              <V3I.plus style={{ width: 11, height: 11 }} /> OR 묶음
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

// ─────────── DRAG SYSTEM HOOK ───────────
// Manages pointer-down/move/up across the canvas.
// onMove(id, x, y, hoverContainerId) called continuously.
// onEnd(id, x, y, dropContainerId|null) called on pointer up.
function useDragSystem({ stageRef, getNodePos, onMove, onEnd }) {
  const drag = useR3(null); // {id, offX, offY, startX, startY}
  const hoverRef = useR3(null);

  const start = useCb3((e, id) => {
    const stage = stageRef.current;
    if (!stage) return;
    const stageRect = stage.getBoundingClientRect();
    const scrollX = stage.scrollLeft;
    const scrollY = stage.scrollTop;
    const np = getNodePos(id);
    if (!np) return;
    const pointerStageX = e.clientX - stageRect.left + scrollX;
    const pointerStageY = e.clientY - stageRect.top + scrollY;
    drag.current = {
      id,
      offX: pointerStageX - np.x,
      offY: pointerStageY - np.y,
      stageRect,
      startTime: Date.now(),
      startClient: { x: e.clientX, y: e.clientY },
      moved: false,
    };
    document.body.style.cursor = 'grabbing';
    e.preventDefault();
  }, [stageRef, getNodePos]);

  useE3(() => {
    const move = (e) => {
      if (!drag.current) return;
      const d = drag.current;
      const stage = stageRef.current;
      if (!stage) return;
      const dx = e.clientX - d.startClient.x;
      const dy = e.clientY - d.startClient.y;
      if (Math.abs(dx) > 3 || Math.abs(dy) > 3) d.moved = true;
      const stageRect = stage.getBoundingClientRect();
      const scrollX = stage.scrollLeft;
      const scrollY = stage.scrollTop;
      const pointerStageX = e.clientX - stageRect.left + scrollX;
      const pointerStageY = e.clientY - stageRect.top + scrollY;
      const nx = Math.max(0, pointerStageX - d.offX);
      const ny = Math.max(0, pointerStageY - d.offY);
      // Detect hover container
      const elsUnder = document.elementsFromPoint(e.clientX, e.clientY);
      let hoverCont = null;
      for (const el of elsUnder) {
        if (el.dataset && el.dataset.dropCont) { hoverCont = el.dataset.dropCont; break; }
      }
      hoverRef.current = hoverCont;
      onMove(d.id, nx, ny, hoverCont);
    };
    const up = (e) => {
      if (!drag.current) return;
      const d = drag.current;
      document.body.style.cursor = '';
      const stage = stageRef.current;
      let nx = 0, ny = 0, hoverCont = null;
      if (stage) {
        const stageRect = stage.getBoundingClientRect();
        const pointerStageX = e.clientX - stageRect.left + stage.scrollLeft;
        const pointerStageY = e.clientY - stageRect.top + stage.scrollTop;
        nx = Math.max(0, pointerStageX - d.offX);
        ny = Math.max(0, pointerStageY - d.offY);
        const elsUnder = document.elementsFromPoint(e.clientX, e.clientY);
        for (const el of elsUnder) {
          if (el.dataset && el.dataset.dropCont) { hoverCont = el.dataset.dropCont; break; }
        }
      }
      onEnd(d.id, nx, ny, hoverCont, d.moved);
      drag.current = null;
      hoverRef.current = null;
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    return () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
  }, [stageRef, onMove, onEnd]);

  return { startDrag: start };
}

// ─────────── FREE CANVAS PANE ───────────
function V3Canvas({
  state, draftIds, dispatch, focusedLeafId, setFocusedLeafId, matchedLeafIds,
  onRequestOR, onAddANDInside, gridStrength,
}) {
  const stageRef = useR3(null);
  const innerRef = useR3(null);
  const [dragOverCont, setDragOverCont] = useS3(null);
  const [livePos, setLivePos] = useS3(null); // { id, x, y } during drag
  const [zoom, setZoom] = useS3(1);

  const getNodePos = useCb3((id) => {
    if (livePos && livePos.id === id) return { x: livePos.x, y: livePos.y };
    const n = state.nodes[id];
    return n ? { x: n.x, y: n.y } : null;
  }, [state, livePos]);

  const onMove = useCb3((id, x, y, hoverCont) => {
    setLivePos({ id, x, y });
    // Only highlight container if it's a different one than current parent
    const node = state.nodes[id];
    const isSelf = hoverCont === id;
    const isOwnAncestor = (hoverCont && node) ? (function check(cid) {
      let cur = state.nodes[cid];
      while (cur) { if (cur.id === id) return true; cur = state.nodes[cur.parentId]; }
      return false;
    })(hoverCont) : false;
    setDragOverCont(isSelf || isOwnAncestor ? null : hoverCont);
  }, [state]);

  const onEnd = useCb3((id, x, y, hoverCont, moved) => {
    setLivePos(null);
    setDragOverCont(null);
    if (!moved) return; // it was a click
    const node = state.nodes[id];
    if (!node) return;
    const isSelf = hoverCont === id;
    const isOwnDesc = hoverCont ? (function check(cid) {
      let cur = state.nodes[cid];
      while (cur) { if (cur.id === id) return true; cur = state.nodes[cur.parentId]; }
      return false;
    })(hoverCont) : false;
    if (hoverCont && !isSelf && !isOwnDesc) {
      // ATTACH to container (or reorder)
      dispatch({ type: 'ATTACH', id, parentId: hoverCont });
      // If root container, also store position update so future drag-out remembers
      dispatch({ type: 'MOVE', id, x, y });
    } else {
      // DETACH to canvas at (x, y) — becomes draft
      if (node.parentId !== null) {
        dispatch({ type: 'DETACH', id, x, y });
      } else {
        // Just move free-positioned node
        dispatch({ type: 'MOVE', id, x, y });
      }
    }
  }, [state, dispatch]);

  const { startDrag } = useDragSystem({ stageRef, getNodePos, onMove, onEnd });

  // Drop from palette
  const handleStageDrop = (e) => {
    const sig = e.dataTransfer.getData('text/v3-sig');
    if (!sig) return;
    e.preventDefault();
    const stageRect = stageRef.current.getBoundingClientRect();
    const x = e.clientX - stageRect.left + stageRef.current.scrollLeft;
    const y = e.clientY - stageRect.top + stageRef.current.scrollTop;
    // Detect if drop is inside a container body
    const elsUnder = document.elementsFromPoint(e.clientX, e.clientY);
    let cont = null;
    for (const el of elsUnder) { if (el.dataset && el.dataset.dropCont) { cont = el.dataset.dropCont; break; } }
    if (cont) {
      dispatch({ type: 'ADD_FROM_PALETTE', sigId: sig, parentId: cont });
    } else {
      dispatch({ type: 'ADD_FROM_PALETTE', sigId: sig, parentId: null, x: x - 80, y: y - 20 });
    }
  };

  // Render a child (recursive)
  const renderNode = (id) => {
    const node = state.nodes[id];
    if (!node) return null;
    const pos = getNodePos(id);
    const isDrag = livePos && livePos.id === id;
    const onPointerDownNode = (e, nid) => startDrag(e, nid);

    if (node.kind === 'leaf') {
      // If has parent (not root) and parent isn't being shown as floating, render inline (parent layout)
      const isFloating = node.parentId === null;
      if (isFloating) {
        return (
          <div
            key={id}
            className={`bk ${isDrag ? 'dragging' : ''} ${node.parentId === null ? 'draft' : ''}`}
            style={{ left: pos.x, top: pos.y }}
            onPointerDown={(e) => {
              if (e.target.closest('.op-btn,.val-chip,.note-pill,.bk-x,.pop,.val-pop-card,.seg-chip')) return;
              startDrag(e, id);
            }}
            onClick={(e) => { e.stopPropagation(); setFocusedLeafId(id); }}
          >
            <LeafContent node={node} focused={focusedLeafId === id} matched={matchedLeafIds.includes(id)}
              onPatch={(p) => dispatch({ type: 'PATCH_LEAF', id, patch: p })}
              onDelete={() => dispatch({ type: 'DELETE', id })} />
          </div>
        );
      }
      // Inline (inside a container)
      return (
        <div key={id} className={`bk ${isDrag ? 'dragging' : ''}`}
          onPointerDown={(e) => {
            if (e.target.closest('.op-btn,.val-chip,.note-pill,.bk-x,.pop,.val-pop-card,.seg-chip')) return;
            startDrag(e, id);
          }}
          onClick={(e) => { e.stopPropagation(); setFocusedLeafId(id); }}
        >
          <LeafContent node={node} focused={focusedLeafId === id} matched={matchedLeafIds.includes(id)}
            onPatch={(p) => dispatch({ type: 'PATCH_LEAF', id, patch: p })}
            onDelete={() => dispatch({ type: 'DELETE', id })} />
        </div>
      );
    }

    // Container
    if (id === state.rootId) {
      return (
        <ContainerBlock key={id} node={node} isRoot
          dropTarget={dragOverCont === id}
          focusedLeafId={focusedLeafId} matchedLeafIds={matchedLeafIds}
          renderChild={renderNode}
          onPointerDownNode={onPointerDownNode}
          onPatchCont={(nid, p) => dispatch({ type: 'PATCH_CONT', id: nid, patch: p })}
          onDeleteNode={(nid) => dispatch({ type: 'DELETE', id: nid })}
          onRequestOR={onRequestOR}
          onAddANDInside={onAddANDInside}
        />
      );
    }
    // Sub-container — could be floating draft or nested in another
    const wrapStyle = node.parentId === null ? { position: 'absolute', left: pos.x, top: pos.y } : {};
    return (
      <div key={id} className={`bk ${isDrag ? 'dragging' : ''} ${node.parentId === null ? 'draft' : ''}`} style={wrapStyle}>
        <ContainerBlock node={node}
          dropTarget={dragOverCont === id}
          focusedLeafId={focusedLeafId} matchedLeafIds={matchedLeafIds}
          renderChild={renderNode}
          onPointerDownNode={onPointerDownNode}
          onPatchCont={(nid, p) => dispatch({ type: 'PATCH_CONT', id: nid, patch: p })}
          onDeleteNode={(nid) => dispatch({ type: 'DELETE', id: nid })}
          onRequestOR={onRequestOR}
          onAddANDInside={onAddANDInside}
        />
      </div>
    );
  };

  // Compute lists: root + draft top-level nodes (parentId === null && id !== rootId)
  const topLevel = useM3(() => {
    const out = [state.rootId];
    Object.values(state.nodes).forEach(n => {
      if (n.id !== state.rootId && n.parentId === null) out.push(n.id);
    });
    return out;
  }, [state.nodes, state.rootId]);

  return (
    <div className="pane cv-pane">
      <div className="cv-tools">
        <div className={`cv-chip draft-count ${draftIds.length ? 'has' : ''}`}>
          <span className="ct-d" />
          <span className="ct-k">미연결</span>
          <span className="ct-n">{draftIds.length}개</span>
          <span style={{ fontSize: 11 }}>{draftIds.length ? '· Cedar에서 제외됨' : '· 모두 연결됨'}</span>
        </div>
        <div className="cv-chip">
          <span className="ct-k">조건 in 정책</span>
          <span className="ct-n">{Object.values(state.nodes).filter(n => n.kind === 'leaf' && v3InPolicy(state, n.id)).length}</span>
        </div>
        <div className="cv-zoom">
          <button onClick={() => setZoom(z => Math.max(0.5, z - 0.1))}>−</button>
          <span className="cv-zoom-pct">{Math.round(zoom * 100)}%</span>
          <button onClick={() => setZoom(z => Math.min(1.5, z + 0.1))}>+</button>
        </div>
      </div>

      <div
        ref={stageRef}
        className={`cv-stage ${gridStrength === 'subtle' ? 'grid-subtle' : gridStrength === 'strong' ? 'grid-strong' : ''}`}
        onDragOver={(e) => { if (e.dataTransfer.types.includes('text/v3-sig')) e.preventDefault(); }}
        onDrop={handleStageDrop}
        onClick={() => setFocusedLeafId(null)}
      >
        <div ref={innerRef} className="cv-stage-inner" style={{ transform: `scale(${zoom})`, paddingTop: 44 }}>
          {/* Draft region label */}
          <div className="cv-draft-region" />
          {topLevel.map(id => renderNode(id))}
        </div>
      </div>
    </div>
  );
}

// Extract the leaf content (chips inside the colored shape) for reuse.
function LeafContent({ node, focused, matched, onPatch, onDelete }) {
  const sig = v3SigById(node.sigId);
  const color = v3ColorFor(node.sigId);
  const shape = sig ? sig.shape : 'rect';
  const [opOpen, setOpOpen] = useS3(false);
  const [valOpen, setValOpen] = useS3(false);

  return (
    <div className={`bk-shape color-${color} s-${shape} ${node.custom ? 'dashed' : ''} ${focused ? 'focus' : ''} ${matched ? 'match' : ''}`}>
      <span className={`seg-chip ${node.custom ? 'custom' : ''}`}>
        <span>{node.label}</span>
        <V3I.caretDown className="sc-c" style={{ width: 10, height: 10 }} />
      </span>
      <span style={{ position: 'relative', display: 'inline-flex' }}>
        <button className="op-btn" onClick={(e) => { e.stopPropagation(); setOpOpen(o => !o); }}>
          <span>{node.operator}</span>
          <V3I.caretDown style={{ width: 10, height: 10 }} />
        </button>
        {opOpen && <OpPop leaf={node} onChange={(op) => onPatch({ operator: op })} onClose={() => setOpOpen(false)} />}
      </span>
      <span style={{ position: 'relative', display: 'inline-flex' }}>
        <button className={`val-chip k-${node.value.kind}`} onClick={(e) => { e.stopPropagation(); setValOpen(o => !o); }}>
          <span>{node.value.text}</span>
          {node.value.unit && <span className="val-u">{node.value.unit}</span>}
        </button>
        {valOpen && <ValPop leaf={node} onChange={(v) => onPatch({ value: v })} onClose={() => setValOpen(false)} />}
      </span>
      <span className="bk-meta">
        {node.custom && (
          <span className="abs-pill" title="값 부재 시 처리"><span className="abs-k">absence</span><span className="abs-v">{node.absence}</span></span>
        )}
        <span className="note-pill" contentEditable suppressContentEditableWarning
          onBlur={(e) => onPatch({ note: e.target.textContent || null })}>
          {node.note || '근거…'}
        </span>
        <button className="bk-x" onClick={(e) => { e.stopPropagation(); onDelete(); }} title="삭제">
          <V3I.x style={{ width: 12, height: 12 }} />
        </button>
      </span>
    </div>
  );
}

// ─────────── OR confirm modal ───────────
function V3ORConfirmModal({ onConfirm, onCancel, dontShowAgain, onSetDontShow }) {
  return (
    <div className="scrim" onClick={onCancel}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-h">
          <div className="m-ic"><V3I.warn style={{ width: 18, height: 18 }} /></div>
          <div className="m-t">OR을 추가하면 괄호 우선순위는 직접 책임집니다</div>
        </div>
        <div className="modal-d">
          OR 컨테이너를 도입하면 조건 묶음의 <b>괄호 우선순위(parenthesization)</b>를 직접 지정하게 됩니다.
          운영자가 위치를 잘못 정하면 정책이 <b>조용히 의도와 다르게</b> 평가될 수 있어요.
        </div>
        <div className="modal-eg">
          <span className="ok">A AND (B OR C)</span> &nbsp; vs &nbsp; <span className="bad">(A AND B) OR C</span>
          <div style={{ fontFamily: 'var(--ff-sans)', fontSize: 11.5, color: 'var(--slate-500)', marginTop: 6 }}>
            같은 조건도 묶는 위치에 따라 통과/차단이 뒤집힙니다.
          </div>
        </div>
        <div className="modal-foot">
          <label>
            <input type="checkbox" checked={dontShowAgain} onChange={(e) => onSetDontShow(e.target.checked)} />
            <span>이 세션에서는 다시 묻지 않기</span>
          </label>
          <span className="spc" />
          <button className="btn-secondary" onClick={onCancel}>취소</button>
          <button className="btn-primary on" onClick={onConfirm}>이해했어요 · OR 추가</button>
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { V3Palette, V3Canvas, V3ORConfirmModal });
