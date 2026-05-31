/* sim2-main.jsx — Scopeball simulation root, RESTRUCTURED.
   주종 역전: 인과 패널(주인공) ‖ 경로 지도 + 판정/정책(보조).
   Phase-aware top bar · 2-phase entry guide · single workspace · causal spine.
   Reuses window.SimPanels (PolicyPanel · VerdictPanel) for the right toggle. */
const { Verdict2, Policy2 } = window.SimPanels2;

/* ── global nav rail (app-level, distinct from sim-internal dock) ── */
const NAV = [
  { id: "home", icon: "home", label: "Home" },
  { id: "editor", icon: "blocks", label: "Editor" },
  { id: "sim", icon: "bolt", label: "Simulation", active: true },
  { id: "monitor", icon: "monitor", label: "Monitoring" },
  { id: "audit", icon: "audit", label: "Audit" },
  { id: "settings", icon: "settings", label: "Settings" },
];
function GlobalNav() {
  return (
    <nav className="gnav">
      <div className="gnav-logo"><div className="mk"><Icon name="bolt" size={16} /></div><span className="word">scopeball</span></div>
      <div className="gnav-ws"><div className="ws-av">A</div><div className="ws-label">Acme<span className="sub">4 wallets · 14 policies</span></div><span className="ws-caret"><Icon name="chevron" size={13} /></span></div>
      <div className="gnav-wallet"><span className="dot"></span><div className="wm"><span className="nm">메인 지갑</span><span className="ad">0xA1c4··7e29</span></div></div>
      <div className="gnav-div"></div>
      <div className="gnav-group">
        {NAV.map((n) => (
          <a key={n.id} className={"gnav-item" + (n.active ? " active" : "")}>
            <span className="icon"><Icon name={n.icon} size={18} /></span><span className="label">{n.label}</span>
          </a>
        ))}
      </div>
      <div className="gnav-bottom"><div className="gnav-user"><div className="av">SK</div><div className="meta"><div className="nm">Soyeon K.</div><div className="em">soyeon@acme.io</div></div></div></div>
    </nav>
  );
}

/* ── causal-spine resolver — seed → unified highlight set ── */
function resolveCausal(sim, seed) {
  const A = new Set(), P = new Set(), S = new Set(), SC = new Set();
  const addAction = (aid) => {
    const r = sim.results.find((x) => x.id === aid); if (!r || A.has(aid)) return;
    A.add(aid); r.applied.forEach((x) => P.add(x.id)); r.eff.forEach((e) => S.add(e.sym));
    sim.scalars.forEach((s) => { if (s.drivers.includes(aid)) SC.add(s.key); });
    // also light the ghost target symbols (counterfactual)
    r.eff.forEach((e) => S.add(e.sym));
  };
  if (seed.kind === "action") addAction(seed.id);
  else if (seed.kind === "policy") {
    P.add(seed.id);
    sim.results.forEach((r) => { if (r.applied.some((x) => x.id === seed.id)) addAction(r.id); });
    const pol = SIM.byId(seed.id); if (pol) { if (pol.scalar) SC.add(pol.scalar); pol.touches.forEach(addAction); }
  } else if (seed.kind === "sym") {
    S.add(seed.id);
    sim.results.forEach((r) => { if (r.eff.some((e) => e.sym === seed.id)) addAction(r.id); });
  } else if (seed.kind === "scalar") {
    SC.add(seed.id);
    const sc = sim.scalars.find((s) => s.key === seed.id); if (sc) { P.add(sc.policy); sc.drivers.forEach(addAction); }
  }
  return { actions: A, policies: P, syms: S, scalars: SC, active: true };
}

/* ── TX input modal (calldata / form / sample) — 추가·편집 진입점 ── */
function TxModal({ modal, addedTx, sim, onToggle, onRemove, onClose, onFocus }) {
  const editing = modal.mode === "edit";
  const initial = editing ? modal.id : "A1";
  const [sel, setSel] = useState(initial);
  const [tab, setTab] = useState(editing ? "form" : "sample");
  const a = SIM.actions.find((x) => x.id === sel) || SIM.actions[0];
  const onSet = addedTx.includes(a.id);
  const calldata = "0x38ed1739000000000000000000000000000000000000000000000000" + "4563918244f400000000000000000000000000000000000000000000000000018f3c…";
  return (
    <div className="txm-overlay" onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="txm">
        <div className="txm-h">
          <div className="ic"><Icon name={editing ? "pencil" : "form"} size={15} /></div>
          <div className="t">{editing ? `TX 편집 · ${a.id}` : "TX 추가"}<span className="s">{editing ? "디코드 결과를 수정" : "calldata · 폼 · 샘플 중 선택"}</span></div>
          <button className="txm-close" onClick={onClose}><Icon name="close" size={15} /></button>
        </div>
        <div className="txm-tabs">
          {!editing && <button className={"txm-tab" + (tab === "sample" ? " on" : "")} onClick={() => setTab("sample")}><Icon name="library" size={13} />샘플</button>}
          <button className={"txm-tab" + (tab === "calldata" ? " on" : "")} onClick={() => setTab("calldata")}><Icon name="code" size={13} />calldata</button>
          <button className={"txm-tab" + (tab === "form" ? " on" : "")} onClick={() => setTab("form")}><Icon name="form" size={13} />폼</button>
        </div>
        <div className="txm-body">
          {tab === "sample" && (
            <div className="txm-samples">
              {SIM.actions.map((s) => {
                const on = addedTx.includes(s.id);
                return (
                  <div key={s.id} className={"txm-sample" + (on ? " on" : "") + (s.id === sel ? " sel" : "")}
                    onClick={() => setSel(s.id)}>
                    <div className="ic"><Icon name={ACT_ICON[s.icon]} size={15} /></div>
                    <div className="m"><div className="nm">{s.chip}</div><div className="ds">{s.full}</div></div>
                    <button className={"add" + (on ? " on" : "")} onClick={(e) => { e.stopPropagation(); onToggle(s.id); }}>
                      {on ? <React.Fragment><Icon name="check" size={13} />추가됨</React.Fragment> : <React.Fragment><Icon name="plus" size={13} />추가</React.Fragment>}
                    </button>
                  </div>
                );
              })}
            </div>
          )}
          {tab === "calldata" && (
            <div className="txm-cd">
              <span className="lab">calldata · {a.id}</span>
              <textarea className="txm-ta" defaultValue={calldata} spellCheck={false}></textarea>
              <div className="txm-cd-row"><button className="txm-decode"><Icon name="zap" size={13} />디코드</button><span className="ok"><Icon name="check" size={12} />디코드 성공 · {SIM.catLabel[a.cat]}</span></div>
            </div>
          )}
          {(tab === "form" || tab === "calldata") && (
            <div className="txm-preview">
              <div className="ph">디코드 결과<span className="cat">{SIM.catLabel[a.cat]}</span></div>
              <div className="kv">
                <span className="k">action</span><span className="v">{a.verb} · {a.cat}</span>
                <span className="k">입력</span><span className="v">{a.flowIn ? `${SIM.fmtAmt(a.flowIn.amt)} ${a.flowIn.sym}` : "—"}</span>
                <span className="k">출력</span><span className="v">{a.flowOut ? `${SIM.fmtAmt(a.flowOut.amt)} ${a.flowOut.sym}` : "—"}</span>
                <span className="k">대상</span><span className="v">{a.target}</span>
                <span className="k">address</span><span className="v mut">{a.targetShort}</span>
              </div>
            </div>
          )}
        </div>
        <div className="txm-foot">
          {editing ? (
            <React.Fragment>
              <button className="txm-ghost danger" onClick={() => { onRemove(a.id); onClose(); }}><Icon name="x" size={14} />이 TX 제거</button>
              <button className="txm-primary" onClick={() => { onFocus(a.id); onClose(); }}><Icon name="check" size={14} />변경 적용</button>
            </React.Fragment>
          ) : (
            <button className="txm-primary" onClick={() => { if (!onSet) onToggle(a.id); onClose(); }}><Icon name="check" size={14} />완료</button>
          )}
        </div>
      </div>
    </div>
  );
}

function gv(globalVerdict, sim) {
  switch (globalVerdict) {
    case "pass": return { cls: "pass", ic: "check", txt: "전체 통과" };
    case "warn": return { cls: "warn", ic: "warn", txt: "검토 필요" };
    case "partial": return { cls: "partial", ic: "alert", txt: "부분 차단", where: sim.blocked[0]?.id };
    case "all-blocked": return { cls: "all-blocked", ic: "x", txt: "전부 차단" };
    default: return { cls: "raw", ic: "dot", txt: "정책 없음" };
  }
}

function App() {
  const [addedTx, setAddedTx] = useState(SIM.defaultTx.slice());
  const [active, setActive] = useState(SIM.defaultActive.slice());
  const [hasResults, setHasResults] = useState(true);   // default load = filled workspace
  const [seed, setSeed] = useState(null);               // sticky focus (hover spine highlight)
  const [selTx, setSelTx] = useState(null);             // #1 selected TX → filter State + verdict
  const [txModal, setTxModal] = useState(null);   // null | {mode:'add'|'edit', id}
  const [vFrac, setVFrac] = useState(0.56);       // verdict share of judge column
  const [dragId, setDragId] = useState(null);     // TX chip being dragged
  const [overId, setOverId] = useState(null);     // drop target chip
  const [toast, setToast] = useState(null);
  const [recalcKey, setRecalcKey] = useState(0);

  // phase: empty → tx-guide → results(workspace)
  useEffect(() => { if (active.length > 0) setHasResults(true); }, [active.length]);
  useEffect(() => { if (addedTx.length === 0) { setHasResults(false); setSeed(null); } }, [addedTx.length]);
  const phase = addedTx.length === 0 ? "empty" : (!hasResults && active.length === 0 ? "tx" : "results");

  const sim = useMemo(() => SIM.evaluate(active, addedTx), [active, addedTx]);
  const hl = useMemo(() => (seed ? resolveCausal(sim, seed) : null), [seed, sim]);

  const flash = (msg, warn) => { setToast({ msg, warn }); setTimeout(() => setToast(null), 2000); };
  const bump = () => setRecalcKey((k) => k + 1);
  const setFocus = (s) => setSeed((cur) => (cur && cur.kind === s.kind && cur.id === s.id ? null : s));
  // 하이라이트(seed) = 가벼운 연결 · 지속. 필터(selTx)와 분리 (#8)
  const onFocusSym = (sym) => setFocus({ kind: "sym", id: sym });
  const onFocusAction = (aid) => setFocus({ kind: "action", id: aid });
  const onFocusScalar = (key) => setFocus({ kind: "scalar", id: key });
  const onJumpScalar = (pid) => setFocus({ kind: "policy", id: pid });
  const selectTx = (aid) => { setSelTx((c) => (c === aid ? null : aid)); setSeed(null); };  // TX 칩 = 전체 필터
  const clearSel = () => setSelTx(null);
  const clearAll = () => { setSelTx(null); setSeed(null); };

  const toggle = (pid) => { setActive((a) => (a.includes(pid) ? a.filter((x) => x !== pid) : [...a, pid])); bump(); };
  const togglePreset = (preId) => { const pre = SIM.presetOf(preId); setActive((a) => { const allOn = pre.members.every((m) => a.includes(m)); return allOn ? a.filter((x) => !pre.members.includes(x)) : [...new Set([...a, ...pre.members])]; }); bump(); };
  const toggleTx = (aid) => { setAddedTx((t) => (t.includes(aid) ? t.filter((x) => x !== aid) : [...t, aid])); bump(); };
  const removeTx = (aid) => { setAddedTx((t) => t.filter((x) => x !== aid)); setSeed(null); setSelTx((c) => (c === aid ? null : c)); bump(); };
  const loadBaseline = () => { setAddedTx(SIM.defaultTx.slice()); setActive([]); flash("샘플 TX 4건 추가됨 · 정책을 선택하세요"); };

  // #1 — drag-reorder = execution order → 드롭 시 즉시 재시뮬
  const reorderTx = (from, to) => {
    if (!from || from === to) return;
    setAddedTx((t) => {
      const arr = t.filter((x) => x !== from);
      const idx = arr.indexOf(to);
      if (idx === -1) return t;
      arr.splice(idx, 0, from);
      return arr;
    });
    flash("실행 순서 변경 · 재시뮬됨"); bump();
  };

  // judge column vertical split (verdict ↕ policy)
  const judgeRef = useRef(null);
  const startVResize = (e) => {
    e.preventDefault();
    const col = judgeRef.current; if (!col) return;
    const rect = col.getBoundingClientRect();
    const move = (ev) => { const f = (ev.clientY - rect.top) / rect.height; setVFrac(Math.max(0.30, Math.min(0.78, f))); };
    const up = () => { window.removeEventListener("mousemove", move); window.removeEventListener("mouseup", up); };
    window.addEventListener("mousemove", move); window.addEventListener("mouseup", up);
  };

  const vinfo = gv(sim.globalVerdict, sim);
  const conflictN = sim.conflicts.length;
  // 승인 카드 조건부 표시: 승인을 참조하는 정책(approve.*)이 활성일 때만 펼침 (#7-1)
  const apActive = active.some((id) => (SIM.byId(id)?.rule || "").startsWith("approve"));
  const orderedTx = addedTx.map((id) => SIM.actions.find((a) => a.id === id)).filter(Boolean);

  /* ── TX chip rail — 드래그로 재정렬(=실행 순서), 압축/펼침, 순번 표시 ── */
  const TxChips = () => (
    <div className="tb-grow tx-dnd">
      {orderedTx.map((a, i) => {
        const r = sim.results.find((x) => x.id === a.id);
        const dot = active.length === 0 ? "raw" : r.verdict;
        const sel = selTx === a.id;
        const rest = a.chip.replace(a.verb, "").trim();
        const isDrag = dragId === a.id, isOver = overId === a.id && dragId && dragId !== a.id;
        return (
          <button key={a.id} draggable
            className={"tx-chip" + (sel ? " sel" : "") + (isDrag ? " dragging" : "") + (isOver ? " over" : "")}
            onClick={() => { if (!dragId) selectTx(a.id); }}
            onDragStart={(e) => { e.dataTransfer.effectAllowed = "move"; setDragId(a.id); }}
            onDragEnd={() => { setDragId(null); setOverId(null); }}
            onDragOver={(e) => { e.preventDefault(); if (overId !== a.id) setOverId(a.id); }}
            onDrop={(e) => { e.preventDefault(); reorderTx(dragId, a.id); setOverId(null); }}>
            <span className="tc-grip"><Icon name="grip" size={13} /></span>
            <span className="tc-seq">{i + 1}</span>
            <span className={"tc-dot " + dot}></span>
            <span className="tc-verb">{a.verb}</span>
            <span className="tc-full">{rest && <span className="rest">{rest}</span>}<span className="tc-tag">{SIM.catLabel[a.cat]}</span>
              <span className="tc-edit" onClick={(e) => { e.stopPropagation(); setTxModal({ mode: "edit", id: a.id }); }} title="편집"><Icon name="pencil" size={11} /></span>
              <span className="tc-rm" onClick={(e) => { e.stopPropagation(); removeTx(a.id); }} title="제거"><Icon name="x" size={11} /></span>
            </span>
          </button>
        );
      })}
      <button className="tx-add" onClick={() => setTxModal({ mode: "add" })}><Icon name="plus" size={14} />TX</button>
    </div>
  );

  return (
    <React.Fragment>
      <GlobalNav />
      <div className="sim-root">
        {/* ════ TOP BAR — 입력 · 출력 (MODE 토글 제거 #2) ════ */}
        <div className="sim-top">
          <div className="tb-brand"><div className="mk"><Icon name="bolt" size={15} /></div><div className="wm">Simulation<span className="sub">policy × tx</span></div></div>

          <div className="tb-sep"></div>
          <div className="tb-group input">
            <span className="tb-glab">입력 · transactions <span className="tb-hint">드래그 = 실행 순서</span></span>
            {phase === "empty" ? (
              <div className="tb-grow"><button className="tx-add" onClick={() => setTxModal({ mode: "add" })}><Icon name="plus" size={14} />TX 추가</button></div>
            ) : <TxChips />}
          </div>

          <div className="tb-flex"></div>

          {phase === "results" && (
            <div className="tb-group output">
              <span className="tb-glab">출력 · status</span>
              <div className="tb-grow">
                <div className={"gverdict " + vinfo.cls}><span className="gv-ic"><Icon name={vinfo.ic} size={11} /></span>{vinfo.txt}{vinfo.where && <span className="gv-where">{vinfo.where}</span>}</div>
                <span className="gcount"><span className="gc-n">{active.length}</span> 활성</span>
                {conflictN > 0 && <span className="gcount conflict"><Icon name="conflict" size={12} /><span className="gc-n">{conflictN}</span> 충돌</span>}
              </div>
            </div>
          )}
        </div>

        {/* ════ BODY — STATE(좌, 主) ‖ 판정 + 정책(우) ════ */}
        <div className="sim-body">
          {phase === "results" ? (
            <div className="sim-work" onClick={(e) => { if (e.target.classList.contains("sim-work")) clearAll(); }}>
              <div className="state-col">
                <CausalPanel sim={sim} selTx={selTx} apActive={apActive} hl={hl} onClearSel={clearSel}
                  onHighlightSym={onFocusSym} onHighlightAction={onFocusAction} onHighlightScalar={onFocusScalar}
                  onSwitchTx={selectTx} onJumpScalar={onJumpScalar}
                  tools={[
                    { key: "tx", icon: "form", label: "TX 입력", onClick: () => setTxModal({ mode: "add" }) },
                    { key: "s0", icon: "reset", label: "S₀ 구성", onClick: () => flash("S₀ 스냅샷 로드됨 · " + SIM.state0.short) },
                    { key: "clr", icon: "target", label: "선택 해제", active: !!selTx || !!seed, onClick: clearAll },
                  ]} />
              </div>
              <div className="judge-col" ref={judgeRef}>
                <div className="card vcard" style={{ flexBasis: (vFrac * 100) + "%" }}>
                  <div className="card-h">
                    <div className="ic"><Icon name="gavel" size={15} /></div>
                    <div className="ti"><div className="ttl">판정 · 위반</div><div className="sub">{selTx ? selTx + " 판정만" : (seed ? "선택 항목·영향 state 강조 중" : "항목 클릭 → 영향 state 강조")}</div></div>
                    {selTx ? <button className="gcount" onClick={clearSel} style={{ height: 26 }}><Icon name="x" size={12} />필터 해제</button>
                      : sim.blocked.length > 0 && <span className="gcount conflict" style={{ height: 26 }}><Icon name="x" size={12} />{sim.blocked.length} 차단</span>}
                  </div>
                  <div className="vcard-b"><Verdict2 sim={sim} selTx={selTx} onSelectAction={onFocusAction} onSeed={(s) => s && setSeed(s)} hl={hl} /></div>
                </div>
                <div className="jr-resize" onMouseDown={startVResize} title="드래그로 크기 조절"><span className="grip"></span></div>
                <div className="card pcard">
                  <div className="card-h">
                    <div className="ic"><Icon name="shield" size={15} /></div>
                    <div className="ti"><div className="ttl">정책</div><div className="sub">preset · on/off · 3-tier</div></div>
                    {conflictN > 0 && <span className="gcount conflict" style={{ height: 26 }}><Icon name="conflict" size={12} />{conflictN} 충돌</span>}
                  </div>
                  <div className="pcard-b"><Policy2 sim={sim} active={active} onToggle={toggle} onTogglePreset={togglePreset} onSeed={(s) => s && setSeed(s)} hl={hl} /></div>
                </div>
              </div>
            </div>
          ) : (
            <div className="guide">
              <div className="guide-card">
                <div className="guide-steps">
                  <div className={"gstep" + (phase === "empty" ? " on" : " done")}><span className="n">{phase === "empty" ? "1" : <Icon name="check" size={13} />}</span><span className="t">TX 추가</span></div>
                  <div className={"gstep-link" + (phase !== "empty" ? " done" : "")}></div>
                  <div className={"gstep" + (phase === "tx" ? " on" : "")}><span className="n">2</span><span className="t">정책 선택</span></div>
                  <div className="gstep-link"></div>
                  <div className="gstep"><span className="n">3</span><span className="t">결과 · 인과</span></div>
                </div>

                {phase === "empty" ? (
                  <React.Fragment>
                    <div className="guide-ic"><Icon name="form" size={30} /></div>
                    <div className="guide-title">시뮬레이션할 TX를 추가하세요</div>
                    <div className="guide-sub">디코드된 트랜잭션을 올리면, 그 위에 정책을 얹어 잔고가 어떻게 바뀌고 어디서 막히는지 확인합니다.</div>
                    <div className="guide-actions">
                      <button className="guide-btn sage" onClick={loadBaseline}><Icon name="zap" size={17} />샘플 TX 4건 불러오기</button>
                      <button className="guide-link"><Icon name="library" size={14} />calldata 직접 붙여넣기 →</button>
                    </div>
                  </React.Fragment>
                ) : (
                  <React.Fragment>
                    <div className="guide-ic"><Icon name="shield" size={30} /></div>
                    <div className="guide-title">적용할 정책을 고르세요</div>
                    <div className="guide-sub">TX {addedTx.length}건이 준비됐습니다. 정책을 켜면 State 패널과 판정·정책이 펼쳐집니다.</div>
                    <div className="guide-picker">
                      <div className="gp-h"><span className="k">2</span>프리셋으로 빠르게 시작</div>
                      <div className="gp-presets">
                        {SIM.presets.map((pre) => (
                          <div key={pre.id} className="pre-chip" onClick={() => togglePreset(pre.id)} style={{ cursor: "pointer" }}>
                            <div className="pre-box"><Icon name="check" size={13} /></div>
                            <div className="pre-m"><div className="nm">{pre.name}</div><div className="ds">{pre.en}</div></div>
                            <span className="pre-cnt">{pre.members.length}</span>
                          </div>
                        ))}
                      </div>
                    </div>
                    <div className="guide-actions" style={{ marginTop: 18 }}>
                      <button className="guide-link" onClick={() => { setAddedTx([]); }}><Icon name="reset" size={14} />← TX 비우기</button>
                    </div>
                  </React.Fragment>
                )}
              </div>
            </div>
          )}

          {txModal && (
            <TxModal modal={txModal} addedTx={addedTx} sim={sim}
              onToggle={toggleTx} onRemove={removeTx} onClose={() => setTxModal(null)} onFocus={onFocusAction} />
          )}

          {toast && <div className={"sim-toast" + (toast.warn ? " warn" : "")}><Icon name={toast.warn ? "warn" : "check"} size={15} />{toast.msg}</div>}
        </div>
      </div>
    </React.Fragment>
  );
}

ReactDOM.createRoot(document.getElementById("app")).render(<App />);
