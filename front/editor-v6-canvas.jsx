// editor-v6-canvas.jsx — free canvas with styled blocks, selection → inspector,
// right-click context menu (logic + quick style), and a logic toolbar.
// Palette lives in its own file now; this canvas accepts 'text/v6-block' drops.

const { useState: useSCv, useRef: useRCv, useEffect: useECv, useMemo: useMCv, useCallback: useCbCv } = React;

// ── drag hooks (ported from v5 canvas; self-contained) ──
function useNodeDrag6({ scrollRef, zoomRef, getNodePos, onMove, onEnd }) {
  const drag = useRCv(null);
  const start = useCbCv((e, id) => {
    const stage = scrollRef.current; if (!stage) return;
    const rect = stage.getBoundingClientRect();
    const z = zoomRef.current || 1;
    const np = getNodePos(id); if (!np) return;
    const wx = (e.clientX - rect.left + stage.scrollLeft) / z;
    const wy = (e.clientY - rect.top + stage.scrollTop) / z;
    drag.current = { id, offX: wx - np.x, offY: wy - np.y, startClient: { x: e.clientX, y: e.clientY }, moved: false };
    document.body.style.cursor = 'grabbing';
    e.preventDefault();
  }, [scrollRef, zoomRef, getNodePos]);

  useECv(() => {
    const move = (e) => {
      if (!drag.current) return;
      const d = drag.current; const stage = scrollRef.current; if (!stage) return;
      const dx = e.clientX - d.startClient.x, dy = e.clientY - d.startClient.y;
      if (Math.abs(dx) > 3 || Math.abs(dy) > 3) d.moved = true;
      const rect = stage.getBoundingClientRect(); const z = zoomRef.current || 1;
      const wx = (e.clientX - rect.left + stage.scrollLeft) / z;
      const wy = (e.clientY - rect.top + stage.scrollTop) / z;
      const nx = Math.max(0, wx - d.offX), ny = Math.max(0, wy - d.offY);
      let hover = null;
      const els = document.elementsFromPoint(e.clientX, e.clientY);
      for (const el of els) { if (el.dataset && el.dataset.dropGrp) { hover = el.dataset.dropGrp; break; } }
      onMove(d.id, nx, ny, hover);
    };
    const up = (e) => {
      if (!drag.current) return;
      const d = drag.current; document.body.style.cursor = '';
      const stage = scrollRef.current; let nx = 0, ny = 0, hover = null;
      if (stage) {
        const rect = stage.getBoundingClientRect(); const z = zoomRef.current || 1;
        const wx = (e.clientX - rect.left + stage.scrollLeft) / z;
        const wy = (e.clientY - rect.top + stage.scrollTop) / z;
        nx = Math.max(0, wx - d.offX); ny = Math.max(0, wy - d.offY);
        const els = document.elementsFromPoint(e.clientX, e.clientY);
        for (const el of els) { if (el.dataset && el.dataset.dropGrp) { hover = el.dataset.dropGrp; break; } }
      }
      onEnd(d.id, nx, ny, hover, d.moved);
      drag.current = null;
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    return () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up); };
  }, [scrollRef, zoomRef, onMove, onEnd]);
  return { startNodeDrag: start };
}

function useWireDrag6({ scrollRef, zoomRef, onEnd }) {
  const drag = useRCv(null);
  const [tempWire, setTempWire] = useSCv(null);
  const start = useCbCv((e, fx, fy) => {
    drag.current = { fromX: fx, fromY: fy };
    setTempWire({ fromX: fx, fromY: fy, x: fx, y: fy, targetId: null });
    document.body.style.cursor = 'grabbing'; e.preventDefault();
  }, []);
  useECv(() => {
    const move = (e) => {
      if (!drag.current) return;
      const stage = scrollRef.current; if (!stage) return;
      const rect = stage.getBoundingClientRect(); const z = zoomRef.current || 1;
      const wx = (e.clientX - rect.left + stage.scrollLeft) / z;
      const wy = (e.clientY - rect.top + stage.scrollTop) / z;
      let targetId = null;
      const els = document.elementsFromPoint(e.clientX, e.clientY);
      for (const el of els) { if (el.dataset && el.dataset.sockIn) { targetId = el.dataset.sockIn; break; } }
      setTempWire({ fromX: drag.current.fromX, fromY: drag.current.fromY, x: wx, y: wy, targetId });
    };
    const up = (e) => {
      if (!drag.current) return;
      document.body.style.cursor = '';
      let targetId = null;
      const els = document.elementsFromPoint(e.clientX, e.clientY);
      for (const el of els) { if (el.dataset && el.dataset.sockIn) { targetId = el.dataset.sockIn; break; } }
      onEnd(targetId); drag.current = null; setTempWire(null);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    return () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up); };
  }, [scrollRef, zoomRef, onEnd]);
  return { startWireDrag: start, tempWire };
}

function wirePath6(x1, y1, x2, y2) {
  const dx = Math.max(40, Math.abs(x2 - x1) * 0.4);
  return `M ${x1},${y1} C ${x1 + dx},${y1} ${x2 - dx},${y2} ${x2},${y2}`;
}
function isAndNext6(c) { return c === 'AND' ? 'OR' : c === 'OR' ? 'NOT' : 'AND'; }

// ── styled-block helpers ──
function bkVars(node) {
  const r = v6ResolveStyle(node.style, v6DomainOf(node));
  const vars = { '--bk-fill': r.fill, '--bk-border': r.border, '--bk-text': r.text };
  if (r.sev) vars['--bk-sev'] = r.sev.edge;
  return { vars, r };
}
// severity corner pill (status color) or neutral user-tag — never on the fill (README §9.4)
function bkTag(r) {
  if (!r.sev && !r.tag) return null;
  return <span className={`bk-tag ${r.sev ? 'sev-' + r.tone : 'neutral'}`}>{r.tag || (r.sev ? r.sev.label : '')}</span>;
}

function V6Canvas({
  state, dispatch,
  focusedLeafId, setFocusedLeafId, matchedLeafIds,
  selectedId, setSelectedId, onOpenInspector,
  zoom, setZoom, gridStrength,
  onRequestOR,
}) {
  const scrollRef = useRCv(null);
  const zoomRef = useRCv(zoom);
  useECv(() => { zoomRef.current = zoom; }, [zoom]);

  const [livePos, setLivePos] = useSCv(null);
  const [dropTarget, setDropTarget] = useSCv(null);
  const [menu, setMenu] = useSCv(null); // { type, id?, x, y, wx, wy }

  useECv(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const t = setTimeout(() => { window.addEventListener('mousedown', close); window.addEventListener('scroll', close, true); }, 0);
    return () => { clearTimeout(t); window.removeEventListener('mousedown', close); window.removeEventListener('scroll', close, true); };
  }, [menu]);

  const getNodePos = useCbCv((id) => {
    if (livePos && livePos.id === id) return { x: livePos.x, y: livePos.y };
    const n = state.nodes[id]; return n ? { x: n.x, y: n.y } : null;
  }, [state, livePos]);

  const onMove = useCbCv((id, x, y, hoverGrp) => {
    setLivePos({ id, x, y });
    const self = hoverGrp === id;
    const isOwnDesc = (hoverGrp) ? (() => { let cur = state.nodes[hoverGrp]; while (cur) { if (cur.id === id) return true; cur = state.nodes[cur.parentId]; } return false; })() : false;
    setDropTarget(self || isOwnDesc ? null : hoverGrp);
  }, [state]);
  const onEnd = useCbCv((id, x, y, hoverGrp, moved) => {
    setLivePos(null); setDropTarget(null);
    if (!moved) return;
    const node = state.nodes[id]; if (!node) return;
    const self = hoverGrp === id;
    const isOwnDesc = hoverGrp ? (() => { let cur = state.nodes[hoverGrp]; while (cur) { if (cur.id === id) return true; cur = state.nodes[cur.parentId]; } return false; })() : false;
    if (hoverGrp && !self && !isOwnDesc) dispatch({ type: 'ATTACH_TO_GROUP', id, parentId: hoverGrp });
    else { if (node.parentId) dispatch({ type: 'DETACH_TO_TOP', id, x, y }); else dispatch({ type: 'MOVE', id, x, y }); }
  }, [state, dispatch]);
  const { startNodeDrag } = useNodeDrag6({ scrollRef, zoomRef, getNodePos, onMove, onEnd });

  const wireEnd = useCbCv((targetId) => { if (targetId) dispatch({ type: 'ADD_WIRE', to: targetId }); }, [dispatch]);
  const { startWireDrag, tempWire } = useWireDrag6({ scrollRef, zoomRef, onEnd: wireEnd });

  const ROOT_OUT_DX = 150, ROOT_OUT_DY = 20, NODE_IN_DX = -10, NODE_IN_DY = 22;
  const root = state.nodes[state.rootId];
  const topNodes = useMCv(() => Object.values(state.nodes).filter(n => n.id !== state.rootId && !n.parentId), [state.nodes]);
  const wiredTos = useMCv(() => new Set(state.wires.map(w => w.to)), [state.wires]);

  const select = (e, id) => { e.stopPropagation(); setSelectedId(id); setFocusedLeafId(id); };

  // world coords from a client point
  const toWorld = (clientX, clientY) => {
    const stage = scrollRef.current; const rect = stage.getBoundingClientRect(); const z = zoomRef.current || 1;
    return { x: (clientX - rect.left + stage.scrollLeft) / z, y: (clientY - rect.top + stage.scrollTop) / z };
  };

  const onPaletteDrop = (e) => {
    const raw = e.dataTransfer.getData('text/v6-block');
    if (!raw) return;
    e.preventDefault();
    let def; try { def = JSON.parse(raw); } catch { return; }
    const w = toWorld(e.clientX, e.clientY);
    let parentId = null;
    const els = document.elementsFromPoint(e.clientX, e.clientY);
    for (const el of els) { if (el.dataset && el.dataset.dropGrp) { parentId = el.dataset.dropGrp; break; } }
    dispatch({ type: 'ADD_SCHEMA_BLOCK', def, parentId, x: w.x - 80, y: w.y - 20 });
  };

  const onWheel = (e) => {
    if (e.ctrlKey || e.metaKey) { e.preventDefault(); const delta = e.deltaY > 0 ? -0.08 : 0.08; setZoom(z => Math.max(0.25, Math.min(4, z + delta))); }
  };

  const onCanvasCtx = (e) => {
    if (e.target.closest('.node, .root-node')) return;
    e.preventDefault();
    const w = toWorld(e.clientX, e.clientY);
    setMenu({ type: 'canvas', x: e.clientX, y: e.clientY, wx: w.x, wy: w.y });
  };
  const onNodeCtx = (e, id) => { e.preventDefault(); e.stopPropagation(); setSelectedId(id); setMenu({ type: 'node', id, x: e.clientX, y: e.clientY }); };

  // logic toolbar add — place at a visible spot
  const addLogicVisible = (kind) => {
    const stage = scrollRef.current; const z = zoomRef.current || 1;
    const wx = (stage.scrollLeft + 260) / z, wy = (stage.scrollTop + 150) / z;
    if (kind === 'OR') onRequestOR({ x: wx, y: wy, parentId: null });
    else dispatch({ type: 'ADD_GROUP', combinator: kind, parentId: null, x: wx, y: wy });
  };
  const addLogicAt = (kind, wx, wy) => {
    if (kind === 'OR') onRequestOR({ x: wx, y: wy, parentId: null });
    else dispatch({ type: 'ADD_GROUP', combinator: kind, parentId: null, x: wx, y: wy });
    setMenu(null);
  };

  // ── condition body w/ style ──
  const conditionBody = (node, inline) => {
    const { vars, r } = bkVars(node);
    return (
      <div className={`body styled shape-${r.shape}${r.tone ? ' sev-' + r.tone : ''}`} style={vars}
        onPointerDown={(e) => {
          if (e.target.closest('.op-btn,.val-chip,.note-pill,.node-x,.pop,.val-pop-card,.seg-chip,.socket,.abs-pill')) return;
          startNodeDrag(e, node.id);
        }}>
        <span className="bk-cap" />
        <V6ConditionBody node={node}
          onPatch={(p) => dispatch({ type: 'PATCH_CONDITION', id: node.id, patch: p })}
          onDelete={() => dispatch({ type: 'DELETE', id: node.id })} />
      </div>
    );
  };

  const renderTop = (node) => {
    const pos = getNodePos(node.id);
    const isWired = wiredTos.has(node.id);
    const isDrag = livePos && livePos.id === node.id;
    const matched = matchedLeafIds.includes(node.id);
    const focused = focusedLeafId === node.id;
    const selected = selectedId === node.id;
    const { vars, r } = bkVars(node);

    if (node.kind === 'condition') {
      return (
        <div key={node.id}
          className={`node ${isDrag ? 'dragging' : ''} ${isWired ? 'included' : 'draft'} ${matched ? 'match' : ''} ${focused ? 'focus' : ''} ${selected ? 'selected' : ''}`}
          style={{ left: pos.x, top: pos.y }}
          onClick={(e) => select(e, node.id)}
          onContextMenu={(e) => onNodeCtx(e, node.id)}>
          <div className={`socket input ${isWired ? 'snap' : ''}`} data-sock-in={node.id} style={{ left: -10, top: NODE_IN_DY }} title={isWired ? '연결됨' : '루트 와이어 연결'} />
          {bkTag(r)}
          <span className="node-chip">{isWired ? 'IN 정책' : '미연결'}</span>
          {conditionBody(node, false)}
        </div>
      );
    }

    // group
    const isOR = node.combinator === 'OR', isNOT = node.combinator === 'NOT';
    return (
      <div key={node.id}
        className={`node ${isDrag ? 'dragging' : ''} ${isWired ? 'included' : 'draft'} ${focused ? 'focus' : ''} ${selected ? 'selected' : ''}`}
        style={{ left: pos.x, top: pos.y }}
        onClick={(e) => select(e, node.id)}
        onContextMenu={(e) => onNodeCtx(e, node.id)}>
        <div className={`socket input ${isWired ? 'snap' : ''}`} data-sock-in={node.id} style={{ left: -10, top: NODE_IN_DY }} title={isWired ? '연결됨' : '루트 와이어 연결'} />
        {bkTag(r)}
        <span className="node-chip">{isWired ? 'IN 정책' : '미연결'}</span>
        <div className={`body group toned shape-${r.shape}${r.tone ? ' sev-' + r.tone : ''} ${isOR ? 'is-or' : ''} ${isNOT ? 'is-not' : ''}`} style={vars}>
          <div className="grp-head" onPointerDown={(e) => { if (e.target.closest('.grp-flip,.grp-x')) return; startNodeDrag(e, node.id); }}>
            <span className="grp-op">{isNOT ? 'NOT · 부정' : isOR ? 'OR · 하나라도 참' : 'AND · 모두 참'}</span>
            <span className="grp-d">{node.childIds.length}개 자식</span>
            <span className="grp-spc" />
            <button className="grp-flip" onClick={(e) => { e.stopPropagation(); dispatch({ type: 'PATCH_GROUP', id: node.id, patch: { combinator: isAndNext6(node.combinator) } }); }}>→ {isAndNext6(node.combinator)}</button>
            <button className="grp-x" onClick={(e) => { e.stopPropagation(); dispatch({ type: 'DELETE', id: node.id }); }}><V5I.x style={{ width: 11, height: 11 }} /></button>
          </div>
          <div className={`grp-body ${dropTarget === node.id ? 'drop-target' : ''}`} data-drop-grp={node.id}>
            {node.childIds.length === 0 && <div className="grp-empty">{isNOT ? '단일 조건만' : '여기에 블록을 끌어 놓으세요'}</div>}
            {node.childIds.map(cid => renderInlineChild(cid))}
          </div>
        </div>
      </div>
    );
  };

  const renderInlineChild = (id) => {
    const n = state.nodes[id]; if (!n) return null;
    const matched = matchedLeafIds.includes(id);
    const focused = focusedLeafId === id;
    const selected = selectedId === id;
    if (n.kind === 'condition') {
      return (
        <div key={id} className={`node-inline ${matched ? 'match' : ''} ${focused ? 'focus' : ''} ${selected ? 'selected' : ''}`}
          onClick={(e) => select(e, id)} onContextMenu={(e) => onNodeCtx(e, id)}>
          {conditionBody(n, true)}
        </div>
      );
    }
    const isOR = n.combinator === 'OR', isNOT = n.combinator === 'NOT';
    const { vars, r } = bkVars(n);
    return (
      <div key={id} className={`node-inline ${focused ? 'focus' : ''} ${selected ? 'selected' : ''}`} onClick={(e) => select(e, id)} onContextMenu={(e) => onNodeCtx(e, id)}>
        <div className={`body group toned shape-${r.shape}${r.tone ? ' sev-' + r.tone : ''} ${isOR ? 'is-or' : ''} ${isNOT ? 'is-not' : ''}`} style={vars}>
          <div className="grp-head" onPointerDown={(e) => { if (e.target.closest('.grp-flip,.grp-x')) return; startNodeDrag(e, id); }}>
            <span className="grp-op">{isNOT ? 'NOT' : isOR ? 'OR' : 'AND'}</span>
            <span className="grp-d">{n.childIds.length}개</span>
            <span className="grp-spc" />
            <button className="grp-flip" onClick={(e) => { e.stopPropagation(); dispatch({ type: 'PATCH_GROUP', id, patch: { combinator: isAndNext6(n.combinator) } }); }}>→ {isAndNext6(n.combinator)}</button>
            <button className="grp-x" onClick={(e) => { e.stopPropagation(); dispatch({ type: 'DELETE', id }); }}><V5I.x style={{ width: 11, height: 11 }} /></button>
          </div>
          <div className={`grp-body ${dropTarget === id ? 'drop-target' : ''}`} data-drop-grp={id}>
            {n.childIds.length === 0 && <div className="grp-empty">비어 있음</div>}
            {n.childIds.map(cc => renderInlineChild(cc))}
          </div>
        </div>
      </div>
    );
  };

  const rootSocketDrag = (e) => { e.stopPropagation(); startWireDrag(e, root.x + ROOT_OUT_DX, root.y + ROOT_OUT_DY); };

  return (
    <div className="cv-wrap">
      <div ref={scrollRef}
        className={`cv-scroll ${gridStrength === 'subtle' ? 'grid-subtle' : gridStrength === 'strong' ? 'grid-strong' : ''}`}
        onDragOver={(e) => { if (e.dataTransfer.types.includes('text/v6-block')) e.preventDefault(); }}
        onDrop={onPaletteDrop}
        onWheel={onWheel}
        onContextMenu={onCanvasCtx}
        onClick={() => { setFocusedLeafId(null); setSelectedId(null); }}>
        <div className="cv-world" style={{ transform: `scale(${zoom})` }}>
          <div className="cv-wires-layer">
            <svg width="5000" height="3200">
              {state.wires.map(w => {
                const target = state.nodes[w.to]; if (!target) return null;
                const lt = (livePos && livePos.id === w.to) ? livePos : null;
                const tx = (lt ? lt.x : target.x) + NODE_IN_DX, ty = (lt ? lt.y : target.y) + NODE_IN_DY;
                const lr = (livePos && livePos.id === state.rootId) ? livePos : null;
                const rx = (lr ? lr.x : root.x) + ROOT_OUT_DX, ry = (lr ? lr.y : root.y) + ROOT_OUT_DY;
                return (<path key={w.id} d={wirePath6(rx, ry, tx, ty)} className="wire" onClick={(e) => { e.stopPropagation(); dispatch({ type: 'REMOVE_WIRE', id: w.id }); }}><title>클릭하여 와이어 끊기</title></path>);
              })}
              {tempWire && <path d={wirePath6(tempWire.fromX, tempWire.fromY, tempWire.x, tempWire.y)} className="wire temp" />}
            </svg>
          </div>

          {root && (
            <div className="root-anchor" style={{ left: root.x, top: root.y }}>
              <div className="ra-body" onPointerDown={(e) => { if (e.target.closest('.ra-combo,.socket')) return; startNodeDrag(e, root.id); }}>
                <span className="ra-dot" />
                <span className="ra-lab">정책 루트</span>
                <button className="ra-combo" title="결합 방식" onClick={(e) => { e.stopPropagation(); dispatch({ type: 'PATCH_GROUP', id: root.id, patch: { combinator: root.combinator === 'OR' ? 'AND' : 'OR' } }); }}>{root.combinator}</button>
              </div>
              <div className="socket output" title="여기서 끌어 조건/묶음에 연결" style={{ right: -10, top: ROOT_OUT_DY - 9, position: 'absolute' }} onPointerDown={rootSocketDrag} />
            </div>
          )}

          {topNodes.map(n => renderTop(n))}
        </div>
      </div>

      {/* context menu */}
      {menu && menu.type === 'canvas' && (
        <div className="ctx" style={{ left: menu.x, top: menu.y }} onMouseDown={(e) => e.stopPropagation()}>
          <div className="ctx-lab">논리 묶음 추가</div>
          <div className="ctx-item" onClick={() => addLogicAt('AND', menu.wx, menu.wy)}><span className="ci-badge">AND</span><span className="ci-k">AND · 모두 참</span><span className="ci-sub">&&</span></div>
          <div className="ctx-item" onClick={() => addLogicAt('OR', menu.wx, menu.wy)}><span className="ci-badge or">OR</span><span className="ci-k">OR · 하나라도 참</span><span className="ci-sub">||</span></div>
          <div className="ctx-item" onClick={() => addLogicAt('NOT', menu.wx, menu.wy)}><span className="ci-badge not">NOT</span><span className="ci-k">NOT · 부정</span><span className="ci-sub">!</span></div>
        </div>
      )}
      {menu && menu.type === 'node' && state.nodes[menu.id] && (
        <div className="ctx" style={{ left: menu.x, top: menu.y }} onMouseDown={(e) => e.stopPropagation()}>
          <div className="ctx-lab">블록 스타일</div>
          <div className="ctx-item" onClick={() => { onOpenInspector(menu.id); setMenu(null); }}><span className="ci-ic"><V6I.palette style={{ width: 15, height: 15, color: 'var(--slate-500)' }} /></span><span className="ci-k">스타일 편집…</span></div>
          <div className="ctx-lab" style={{ paddingTop: 2 }}>심각도 · status</div>
          <div className="ctx-tones">
            <div className="ctx-tone none" title="없음" onClick={() => { dispatch({ type: 'SET_BLOCK_STYLE', id: menu.id, patch: { tone: null } }); setMenu(null); }} />
            {['fail', 'warn', 'pass'].map(t => (
              <div key={t} className={`ctx-tone ${t}`} title={V6_TONES[t].label} onClick={() => { dispatch({ type: 'SET_BLOCK_STYLE', id: menu.id, patch: { tone: t } }); setMenu(null); }} />
            ))}
          </div>
          <div className="ctx-div" />
          <div className="ctx-item danger" onClick={() => { dispatch({ type: 'DELETE', id: menu.id }); setMenu(null); setSelectedId(null); }}><span className="ci-ic"><V6I.trash style={{ width: 15, height: 15 }} /></span><span className="ci-k">삭제</span></div>
        </div>
      )}
    </div>
  );
}

Object.assign(window, { V6Canvas, wirePath6 });
