/* sim-canvas.jsx v2 — node-link simulation canvas
   Center EOA (draggable, provenance + scoped scalars) · satellite = action
   destination · flow-only edge labels · breach overlay · causal-spine highlight.
   Exposes window.SimCanvas. */
const { useState, useRef, useEffect, useCallback, useMemo } = React;

function useSize(ref) {
  const [s, setS] = useState({ w: 1300, h: 960 });
  useEffect(() => {
    if (!ref.current) return;
    const ro = new ResizeObserver(() => { const el = ref.current; if (el) setS({ w: el.offsetWidth, h: el.offsetHeight }); });
    ro.observe(ref.current); setS({ w: ref.current.offsetWidth, h: ref.current.offsetHeight });
    return () => ro.disconnect();
  }, []);
  return s;
}

const lit = (hl, kind, id) => hl && hl.active && hl[kind] && hl[kind].has(id);
const dimmed = (hl, kind, id) => hl && hl.active && !(hl[kind] && hl[kind].has(id));

/* flow-only edge label (spec §1.3 + feedback: amounts only) */
function EdgeLabel({ x, y, r, recalc, onSeed, hl }) {
  const blocked = r.verdict === "forbid" || r.verdict === "rejected";
  const outEff = r.eff.find((e) => e.out);
  const transformed = outEff && r.cuts.length > 0;
  const cls = "elabel" + (recalc ? " recalc" : "") + (dimmed(hl, "actions", r.id) ? " ghost-dim" : "") + (lit(hl, "actions", r.id) ? " lit" : "");
  const flow = () => {
    if (r.flowIn && r.flowOut) return (<React.Fragment>
      <span>{SIM.fmtAmt(r.flowIn.amt)} {r.flowIn.sym}</span>
      <span className="elabel-arrow"><Icon name="transfer" size={11} /></span>
      <span className={"elabel-out" + (transformed ? " cut" : "")}>
        {transformed && <span className="strike">{SIM.fmtAmt(r.flowOut.amt)}</span>}
        {SIM.fmtAmt(outEff.d)} {r.flowOut.sym}</span>
    </React.Fragment>);
    const f = r.flowOut || r.flowIn;
    const into = !!r.flowIn && !r.flowOut;
    return (<React.Fragment>
      {into && <span className="elabel-arrow"><Icon name="supply" size={11} /></span>}
      <span className={into ? "" : "elabel-out"}>{SIM.fmtAmt(f.amt)} {f.sym}</span>
      {!into && <span className="elabel-arrow"><Icon name="borrow" size={11} /></span>}
    </React.Fragment>);
  };
  return (
    <div className={cls} style={{ left: x, top: y }} onMouseEnter={() => onSeed({ kind: "action", id: r.id })} onMouseLeave={() => onSeed(null)}>
      <div className="elabel-main"><span className="elabel-seq">{r.id}</span>{flow()}</div>
      {(transformed || r.verdict === "warn" || blocked) && (
        <div className="elabel-tags">
          {transformed && <span className="etag transform">−0.3% 수수료</span>}
          {r.verdict === "warn" && <span className="etag warn">{r.firedReason}</span>}
          {blocked && <span className="etag forbid">차단됨</span>}
        </div>
      )}
    </div>
  );
}

/* center EOA — draggable, provenance, scoped scalars */
function EOANode({ pos, sim, onDragStart, onSeed, hl }) {
  const cls = sim.scalars.some((s) => s.breach) ? "breach" : sim.scalars.some((s) => s.near) || sim.balances.some((b) => b.breach) ? "warn" : "";
  const lend = sim.positions.lending;
  const symLit = (sym) => lit(hl, "syms", sym);
  const symDim = (sym) => dimmed(hl, "syms", sym);
  return (
    <div className={"eoa " + cls} style={{ left: pos.x, top: pos.y }}>
      <div className="eoa-head" onMouseDown={onDragStart}>
        <div className="eoa-av">EOA</div>
        <div className="eoa-meta">
          <div className="eoa-eye">{SIM.state0.walletName} · EOA</div>
          <div className="eoa-addr">{SIM.state0.short}</div>
        </div>
        {cls && <div className={"eoa-flag " + (cls === "breach" ? "breach" : "warn")}><Icon name={cls === "breach" ? "alert" : "warn"} size={13} /></div>}
      </div>

      <div className="eoa-sec">
        <div className="eoa-sec-t"><span>잔고 · balances</span><span>{SIM.usd(sim.balances.reduce((s, x) => s + x.after * x.price, 0))}</span></div>
        {sim.balances.map((b) => {
          const changed = b.delta !== 0 || b.ghosts.length;
          return (
            <div key={b.sym} className={"eoa-bal" + (symLit(b.sym) ? " lit" : "") + (symDim(b.sym) ? " dim" : "")}
              onMouseEnter={() => changed && onSeed({ kind: "sym", id: b.sym })} onMouseLeave={() => onSeed(null)}>
              <span className="b-sym">{b.sym}</span>
              <span className={"b-amt " + (b.breach ? "breach" : b.delta > 0 ? "up" : "")}>{SIM.fmtAmt(b.after)}</span>
              <span className={"b-d " + (b.delta > 0 ? "up" : b.delta < 0 ? "down" : "")}>{b.delta === 0 ? (b.ghosts.length ? "—" : "") : SIM.signed(b.delta)}</span>
            </div>
          );
        })}
      </div>

      <div className="eoa-sec">
        <div className="eoa-sec-t"><span>포지션 · {lend.venue}</span></div>
        <div className="eoa-pos"><span className="p-k">담보</span><span className="p-v">{SIM.fmtAmt(lend.collAfter)} {lend.collSym}{lend.collDelta ? ` (${SIM.signed(lend.collDelta)})` : ""}</span></div>
        <div className="eoa-pos"><span className="p-k">부채</span><span className="p-v">{SIM.fmtAmt(lend.debtAfter)} {lend.debtSym}{lend.debtDelta ? ` (${SIM.signed(lend.debtDelta)})` : ""}</span></div>
        {sim.positions.lp.scoped && <div className="eoa-pos"><span className="p-k">LP</span><span className="p-v">{SIM.usd(sim.positions.lp.value)}</span></div>}
      </div>

      {sim.scalars.map((s) => {
        const pct = Math.max(5, Math.min(100, (s.after / s.max) * 100));
        const fpct = Math.max(0, Math.min(100, (s.floor / s.max) * 100));
        const st = s.breach ? "breach" : s.near ? "near" : "safe";
        return (
          <div key={s.key} className={"eoa-sec eoa-scalar" + (lit(hl, "scalars", s.key) ? " lit" : "") + (dimmed(hl, "scalars", s.key) ? " dim" : "")}
            onMouseEnter={() => onSeed({ kind: "scalar", id: s.key })} onMouseLeave={() => onSeed(null)}>
            <div className="eoa-sec-t"><span>{s.label}</span><span className="sc-rule" title={s.rule}>{s.fmtv(s.floor)} {s.isCap ? "cap" : "floor"}</span></div>
            <div className="eoa-hf">
              <span className="hf-v" style={{ color: st === "breach" ? "var(--fail-600)" : st === "near" ? "var(--warn-700)" : undefined }}>{s.fmtv(s.after)}</span>
              <div className="hf-bar">
                <div className={"hf-fill " + st} style={{ width: pct + "%" }}></div>
                <div className="hf-floor" style={{ left: fpct + "%" }}></div>
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}

function SimCanvas(props) {
  const { sim, mode, selectedTx, hl, onSelectTx, onBreachClick, onSeed, recalcKey } = props;
  const wrapRef = useRef(null);
  const { w, h } = useSize(wrapRef);
  const [view, setView] = useState({ x: 0, y: 0, scale: 1 });
  const [off, setOff] = useState({});            // per-node drag offsets
  const pan = useRef(null); const drag = useRef(null);

  const cx = w * 0.475, cy = h * 0.5, R = Math.min(312, Math.max(270, h * 0.32));
  const O = (id) => off[id] || { x: 0, y: 0 };
  const eoaPos = { x: cx + O("eoa").x, y: cy + O("eoa").y };

  const geo = useMemo(() => sim.results.map((r) => {
    const rad = (r.angle * Math.PI) / 180, dir = { x: Math.cos(rad), y: Math.sin(rad) };
    const o = off[r.id] || { x: 0, y: 0 };
    const satX = cx + dir.x * R + o.x, satY = cy + dir.y * R + o.y;
    const start = { x: eoaPos.x + dir.x * 120, y: eoaPos.y + dir.y * 120 };
    const endArrow = { x: satX - dir.x * 48, y: satY - dir.y * 48 };
    const perp = { x: -dir.y, y: dir.x };
    const a1 = { x: endArrow.x - dir.x * 11 + perp.x * 6, y: endArrow.y - dir.y * 11 + perp.y * 6 };
    const a2 = { x: endArrow.x - dir.x * 11 - perp.x * 6, y: endArrow.y - dir.y * 11 - perp.y * 6 };
    const mid = { x: (start.x + endArrow.x) / 2, y: (start.y + endArrow.y) / 2 };
    const breach = { x: start.x + (endArrow.x - start.x) * 0.42, y: start.y + (endArrow.y - start.y) * 0.42 };
    return { r, dir, satX, satY, start, endArrow, mid, breach, arrow: `${endArrow.x},${endArrow.y} ${a1.x},${a1.y} ${a2.x},${a2.y}` };
  }), [sim, w, h, off]);

  const edgeClass = (r) => {
    if (dimmed(hl, "actions", r.id)) return "dim";
    if (selectedTx && r.id !== selectedTx) return "dim";
    if (r.verdict === "forbid" || r.verdict === "rejected") return "forbid";
    if (r.verdict === "warn") return "warn";
    if (r.cuts.length) return "transform";
    if (r.applied.length || r.executed) return "permit";
    return "neutral";
  };
  const satClass = (r) => {
    let c = "sat";
    if (r.verdict === "forbid" || r.verdict === "rejected") c += " blocked";
    else if (r.verdict === "warn") c += " warn";
    else if (r.applied.length || r.executed) c += " affected";
    if (selectedTx === r.id || lit(hl, "actions", r.id)) c += " sel";
    if ((selectedTx && r.id !== selectedTx) || dimmed(hl, "actions", r.id)) c += " dim";
    return c;
  };

  // drag a node
  const startNodeDrag = (id) => (e) => {
    e.stopPropagation();
    drag.current = { id, sx: e.clientX, sy: e.clientY, ox: O(id).x, oy: O(id).y, moved: false };
  };
  const onBgDown = (e) => {
    if (e.target.closest(".eoa,.sat,.elabel,.breach-mk,.breach-call,.cv-zoom,.cv-allblock,.cv-legend")) return;
    pan.current = { sx: e.clientX, sy: e.clientY, vx: view.x, vy: view.y };
  };
  useEffect(() => {
    const move = (e) => {
      if (drag.current) {
        const dx = (e.clientX - drag.current.sx) / view.scale, dy = (e.clientY - drag.current.sy) / view.scale;
        if (Math.abs(dx) + Math.abs(dy) > 3) drag.current.moved = true;
        setOff((o) => ({ ...o, [drag.current.id]: { x: drag.current.ox + dx, y: drag.current.oy + dy } }));
      } else if (pan.current) {
        setView((v) => ({ ...v, x: pan.current.vx + e.clientX - pan.current.sx, y: pan.current.vy + e.clientY - pan.current.sy }));
      }
    };
    const up = () => { setTimeout(() => { drag.current = null; }, 0); pan.current = null; };
    window.addEventListener("mousemove", move); window.addEventListener("mouseup", up);
    return () => { window.removeEventListener("mousemove", move); window.removeEventListener("mouseup", up); };
  }, [view.scale]);
  const zoom = (d) => setView((v) => ({ ...v, scale: Math.max(0.5, Math.min(1.6, +(v.scale + d).toFixed(2))) }));
  const onWheel = (e) => { if (!e.ctrlKey && !e.metaKey) return; e.preventDefault(); zoom(e.deltaY > 0 ? -0.08 : 0.08); };

  return (
    <div className="sim-canvas" ref={wrapRef} onMouseDown={onBgDown} onWheel={onWheel} style={{ cursor: pan.current ? "grabbing" : "grab" }}>
      <div className={"cv-world" + (pan.current || drag.current ? " panning" : "")} style={{ transform: `translate(${view.x}px,${view.y}px) scale(${view.scale})` }}>
        <svg className="cv-edges">
          {geo.map(({ r, start, endArrow, arrow }) => {
            const cls = edgeClass(r), hot = selectedTx === r.id || lit(hl, "actions", r.id);
            return (
              <g key={r.id}>
                <path className={`edge-path ${cls}${hot ? " hot" : ""}`} d={`M ${start.x} ${start.y} L ${endArrow.x} ${endArrow.y}`} />
                {(cls === "permit" || cls === "transform") && <path className={`edge-path ${cls} edge-flow`} d={`M ${start.x} ${start.y} L ${endArrow.x} ${endArrow.y}`} />}
                <polygon className={`edge-arrow ${cls}`} points={arrow} />
              </g>
            );
          })}
        </svg>

        <EOANode pos={eoaPos} sim={sim} onDragStart={startNodeDrag("eoa")} onSeed={onSeed} hl={hl} />

        {geo.map(({ r, satX, satY }) => (
          <div key={r.id} className={satClass(r)} style={{ left: satX, top: satY }}
            onMouseDown={startNodeDrag(r.id)}
            onClick={(e) => { e.stopPropagation(); if (drag.current && drag.current.moved) return; onSelectTx(selectedTx === r.id ? null : r.id); }}
            onMouseEnter={() => onSeed({ kind: "action", id: r.id })} onMouseLeave={() => onSeed(null)}>
            <div className="sat-disc">
              <span className="sat-ic"><Icon name={CAT_ICON[r.cat]} size={23} /></span>
              <span className="sat-tag">{SIM.catLabel[r.cat]}</span>
            </div>
            <div className="sat-target"><span className="st-n">{r.target}</span><span className="st-a">{r.targetShort}</span></div>
          </div>
        ))}

        {geo.map(({ r, mid }) => <EdgeLabel key={r.id} x={mid.x} y={mid.y} r={r} recalc={recalcKey} onSeed={onSeed} hl={hl} />)}

        {geo.filter(({ r }) => r.verdict === "forbid" && (!selectedTx || selectedTx === r.id)).map(({ r, breach, dir }) => {
          const right = dir.x >= 0;
          const callStyle = right ? { left: breach.x - 6, top: breach.y + 4 } : { left: breach.x + 6, top: breach.y + 4, transform: "translate(-100%,0)", textAlign: "right" };
          return (
            <React.Fragment key={"br" + r.id}>
              <div className="breach-mk" style={{ left: breach.x, top: breach.y }} onClick={(e) => { e.stopPropagation(); onBreachClick(r.id); }}
                onMouseEnter={() => onSeed({ kind: "action", id: r.id })} onMouseLeave={() => onSeed(null)}><Icon name="x" size={15} /></div>
              <div className="breach-call" style={callStyle} onClick={(e) => { e.stopPropagation(); onBreachClick(r.id); }}>
                <span>{r.firedReason}</span><span className="bc-rule">{SIM.byId(r.firedBy)?.rule || "bundle"}</span>
              </div>
            </React.Fragment>
          );
        })}
      </div>

      {sim.globalVerdict === "raw" && <div className="cv-banner raw"><Icon name="info" size={15} />정책 없음 — 제약 없는 원시 흐름</div>}
      {sim.globalVerdict === "all-blocked" && <div className="cv-banner allblock"><Icon name="alert" size={15} />{sim.bundleRejected ? "번들 거부 — S′ = S₀" : "모든 액션 차단됨"}</div>}
      {sim.globalVerdict === "all-blocked" && (
        <div className="cv-allblock">
          <div className="ab-t"><Icon name="alert" size={16} />{sim.bundleRejected ? "번들 원자성 — 그룹 전체 거부" : "이 정책 조합으론 유효한 결과 없음"}</div>
          <div className="ab-d">{sim.bundleRejected ? "중간 액션이 차단되어 시퀀스 전체가 무효화됨. 잔고는 S₀ 그대로 — 어떤 변화도 적용되지 않음." : "활성 정책이 모든 후보 액션을 거부합니다."}</div>
          <div className="ab-pairs">
            {sim.results.filter((r) => r.firedBy).map((r) => (
              <div className="ab-pair" key={r.id}><span className="pr">{r.id}</span><span>{r.verb} → {SIM.byId(r.firedBy)?.name}</span></div>
            ))}
          </div>
        </div>
      )}

      <div className="cv-legend">
        <div className="lg-t">간선 = 의미</div>
        <div className="lg-row"><span className="lg-sw pass"></span>통과</div>
        <div className="lg-row"><span className="lg-sw transform"></span>변환 적용</div>
        <div className="lg-row"><span className="lg-sw forbid"></span>차단</div>
        <div className="lg-row"><span className="lg-sw dim"></span>비활성/하류</div>
      </div>
      <div className="cv-hint"><span className="k">⌘ + 스크롤</span> 줌 · <span className="k">노드 드래그</span> 이동 · 위성 = TX 솎기</div>
      <div className="cv-zoom">
        <button onClick={() => zoom(-0.1)}><Icon name="minus" size={14} /></button>
        <span className="zpct">{Math.round(view.scale * 100)}%</span>
        <button onClick={() => zoom(0.1)}><Icon name="plus" size={14} /></button>
        <button onClick={() => { setView({ x: 0, y: 0, scale: 1 }); setOff({}); }} title="reset"><Icon name="target" size={14} /></button>
      </div>
    </div>
  );
}
window.SimCanvas = SimCanvas;
