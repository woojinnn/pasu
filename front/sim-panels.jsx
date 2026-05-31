/* sim-panels.jsx v2 — floating inspector panels
   FloatingPanel chrome + TX · Policy(3-tier, causal) · Balance(provenance+ghost+scalars) · Verdict.
   Exposes window.SimPanels. */

function FloatingPanel({ id, title, sub, icon, pos, z, width, pinned, translucent, onMove, onFocus, onClose, onPin, onTranslucent, children }) {
  const drag = useRef(null);
  const onDown = (e) => { if (e.target.closest(".fp-btn")) return; onFocus && onFocus(id); drag.current = { sx: e.clientX, sy: e.clientY, px: pos.x, py: pos.y }; };
  useEffect(() => {
    const move = (e) => { if (!drag.current) return; onMove(id, { x: Math.max(0, drag.current.px + e.clientX - drag.current.sx), y: Math.max(0, drag.current.py + e.clientY - drag.current.sy) }); };
    const up = () => (drag.current = null);
    window.addEventListener("mousemove", move); window.addEventListener("mouseup", up);
    return () => { window.removeEventListener("mousemove", move); window.removeEventListener("mouseup", up); };
  }, [id, pos]);
  return (
    <div className={"fpanel" + (pinned ? " pinned" : "") + (translucent ? " translucent" : "")} style={{ left: pos.x, top: pos.y, width, zIndex: z }} onMouseDown={() => onFocus && onFocus(id)}>
      <div className="fp-head" onMouseDown={onDown}>
        <div className="fp-ic"><Icon name={icon} size={14} /></div>
        <div className="fp-t"><div className="ttl">{title}</div>{sub && <div className="sub">{sub}</div>}</div>
        <button className={"fp-btn" + (translucent ? " on" : "")} title="반투명" onClick={() => onTranslucent(id)}><Icon name="eye" size={14} /></button>
        <button className={"fp-btn" + (pinned ? " on" : "")} title="핀 고정" onClick={() => onPin(id)}><Icon name="pin" size={14} /></button>
        <button className="fp-btn" title="닫기" onClick={() => onClose(id)}><Icon name="close" size={14} /></button>
      </div>
      <div className="fp-body">{children}</div>
    </div>
  );
}

/* ── TX INPUT ─────────────────────────────────────────────────────── */
function TXPanel({ actions, selectedTx, onSelectTx }) {
  const [tab, setTab] = useState("calldata");
  const a = actions.find((x) => x.id === (selectedTx || actions[0].id)) || actions[0];
  const calldata = "0x38ed1739000000000000000000000000000000000000000000000000" + "4563918244f400000000000000000000000000000000000000000000000000018f...";
  return (
    <div className="tx-pane">
      <div className="seg-tabs" style={{ margin: "-13px -14px 0", padding: "8px 10px" }}>
        <button className={"seg-tab" + (tab === "calldata" ? " on" : "")} onClick={() => setTab("calldata")}><Icon name="code" size={13} />calldata</button>
        <button className={"seg-tab" + (tab === "form" ? " on" : "")} onClick={() => setTab("form")}><Icon name="form" size={13} />폼</button>
        <button className={"seg-tab" + (tab === "sample" ? " on" : "")} onClick={() => setTab("sample")}><Icon name="library" size={13} />샘플</button>
      </div>
      {tab === "calldata" && (
        <div className="tx-cdata">
          <span className="tx-lab">calldata</span>
          <textarea className="tx-textarea" defaultValue={calldata} spellCheck={false} />
          <div className="tx-decode-row"><button className="tx-decode-btn"><Icon name="zap" size={13} />디코드</button><span className="tx-decode-state ok"><Icon name="check" size={12} />디코드 성공</span></div>
        </div>
      )}
      {tab === "form" && (
        <div className="tx-cdata"><div className="tx-kv" style={{ padding: 0 }}>
          <span className="k">송신</span><span className="v mut">EOA · {SIM.state0.short}</span>
          <span className="k">카테고리</span><span className="v">{SIM.catLabel[a.cat]}</span>
          <span className="k">입력</span><span className="v">{a.flowIn ? `${SIM.fmtAmt(a.flowIn.amt)} ${a.flowIn.sym}` : "—"}</span>
          <span className="k">대상</span><span className="v">{a.targetShort}</span>
        </div></div>
      )}
      {tab === "sample" && (
        <div className="tx-samples">{actions.map((s) => (
          <div key={s.id} className="tx-sample" onClick={() => onSelectTx(s.id)} style={selectedTx === s.id ? { borderColor: "var(--sage-400)", background: "var(--sage-50)" } : null}>
            <div className="ts-ic"><Icon name={ACT_ICON[s.icon]} size={14} /></div>
            <div className="ts-m"><div className="nm">{s.chip}</div><div className="ds">{s.full}</div></div>
          </div>
        ))}</div>
      )}
      {tab !== "sample" && (
        <div className="tx-preview"><div className="tx-preview-h">디코드 결과<span className="cat">{SIM.catLabel[a.cat]}</span></div>
          <div className="tx-kv">
            <span className="k">action</span><span className="v">{a.verb} · {a.cat}</span>
            <span className="k">입력</span><span className="v">{a.flowIn ? `${SIM.fmtAmt(a.flowIn.amt)} ${a.flowIn.sym}` : "—"}</span>
            <span className="k">출력</span><span className="v">{a.flowOut ? `${SIM.fmtAmt(a.flowOut.amt)} ${a.flowOut.sym}` : "—"}</span>
            <span className="k">대상</span><span className="v">{a.target}</span>
            <span className="k">address</span><span className="v mut">{a.targetShort}</span>
          </div>
        </div>
      )}
      <button className="tx-add-btn"><Icon name="plus" size={14} />TX 칩으로 추가</button>
    </div>
  );
}

/* ── POLICY (3-tier, causal) ──────────────────────────────────────── */
function PolicyRow({ p, active, fire, onToggle, onSeed, hl, q }) {
  const rid = q ? p.rule.replace(new RegExp("(" + q + ")", "ig"), "<mark>$1</mark>") : p.rule;
  const cls = "pol-row" + (lit(hl, "policies", p.id) ? " lit" : "") + (dimmed(hl, "policies", p.id) ? " dim" : "");
  return (
    <div className={cls} onMouseEnter={() => onSeed({ kind: "policy", id: p.id })} onMouseLeave={() => onSeed(null)}>
      <div className={"pol-toggle" + (active ? " on" : "")} onClick={() => onToggle(p.id)}></div>
      <div className="pol-m">
        <div className="nm">{fire && <span className={"fire " + fire}></span>}{p.name}<span className="pol-cat">{SIM.catLabel[p.cat]}</span></div>
        <div className="rid" dangerouslySetInnerHTML={{ __html: rid }}></div>
      </div>
      <div className="pol-right">
        <span className={"pbadge " + p.kind}>{p.kind === "constraint" ? "제약형" : "변환형"}</span>
        {active && <span className={"pol-fire-lab " + (fire || "idle")}>{fire === "forbid" ? "차단" : fire === "warn" ? "검토" : fire === "permit" ? "통과" : fire === "transform" ? "적용" : "미발화"}</span>}
      </div>
    </div>
  );
}

function PolicyPanel({ sim, active, onToggle, onTogglePreset, onSeed, hl }) {
  const [q, setQ] = useState(""); const [openRelated, setOpenRelated] = useState(false);
  const activeSet = new Set(active);
  const fireOf = (pid) => { for (const r of sim.results) { const h = r.applied.find((x) => x.id === pid); if (h) return h.outcome; } return null; };
  const conflictIds = new Set(sim.conflicts.flatMap((c) => c.pair));
  const visible = SIM.policies.filter((p) => !p.hidden || activeSet.has(p.id));
  const applied = visible.filter((p) => activeSet.has(p.id) && sim.appliedPolicyIds.includes(p.id));
  const related = visible.filter((p) => !activeSet.has(p.id));
  const searchResults = q ? SIM.policies.filter((p) => p.rule.toLowerCase().includes(q.toLowerCase()) || p.name.includes(q)) : null;
  return (
    <div>
      <div className="pre-wrap">
        <div className="pre-lab"><Icon name="layers" size={12} />프리셋 · 묶음 단위 선택</div>
        <div className="pre-chips">{SIM.presets.map((pre) => {
          const onN = pre.members.filter((m) => activeSet.has(m));
          const state = onN.length === 0 ? "" : onN.length === pre.members.length ? "on" : "mixed";
          return (
            <div key={pre.id} className={"pre-chip " + state} onClick={() => onTogglePreset(pre.id)}>
              <div className="pre-box"><Icon name={state === "mixed" ? "minus" : "check"} size={13} /></div>
              <div className="pre-m"><div className="nm">{pre.name}</div><div className="ds">{pre.en}</div></div>
              <span className="pre-cnt">{onN.length}/{pre.members.length}</span>
            </div>
          );
        })}</div>
      </div>
      <div className="pol-search"><Icon name="search" size={14} /><input placeholder="무관 정책 검색 (rule ID · 이름)…" value={q} onChange={(e) => setQ(e.target.value)} />{q && <button className="ps-clear" onClick={() => setQ("")}><Icon name="x" size={13} /></button>}</div>

      {searchResults && (
        <div className="pol-sec"><div className="pol-sec-h"><span className="ps-dot related"></span><span className="ps-t">검색 결과</span><span className="ps-n">{searchResults.length}</span></div>
          <div className="pol-sec-body">{searchResults.length === 0 ? <div className="pol-empty">일치하는 정책 없음</div> : searchResults.map((p) => <PolicyRow key={p.id} p={p} active={activeSet.has(p.id)} fire={fireOf(p.id)} onToggle={onToggle} onSeed={onSeed} hl={hl} q={q} />)}</div>
        </div>
      )}
      {!q && (
        <React.Fragment>
          <div className="pol-sec"><div className="pol-sec-h"><span className="ps-dot applied"></span><span className="ps-t">적용됨</span><span className="ps-n">{applied.length}</span></div>
            <div className="pol-sec-body">{applied.length === 0 ? <div className="pol-empty">활성 정책 없음 — <b>기준선</b></div> : applied.map((p) => <PolicyRow key={p.id} p={p} active fire={fireOf(p.id)} onToggle={onToggle} onSeed={onSeed} hl={hl} />)}</div>
          </div>
          {sim.conflicts.length > 0 && (
            <div className="pol-sec conflict"><div className="pol-sec-h"><span className="ps-dot conflict"></span><span className="ps-t">충돌</span><span className="ps-n">{sim.conflicts.length}</span></div>
              <div className="pol-sec-body">{sim.conflicts.map((c) => (
                <div key={c.id} className={"cflt " + (c.grade === "error" ? "error" : "")}>
                  <div className="cflt-h"><span className="cflt-type">{c.type}</span><span className="cflt-grade">{c.grade}</span></div>
                  <div className="cflt-why">{c.why}</div>
                  <div className="cflt-pair">
                    <span className="cflt-pol" onMouseEnter={() => onSeed({ kind: "policy", id: c.pair[0] })} onMouseLeave={() => onSeed(null)}>{SIM.byId(c.pair[0]).rule}</span>
                    <span className="cflt-x">⇋</span>
                    <span className="cflt-pol" onMouseEnter={() => onSeed({ kind: "policy", id: c.pair[1] })} onMouseLeave={() => onSeed(null)}>{SIM.byId(c.pair[1]).rule}</span>
                  </div>
                </div>
              ))}</div>
            </div>
          )}
          <div className="pol-sec"><div className={"pol-sec-h" + (openRelated ? "" : " collapsed")} onClick={() => setOpenRelated((v) => !v)}><span className="ps-dot related"></span><span className="ps-t">관련 · 비활성</span><span className="ps-n">{related.length}</span><span className="ps-car"><Icon name="chevron" size={14} /></span></div>
            {openRelated && <div className="pol-sec-body">{related.map((p) => <PolicyRow key={p.id} p={p} active={false} fire={null} onToggle={onToggle} onSeed={onSeed} hl={hl} />)}<div className="pol-empty" style={{ padding: "8px 6px 2px", fontSize: 10.5 }}>무관 정책은 숨김 — <b>검색</b>으로만 접근</div></div>}
          </div>
        </React.Fragment>
      )}
    </div>
  );
}

/* ── BALANCE & POSITION (provenance + counterfactual + scalars) ───── */
function ProvLine({ aid, verb, d, sym, cut, ghost, blockedBy }) {
  return (
    <div className={"prov" + (ghost ? " ghost" : "")}>
      <span className="prov-aid">{aid}</span>
      <span className="prov-verb">{verb}</span>
      <span className={"prov-d " + (ghost ? "" : d > 0 ? "up" : "down")}>{SIM.signed(d)} {sym}</span>
      {cut && <span className="prov-cut">{cut.rule} {SIM.signed(cut.d)}</span>}
      {ghost && <span className="prov-block"><Icon name="ghost" size={10} />{SIM.byId(blockedBy)?.rule || "bundle 거부"}</span>}
    </div>
  );
}

function BalancePanel({ sim, onSeed, hl, onJumpScalar }) {
  const maxOf = (b) => (Math.max(b.before, b.after, b.floor ? b.floor * 1.4 : 0) * 1.05) || 1;
  return (
    <div className="res-pane">
      <div className="res-sec-t">잔고 변화<span className="sub">S → S′ · 출처 추적</span></div>
      {sim.balances.map((b) => {
        const max = maxOf(b), cls = b.breach ? "breach" : b.near ? "near" : "";
        const changed = b.delta !== 0 || b.ghosts.length;
        return (
          <div key={b.sym} className={"bal-row " + cls + (lit(hl, "syms", b.sym) ? " lit" : "") + (dimmed(hl, "syms", b.sym) ? " dim" : "")}
            onMouseEnter={() => changed && onSeed({ kind: "sym", id: b.sym })} onMouseLeave={() => onSeed(null)}>
            <div className="bal-top"><span className="sym">{b.sym}</span><span className="px">@ {SIM.usd(b.price)}</span>
              <span className={"delta " + (b.delta > 0 ? "up" : b.delta < 0 ? "down" : "zero")}>{b.delta === 0 ? (b.ghosts.length ? "변화 없음" : "—") : SIM.signed(b.delta)}</span></div>
            <div className="bal-bars">
              <div className="bal-bar before" style={{ width: (b.before / max * 100) + "%" }}></div>
              <div className={"bal-bar after " + cls} style={{ width: (b.after / max * 100) + "%" }}></div>
              {b.floor != null && <div className="bal-floor" style={{ left: (b.floor / max * 100) + "%" }}><span className="ff-l">{b.floorLabel} {b.floor}</span></div>}
            </div>
            <div className="bal-amts"><span className="ba-b">before {SIM.fmtAmt(b.before)}</span><span className={"ba-a " + cls}>after {SIM.fmtAmt(b.after)}</span></div>
            {changed && (
              <div className="prov-list">
                {b.sources.map((s, i) => <ProvLine key={"s" + i} aid={s.aid} verb={s.verb} d={s.d} sym={b.sym} cut={s.cut} />)}
                {b.ghosts.map((g, i) => <ProvLine key={"g" + i} aid={g.aid} verb={g.verb} d={g.d} sym={b.sym} ghost blockedBy={g.blockedBy} />)}
              </div>
            )}
          </div>
        );
      })}

      <div className="res-sec-t" style={{ marginTop: 12 }}>포지션 · {sim.positions.lending.venue}</div>
      {[{ k: "담보", sym: sim.positions.lending.collSym, v: sim.positions.lending.collAfter, d: sim.positions.lending.collDelta, prov: sim.positions.lending.collProv },
        { k: "부채", sym: sim.positions.lending.debtSym, v: sim.positions.lending.debtAfter, d: sim.positions.lending.debtDelta, prov: sim.positions.lending.debtProv }].map((p) => (
        <div className="pos-row" key={p.k}>
          <span className="pk">{p.k}</span>
          <div className="pm"><div className="nm">{SIM.fmtAmt(p.v)} {p.sym}</div>
            {p.prov.length > 0 && <div className="vn">{p.prov.map((x) => `${x.aid} ${x.ghost ? "유령" : ""}${SIM.signed(x.d)}`).join(" · ")}</div>}</div>
          {p.d !== 0 && <div className="pv"><span className={"pd " + (p.d > 0 ? "up" : "down")}>{SIM.signed(p.d)}</span></div>}
        </div>
      ))}

      {sim.scalars.length > 0 && <div className="res-sec-t" style={{ marginTop: 12 }}>파생 스칼라 · 정책 참조분만</div>}
      {sim.scalars.map((s) => {
        const st = s.breach ? "breach" : s.near ? "near" : "safe";
        const pct = Math.max(5, Math.min(100, (s.after / s.max) * 100)), fpct = (s.floor / s.max) * 100;
        return (
          <div key={s.key} className={"hf-card " + st + (lit(hl, "scalars", s.key) ? " lit" : "") + (dimmed(hl, "scalars", s.key) ? " dim" : "")}
            onMouseEnter={() => onSeed({ kind: "scalar", id: s.key })} onMouseLeave={() => onSeed(null)}>
            <div className="hf-h"><span className="hl">{s.label}</span><span className="hv">{s.fmtv(s.after)}<span className="hb"> / {s.fmtv(s.before)} 전</span></span></div>
            <div className="hf-meter"><div className={"mf " + st} style={{ width: pct + "%" }}></div><div className="mfloor" style={{ left: fpct + "%" }}><span className="ml">{s.isCap ? "cap" : "floor"} {s.fmtv(s.floor)}</span></div></div>
            <div className="hf-prov"><span className="rule" onClick={() => onJumpScalar(s.policy)}>{s.rule}</span> 가 거는 값 · 구동: {s.drivers.join(", ")}</div>
          </div>
        );
      })}
    </div>
  );
}

/* ── VERDICT & VIOLATION (causal) ─────────────────────────────────── */
function VerdictPanel({ sim, onSelectCanvas, onSeed, hl }) {
  const violations = sim.results.filter((r) => r.verdict === "forbid");
  return (
    <div className="res-pane">
      <div className="res-sec-t">액션별 판정</div>
      {sim.results.map((r) => {
        const v = r.verdict === "rejected" ? "forbid" : r.verdict;
        return (
          <div className={"vrd-row " + v + (lit(hl, "actions", r.id) ? " lit" : "") + (dimmed(hl, "actions", r.id) ? " dim" : "")} key={r.id}
            onClick={() => onSelectCanvas(r.id)} onMouseEnter={() => onSeed({ kind: "action", id: r.id })} onMouseLeave={() => onSeed(null)}>
            <div className="vrd-ic"><Icon name={v === "permit" ? "check" : v === "warn" ? "warn" : "x"} size={13} /></div>
            <div className="vrd-m"><div className="nm"><span className="seq">{r.id}</span>{r.chip}</div>
              <div className="rl">{r.firedBy ? SIM.byId(r.firedBy)?.rule : r.verdict === "rejected" ? "bundle 원자성" : "발화 규칙 없음"}</div></div>
            <span className="vrd-st">{v === "permit" ? "permit" : v === "warn" ? "review" : r.verdict === "rejected" ? "rejected" : "forbid"}</span>
          </div>
        );
      })}
      {violations.length > 0 && (
        <React.Fragment>
          <div className="res-sec-t" style={{ marginTop: 12 }}>위반 상세<span className="sub">{violations.length} 차단</span></div>
          {violations.map((r) => { const pol = SIM.byId(r.firedBy); return (
            <div className="viol-card" key={r.id} onMouseEnter={() => onSeed({ kind: "action", id: r.id })} onMouseLeave={() => onSeed(null)}>
              <div className="viol-h"><div className="vh-ic"><Icon name="x" size={11} /></div><div className="vh-t">{r.chip} 차단</div><span className="vh-seq">{r.id}</span></div>
              <div className="viol-b">
                <div className="viol-kv"><span className="vk">발화 규칙</span><span className="vv">{pol ? <span className="rule" onMouseEnter={() => onSeed({ kind: "policy", id: r.firedBy })}>{pol.rule}</span> : "bundle 원자성"}</span></div>
                <div className="viol-kv"><span className="vk">사유</span><span className="vv">{r.firedReason}</span></div>
                {r.eff.length > 0 && <div className="viol-kv"><span className="vk">막은 변화</span><span className="vv">{r.eff.map((e) => `${SIM.signed(e.d)} ${e.sym}`).join(" · ")} <span className="cf-note">— 이 변화가 안 일어남</span></span></div>}
              </div>
            </div>
          ); })}
        </React.Fragment>
      )}
    </div>
  );
}

window.SimPanels = { FloatingPanel, TXPanel, PolicyPanel, BalancePanel, VerdictPanel };
