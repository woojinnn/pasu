// editor-v5-canvas.jsx — Free canvas, wire system, palette, drag.

const { useState: useSCv, useRef: useRCv, useEffect: useECv, useMemo: useMCv, useCallback: useCbCv, useLayoutEffect: useLECv } = React;

// ─────────── PALETTE ───────────
function V5Palette({ onClose, onAddSignal, onAddGroup, locale }) {
  const [q, setQ] = useSCv('');
  const [openBase, setOpenBase] = useSCv(true);
  const [openCustom, setOpenCustom] = useSCv(true);
  const [openLogic, setOpenLogic] = useSCv(true);

  const filter = (items, getLbl) => {
    if (!q) return items;
    const qq = q.toLowerCase();
    return items.filter(s => (getLbl(s).toLowerCase().includes(qq)) || s.id.toLowerCase().includes(qq));
  };

  return (
    <div className="sheet sheet-palette">
      <div className="pal-h">
        <span className="pal-t">Block palette</span>
        <button className="pal-x" onClick={onClose} title="닫기 (Esc)">
          <V5I.x style={{ width: 12, height: 12 }} />
        </button>
      </div>
      <div className="pal-search">
        <V5I.search style={{ width: 13, height: 13, color: 'var(--slate-400)' }} />
        <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="필드 · 묶음 · 경로" />
      </div>
      <div className="pal-scroll">
        {/* Logic groups */}
        <button className="pal-sect-h" onClick={() => setOpenLogic(!openLogic)}>
          <V5I.caretDown style={{ width: 11, height: 11, transform: openLogic ? '' : 'rotate(-90deg)', transition: 'transform 120ms' }} />
          <span className="pal-sect-t">논리 묶음 (logic)</span>
          <span className="pal-sect-c">{V5_SIGS.logic.length}</span>
        </button>
        {openLogic && (
          <div className="pal-items">
            {filter(V5_SIGS.logic, s => s.label[locale] || s.label.en).map(s => (
              <div key={s.id}
                className={`pal-it logic ${s.id === 'OR' ? 'is-or' : ''} ${s.id === 'NOT' ? 'is-not' : ''}`}
                title={s.label[locale]}
                draggable
                onDragStart={(e) => {
                  e.dataTransfer.setData('text/v5-grp', s.id);
                  e.dataTransfer.effectAllowed = 'copy';
                }}
                onClick={() => onAddGroup(s.id)}
              >
                <span className="pal-sw">{s.id}</span>
                <span className="pal-lbl">{s.label[locale] || s.label.en}</span>
                <span className="pal-sub">{s.id === 'AND' ? 'all' : s.id === 'OR' ? 'any' : '!'}</span>
                <button className="pal-add" onClick={(e) => { e.stopPropagation(); onAddGroup(s.id); }}>
                  <V5I.plus style={{ width: 13, height: 13 }} />
                </button>
              </div>
            ))}
          </div>
        )}

        <button className="pal-sect-h" onClick={() => setOpenBase(!openBase)}>
          <V5I.caretDown style={{ width: 11, height: 11, transform: openBase ? '' : 'rotate(-90deg)', transition: 'transform 120ms' }} />
          <span className="pal-sect-t">기본 필드 (calldata)</span>
          <span className="pal-sect-c">{V5_SIGS.base.length}</span>
        </button>
        {openBase && (
          <div className="pal-items">
            {filter(V5_SIGS.base, s => s.label[locale] || s.label.en).map(s => (
              <PalItem key={s.id} sig={s} locale={locale} onAdd={() => onAddSignal(s.id)} />
            ))}
          </div>
        )}

        <button className="pal-sect-h" onClick={() => setOpenCustom(!openCustom)}>
          <V5I.caretDown style={{ width: 11, height: 11, transform: openCustom ? '' : 'rotate(-90deg)', transition: 'transform 120ms' }} />
          <span className="pal-sect-t">커스텀 (manifest enrichment)</span>
          <span className="pal-sect-c">{V5_SIGS.custom.length}</span>
        </button>
        {openCustom && (
          <div className="pal-items">
            {filter(V5_SIGS.custom, s => s.label[locale] || s.label.en).map(s => (
              <PalItem key={s.id} sig={s} locale={locale} onAdd={() => onAddSignal(s.id)} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function PalItem({ sig, locale, onAdd }) {
  const color = v5ColorFor(sig.id);
  return (
    <div className="pal-it" draggable
      onDragStart={(e) => {
        e.dataTransfer.setData('text/v5-sig', sig.id);
        e.dataTransfer.effectAllowed = 'copy';
      }}
      onClick={onAdd}
    >
      <span className={`pal-sw ${color} s-${sig.shape} ${sig.custom ? 'dashed' : ''}`} />
      <span className="pal-lbl">{sig.label[locale] || sig.label.en}</span>
      <button className="pal-add" onClick={(e) => { e.stopPropagation(); onAdd(); }}>
        <V5I.plus style={{ width: 13, height: 13 }} />
      </button>
    </div>
  );
}

// ─────────── DRAG SYSTEM ───────────
// Two drag modes:
//   'node' — moving a node by its body or group header
//   'wire' — drawing a temp wire from a socket
function useNodeDrag({ scrollRef, zoomRef, getNodePos, onMove, onEnd }) {
  const drag = useRCv(null);
  const start = useCbCv((e, id) => {
    const stage = scrollRef.current; if (!stage) return;
    const rect = stage.getBoundingClientRect();
    const z = zoomRef.current || 1;
    const np = getNodePos(id); if (!np) return;
    const wx = (e.clientX - rect.left + stage.scrollLeft) / z;
    const wy = (e.clientY - rect.top + stage.scrollTop) / z;
    drag.current = {
      id,
      offX: wx - np.x, offY: wy - np.y,
      startClient: { x: e.clientX, y: e.clientY }, moved: false,
    };
    document.body.style.cursor = 'grabbing';
    e.preventDefault();
  }, [scrollRef, zoomRef, getNodePos]);

  useECv(() => {
    const move = (e) => {
      if (!drag.current) return;
      const d = drag.current;
      const stage = scrollRef.current; if (!stage) return;
      const dx = e.clientX - d.startClient.x;
      const dy = e.clientY - d.startClient.y;
      if (Math.abs(dx) > 3 || Math.abs(dy) > 3) d.moved = true;
      const rect = stage.getBoundingClientRect();
      const z = zoomRef.current || 1;
      const wx = (e.clientX - rect.left + stage.scrollLeft) / z;
      const wy = (e.clientY - rect.top + stage.scrollTop) / z;
      const nx = Math.max(0, wx - d.offX);
      const ny = Math.max(0, wy - d.offY);
      let hover = null;
      const els = document.elementsFromPoint(e.clientX, e.clientY);
      for (const el of els) {
        if (el.dataset && el.dataset.dropGrp) { hover = el.dataset.dropGrp; break; }
      }
      onMove(d.id, nx, ny, hover);
    };
    const up = (e) => {
      if (!drag.current) return;
      const d = drag.current; document.body.style.cursor = '';
      const stage = scrollRef.current;
      let nx = 0, ny = 0, hover = null;
      if (stage) {
        const rect = stage.getBoundingClientRect();
        const z = zoomRef.current || 1;
        const wx = (e.clientX - rect.left + stage.scrollLeft) / z;
        const wy = (e.clientY - rect.top + stage.scrollTop) / z;
        nx = Math.max(0, wx - d.offX); ny = Math.max(0, wy - d.offY);
        const els = document.elementsFromPoint(e.clientX, e.clientY);
        for (const el of els) {
          if (el.dataset && el.dataset.dropGrp) { hover = el.dataset.dropGrp; break; }
        }
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

// Wire drag — drags from root output socket to a top-level input socket
function useWireDrag({ scrollRef, zoomRef, onEnd }) {
  const drag = useRCv(null);
  const [tempWire, setTempWire] = useSCv(null); // {fromX, fromY, x, y, targetId}

  const start = useCbCv((e, fromWorldX, fromWorldY) => {
    drag.current = { fromX: fromWorldX, fromY: fromWorldY };
    setTempWire({ fromX: fromWorldX, fromY: fromWorldY, x: fromWorldX, y: fromWorldY, targetId: null });
    document.body.style.cursor = 'grabbing';
    e.preventDefault();
  }, []);

  useECv(() => {
    const move = (e) => {
      if (!drag.current) return;
      const stage = scrollRef.current; if (!stage) return;
      const rect = stage.getBoundingClientRect();
      const z = zoomRef.current || 1;
      const wx = (e.clientX - rect.left + stage.scrollLeft) / z;
      const wy = (e.clientY - rect.top + stage.scrollTop) / z;
      let targetId = null;
      const els = document.elementsFromPoint(e.clientX, e.clientY);
      for (const el of els) {
        if (el.dataset && el.dataset.sockIn) { targetId = el.dataset.sockIn; break; }
      }
      setTempWire({ fromX: drag.current.fromX, fromY: drag.current.fromY, x: wx, y: wy, targetId });
    };
    const up = (e) => {
      if (!drag.current) return;
      document.body.style.cursor = '';
      let targetId = null;
      const els = document.elementsFromPoint(e.clientX, e.clientY);
      for (const el of els) {
        if (el.dataset && el.dataset.sockIn) { targetId = el.dataset.sockIn; break; }
      }
      onEnd(targetId);
      drag.current = null; setTempWire(null);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    return () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up); };
  }, [scrollRef, zoomRef, onEnd]);

  return { startWireDrag: start, tempWire };
}

// Bezier path generator (left→right horizontal control points)
function wirePath(x1, y1, x2, y2) {
  const dx = Math.max(40, Math.abs(x2 - x1) * 0.4);
  const c1x = x1 + dx, c1y = y1;
  const c2x = x2 - dx, c2y = y2;
  return `M ${x1},${y1} C ${c1x},${c1y} ${c2x},${c2y} ${x2},${y2}`;
}

// ─────────── MAIN CANVAS ───────────
function V5Canvas({
  state, dispatch,
  focusedLeafId, setFocusedLeafId, matchedLeafIds,
  zoom, setZoom, gridStrength,
  onRequestOR,
}) {
  const scrollRef = useRCv(null);
  const zoomRef = useRCv(zoom);
  useECv(() => { zoomRef.current = zoom; }, [zoom]);

  const [livePos, setLivePos] = useSCv(null);     // { id, x, y } during drag
  const [dropTarget, setDropTarget] = useSCv(null); // target group id

  // Map id → world position (account for parent groups: children inside groups
  // render in-flow; their canvas position is the group's position + offset which
  // we estimate via DOM measurement, NOT stored in state. So we ONLY get world
  // positions for top-level nodes here — wires only go to top-level anyway.
  const getNodePos = useCbCv((id) => {
    if (livePos && livePos.id === id) return { x: livePos.x, y: livePos.y };
    const n = state.nodes[id];
    return n ? { x: n.x, y: n.y } : null;
  }, [state, livePos]);

  // Node move handler
  const onMove = useCbCv((id, x, y, hoverGrp) => {
    setLivePos({ id, x, y });
    const node = state.nodes[id];
    const self = hoverGrp === id;
    const isOwnDesc = (hoverGrp && node) ? (() => {
      let cur = state.nodes[hoverGrp];
      while (cur) { if (cur.id === id) return true; cur = state.nodes[cur.parentId]; }
      return false;
    })() : false;
    setDropTarget(self || isOwnDesc ? null : hoverGrp);
  }, [state]);
  const onEnd = useCbCv((id, x, y, hoverGrp, moved) => {
    setLivePos(null); setDropTarget(null);
    if (!moved) return;
    const node = state.nodes[id]; if (!node) return;
    const self = hoverGrp === id;
    const isOwnDesc = hoverGrp ? (() => {
      let cur = state.nodes[hoverGrp];
      while (cur) { if (cur.id === id) return true; cur = state.nodes[cur.parentId]; }
      return false;
    })() : false;
    if (hoverGrp && !self && !isOwnDesc) {
      dispatch({ type: 'ATTACH_TO_GROUP', id, parentId: hoverGrp });
    } else {
      if (node.parentId) dispatch({ type: 'DETACH_TO_TOP', id, x, y });
      else dispatch({ type: 'MOVE', id, x, y });
    }
  }, [state, dispatch]);
  const { startNodeDrag } = useNodeDrag({ scrollRef, zoomRef, getNodePos, onMove, onEnd });

  // Wire drag — finalize
  const wireEnd = useCbCv((targetId) => {
    if (targetId) dispatch({ type: 'ADD_WIRE', to: targetId });
  }, [dispatch]);
  const { startWireDrag, tempWire } = useWireDrag({ scrollRef, zoomRef, onEnd: wireEnd });

  // Compute socket positions (world coords). Root output = right edge of root.
  // For each top-level node: input socket = left edge of node.
  // We use stored x/y and a couple constants since condition body width varies — but
  // we put the socket at a fixed offset that looks consistent.
  const ROOT_OUT_DX = 220; // approx root body width
  const ROOT_OUT_DY = 22;
  const ROOT_HEAD_W = 220;
  const ROOT_HEAD_H = 44;
  const NODE_IN_DX = -10; // socket sticks out 10px to the left
  const NODE_IN_DY = 22;

  const root = state.nodes[state.rootId];
  const rootSocketPos = root ? { x: root.x + ROOT_OUT_DX, y: root.y + ROOT_OUT_DY } : { x: 0, y: 0 };

  const topNodes = useMCv(() => Object.values(state.nodes).filter(n => n.id !== state.rootId && !n.parentId), [state.nodes]);
  const wiredTos = useMCv(() => new Set(state.wires.map(w => w.to)), [state.wires]);
  const draftCount = useMCv(() => topNodes.filter(n => !wiredTos.has(n.id)).length, [topNodes, wiredTos]);
  const includedTopCount = useMCv(() => Object.values(state.nodes).filter(n => n.id !== state.rootId && v5Included(state, n.id) && n.kind === 'condition').length, [state]);

  // Drop from palette
  const onPaletteDrop = (e) => {
    const sig = e.dataTransfer.getData('text/v5-sig');
    const grp = e.dataTransfer.getData('text/v5-grp');
    if (!sig && !grp) return;
    e.preventDefault();
    const stage = scrollRef.current;
    const rect = stage.getBoundingClientRect();
    const z = zoomRef.current || 1;
    const wx = (e.clientX - rect.left + stage.scrollLeft) / z;
    const wy = (e.clientY - rect.top + stage.scrollTop) / z;
    let parentId = null;
    const els = document.elementsFromPoint(e.clientX, e.clientY);
    for (const el of els) { if (el.dataset && el.dataset.dropGrp) { parentId = el.dataset.dropGrp; break; } }
    if (sig) dispatch({ type: 'ADD_CONDITION', sigId: sig, parentId, x: wx - 80, y: wy - 20 });
    else if (grp) {
      if (grp === 'OR') onRequestOR({ x: wx - 80, y: wy - 20, parentId });
      else dispatch({ type: 'ADD_GROUP', combinator: grp, parentId, x: wx - 80, y: wy - 20 });
    }
  };

  // Wheel zoom (Ctrl/⌘+wheel)
  const onWheel = (e) => {
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      const delta = e.deltaY > 0 ? -0.08 : 0.08;
      setZoom(z => Math.max(0.25, Math.min(4, z + delta)));
    }
  };

  // Render node — top-level only here. Group children render in-flow inside the
  // group's body via renderGroupChild.
  const renderTop = (node) => {
    const pos = getNodePos(node.id);
    const isWired = wiredTos.has(node.id);
    const isDrag = livePos && livePos.id === node.id;
    const matched = matchedLeafIds.includes(node.id);
    const focused = focusedLeafId === node.id;

    if (node.kind === 'condition') {
      return (
        <div key={node.id}
          className={`node ${isDrag ? 'dragging' : ''} ${isWired ? 'included' : 'draft'} ${matched ? 'match' : ''} ${focused ? 'focus' : ''}`}
          style={{ left: pos.x, top: pos.y }}
          onClick={(e) => { e.stopPropagation(); setFocusedLeafId(node.id); }}
        >
          {/* input socket on the left */}
          {!isWired && (
            <div className="socket input"
              data-sock-in={node.id}
              style={{ left: -10, top: NODE_IN_DY }}
              title="여기에 루트 와이어 연결" />
          )}
          {isWired && (
            <div className="socket input snap"
              data-sock-in={node.id}
              style={{ left: -10, top: NODE_IN_DY }}
              title="연결됨 — 와이어 클릭 시 끊기" />
          )}
          <span className="node-chip">{isWired ? 'IN 정책' : '미연결'}</span>
          <div className="body"
            onPointerDown={(e) => {
              if (e.target.closest('.op-btn,.val-chip,.note-pill,.node-x,.pop,.val-pop-card,.seg-chip,.socket')) return;
              startNodeDrag(e, node.id);
            }}
          >
            <V5ConditionBody node={node}
              onPatch={(p) => dispatch({ type: 'PATCH_CONDITION', id: node.id, patch: p })}
              onDelete={() => dispatch({ type: 'DELETE', id: node.id })} />
          </div>
        </div>
      );
    }

    // Group (top-level)
    const isOR = node.combinator === 'OR';
    const isNOT = node.combinator === 'NOT';
    return (
      <div key={node.id}
        className={`node ${isDrag ? 'dragging' : ''} ${isWired ? 'included' : 'draft'} ${focused ? 'focus' : ''}`}
        style={{ left: pos.x, top: pos.y }}
        onClick={(e) => { e.stopPropagation(); setFocusedLeafId(node.id); }}
      >
        {!isWired && (
          <div className="socket input"
            data-sock-in={node.id}
            style={{ left: -10, top: NODE_IN_DY }}
            title="여기에 루트 와이어 연결" />
        )}
        {isWired && (
          <div className="socket input snap"
            data-sock-in={node.id}
            style={{ left: -10, top: NODE_IN_DY }}
            title="연결됨" />
        )}
        <span className="node-chip">{isWired ? 'IN 정책' : '미연결'}</span>
        <div className={`body group ${isOR ? 'is-or' : ''} ${isNOT ? 'is-not' : ''}`}>
          <div className="grp-head" onPointerDown={(e) => {
            if (e.target.closest('.grp-flip,.grp-x')) return;
            startNodeDrag(e, node.id);
          }}>
            <span className="grp-op">{isNOT ? 'NOT · 부정' : isOR ? 'OR · 하나라도 참' : 'AND · 모두 참'}</span>
            <span className="grp-d">{node.childIds.length}개 자식</span>
            <span className="grp-spc" />
            <button className="grp-flip" onClick={(e) => {
              e.stopPropagation();
              const next = isNOT ? 'AND' : (isAndNext(node.combinator));
              dispatch({ type: 'PATCH_GROUP', id: node.id, patch: { combinator: next } });
            }}>→ {isAndNext(node.combinator)}</button>
            <button className="grp-x" onClick={(e) => { e.stopPropagation(); dispatch({ type: 'DELETE', id: node.id }); }}>
              <V5I.x style={{ width: 11, height: 11 }} />
            </button>
          </div>
          <div
            className={`grp-body ${dropTarget === node.id ? 'drop-target' : ''}`}
            data-drop-grp={node.id}
          >
            {node.childIds.length === 0 && (
              <div className="grp-empty">{isNOT ? '단일 조건만' : '여기에 블록을 끌어 놓으세요'}</div>
            )}
            {node.childIds.map(cid => renderInlineChild(cid))}
          </div>
        </div>
      </div>
    );
  };

  // Inline (in-group) child
  const renderInlineChild = (id) => {
    const n = state.nodes[id]; if (!n) return null;
    const matched = matchedLeafIds.includes(id);
    const focused = focusedLeafId === id;
    if (n.kind === 'condition') {
      return (
        <div key={id}
          className={`node-inline ${matched ? 'match' : ''} ${focused ? 'focus' : ''}`}
          onClick={(e) => { e.stopPropagation(); setFocusedLeafId(id); }}
        >
          {/* For included children, we don't show "미연결" chip (rule from spec). */}
          <div className={`body ${matched ? 'match' : ''}`}
               onPointerDown={(e) => {
                 if (e.target.closest('.op-btn,.val-chip,.note-pill,.node-x,.pop,.val-pop-card,.seg-chip')) return;
                 startNodeDrag(e, id);
               }}>
            <V5ConditionBody node={n}
              onPatch={(p) => dispatch({ type: 'PATCH_CONDITION', id, patch: p })}
              onDelete={() => dispatch({ type: 'DELETE', id })} />
          </div>
        </div>
      );
    }
    // nested group inline
    const isOR = n.combinator === 'OR'; const isNOT = n.combinator === 'NOT';
    return (
      <div key={id} className={`node-inline ${focused ? 'focus' : ''}`}>
        <div className={`body group ${isOR ? 'is-or' : ''} ${isNOT ? 'is-not' : ''}`}>
          <div className="grp-head" onPointerDown={(e) => {
            if (e.target.closest('.grp-flip,.grp-x')) return;
            startNodeDrag(e, id);
          }}>
            <span className="grp-op">{isNOT ? 'NOT' : isOR ? 'OR' : 'AND'}</span>
            <span className="grp-d">{n.childIds.length}개</span>
            <span className="grp-spc" />
            <button className="grp-flip" onClick={(e) => { e.stopPropagation(); dispatch({ type: 'PATCH_GROUP', id, patch: { combinator: isAndNext(n.combinator) } }); }}>→ {isAndNext(n.combinator)}</button>
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

  return (
    <div className="cv-wrap">
      <div ref={scrollRef}
           className={`cv-scroll ${gridStrength === 'subtle' ? 'grid-subtle' : gridStrength === 'strong' ? 'grid-strong' : ''}`}
           onDragOver={(e) => { if (e.dataTransfer.types.includes('text/v5-sig') || e.dataTransfer.types.includes('text/v5-grp')) e.preventDefault(); }}
           onDrop={onPaletteDrop}
           onWheel={onWheel}
           onClick={() => setFocusedLeafId(null)}>
        <div className="cv-world" style={{ transform: `scale(${zoom})` }}>
          {/* Wires SVG (rendered in unscaled coordinates since world is scaled) */}
          <div className="cv-wires-layer">
            <svg width="5000" height="3200">
              {state.wires.map(w => {
                const target = state.nodes[w.to]; if (!target) return null;
                const livePosTarget = (livePos && livePos.id === w.to) ? livePos : null;
                const tx = (livePosTarget ? livePosTarget.x : target.x) + NODE_IN_DX;
                const ty = (livePosTarget ? livePosTarget.y : target.y) + NODE_IN_DY;
                const liveRoot = (livePos && livePos.id === state.rootId) ? livePos : null;
                const rx = (liveRoot ? liveRoot.x : root.x) + ROOT_OUT_DX;
                const ry = (liveRoot ? liveRoot.y : root.y) + ROOT_OUT_DY;
                return (
                  <path key={w.id} d={wirePath(rx, ry, tx, ty)} className="wire"
                    onClick={(e) => { e.stopPropagation(); dispatch({ type: 'REMOVE_WIRE', id: w.id }); }}
                  >
                    <title>클릭하여 와이어 끊기</title>
                  </path>
                );
              })}
              {tempWire && (
                <path d={wirePath(tempWire.fromX, tempWire.fromY, tempWire.x, tempWire.y)} className="wire temp" />
              )}
            </svg>
          </div>

          {/* Root node */}
          {root && (
            <div className="root-node" style={{ left: root.x, top: root.y }}>
              <div className="root-body" onPointerDown={(e) => {
                if (e.target.closest('.root-combo,.socket')) return;
                startNodeDrag(e, root.id);
              }}>
                <span className="root-eye">POLICY</span>
                <span className="root-t">Swap baseline</span>
                <button className="root-combo" onClick={(e) => {
                  e.stopPropagation();
                  dispatch({ type: 'PATCH_GROUP', id: root.id, patch: { combinator: root.combinator === 'OR' ? 'AND' : 'OR' } });
                }}>{root.combinator}</button>
              </div>
              <div className="socket output"
                title="여기서 끌어 조건/묶음에 연결"
                style={{ right: -10, top: ROOT_OUT_DY - 9, position: 'absolute' }}
                onPointerDown={(e) => {
                  e.stopPropagation();
                  startWireDrag(e, root.x + ROOT_OUT_DX, root.y + ROOT_OUT_DY);
                }}
              />
            </div>
          )}

          {/* Top-level nodes */}
          {topNodes.map(n => renderTop(n))}
        </div>
      </div>

      {/* Top control strip (chips + zoom) — also has palette/policy/cedar toggles
          when rendered by parent. Parent provides those. */}
    </div>
  );
}

function isAndNext(c) {
  if (c === 'AND') return 'OR';
  if (c === 'OR') return 'NOT';
  return 'AND';
}

Object.assign(window, { V5Palette, V5Canvas, wirePath });
