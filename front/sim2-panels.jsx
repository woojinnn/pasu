/* sim2-panels.jsx — 3차 피드백: 판정 승격 + 가독성(자동 노출 + 나머지 접기).
   Verdict2: forbid/review 위로 · permit 다수는 "통과 N건" 접기 · 위반 상세 첫 건 자동 펼침.
   Policy2: 프리셋이 접힌 묶음(기본) · 펼칠 때만 개별 정책 · 충돌 자동 노출 · 무관 검색.
   Reuses lit()/dimmed() globals from sim2-canvas.jsx. Exposes window.SimPanels2. */
/* useState/useEffect come from sim2-canvas.jsx's shared top-level (loaded before). */

/* ── VERDICT (2급 시민으로 승격) ──────────────────────────────── */
function Verdict2({ sim, selTx, onSelectAction, onSeed, hl }) {
  const order = { forbid: 0, warn: 1, permit: 2 };
  const rows = [...sim.results].sort((a, b) => (order[a.verdict] ?? 3) - (order[b.verdict] ?? 3));
  const flagged = rows.filter((r) => r.verdict === "forbid" || r.verdict === "warn");
  const passed = rows.filter((r) => r.verdict === "permit");
  const violations = sim.results.filter((r) => r.verdict === "forbid");
  const [openPass, setOpenPass] = useState(false);
  const [openViol, setOpenViol] = useState({});
  // 위반이 있으면 첫 건 기본 펼침
  useEffect(() => { if (violations.length) setOpenViol({ [violations[0].id]: true }); }, [sim]);
  const toggleViol = (id) => setOpenViol((o) => ({ ...o, [id]: !o[id] }));

  const VRow = (r) => {
    const v = r.verdict;
    return (
      <div className={"v2-row " + v + (lit(hl, "actions", r.id) ? " lit" : "")} key={r.id}
        onClick={() => onSelectAction(r.id)}>
        <div className="v2-ic"><Icon name={v === "permit" ? "check" : v === "warn" ? "warn" : "x"} size={14} /></div>
        <div className="v2-m">
          <div className="nm"><span className="aid">{r.id}</span>{r.chip}</div>
          <div className="rl">{r.firedBy ? SIM.byId(r.firedBy)?.rule : "발화 규칙 없음 · 통과"}</div>
        </div>
        <span className={"v2-st " + v}>{v === "permit" ? "permit" : v === "warn" ? "review" : "forbid"}</span>
      </div>
    );
  };

  // #1 — TX 선택 시 그 TX의 판정만
  if (selTx) {
    const r = sim.results.find((x) => x.id === selTx);
    if (!r) return null;
    const pol = SIM.byId(r.firedBy);
    return (
      <div className="v2-pane">
        <div className="v2-filterhint"><Icon name="target" size={12} />{r.id} 판정만 표시</div>
        {VRow(r)}
        {r.verdict === "forbid" && (
          <div className="v2-viol open">
            <div className="v2-viol-b" style={{ borderTop: 0 }}>
              <div className="kv"><span className="k">발화 규칙</span><span className="v"><span className="rule">{pol?.rule}</span></span></div>
              <div className="kv"><span className="k">사유</span><span className="v">{r.firedReason}</span></div>
              {r.eff.length > 0 && <div className="kv"><span className="k">막은 변화</span><span className="v ghost">{r.eff.map((e) => `${SIM.signed(e.d)} ${e.sym}`).join(" · ")} <span className="cf">— 이 변화가 안 일어남</span></span></div>}
            </div>
          </div>
        )}
        {r.verdict === "warn" && pol && (
          <div className="v2-viol open"><div className="v2-viol-b" style={{ borderTop: 0 }}>
            <div className="kv"><span className="k">발화 규칙</span><span className="v"><span className="rule">{pol.rule}</span></span></div>
            <div className="kv"><span className="k">사유</span><span className="v">{r.firedReason}</span></div>
          </div></div>
        )}
        {r.verdict === "permit" && <div className="v2-allpass"><div className="ic"><Icon name="check" size={16} /></div><div className="t">통과<span className="s">발화한 차단·검토 규칙 없음</span></div></div>}
      </div>
    );
  }

  return (
    <div className="v2-pane">
      {flagged.length > 0 ? <div className="v2-lead">{flagged.map(VRow)}</div>
        : <div className="v2-allpass"><div className="ic"><Icon name="check" size={16} /></div><div className="t">전체 통과<span className="s">차단·검토 없음 · {passed.length}건 permit</span></div></div>}

      {passed.length > 0 && flagged.length > 0 && (
        <div className="v2-passfold">
          <div className={"v2-passhead" + (openPass ? " open" : "")} onClick={() => setOpenPass((v) => !v)}>
            <span className="dot"></span><span className="t">통과 {passed.length}건</span>
            <span className="car"><Icon name="chevron" size={14} /></span>
          </div>
          {openPass && <div className="v2-passbody">{passed.map(VRow)}</div>}
        </div>
      )}

      {violations.length > 0 && (
        <React.Fragment>
          <div className="v2-sec-t"><Icon name="alert" size={13} />위반 상세<span className="n">{violations.length}</span></div>
          {violations.map((r) => {
            const pol = SIM.byId(r.firedBy); const open = !!openViol[r.id];
            return (
              <div className={"v2-viol" + (open ? " open" : "") + (lit(hl, "actions", r.id) ? " lit" : "")} key={r.id}>
                <div className="v2-viol-h" onClick={() => toggleViol(r.id)}>
                  <div className="vh-ic"><Icon name="x" size={12} /></div>
                  <div className="vh-t">{r.chip} <span className="aid">{r.id}</span></div>
                  <span className="vh-car"><Icon name="chevron" size={14} /></span>
                </div>
                {open && (
                  <div className="v2-viol-b">
                    <div className="kv"><span className="k">발화 규칙</span><span className="v"><span className="rule" onClick={(e) => { e.stopPropagation(); onSelectAction(r.id); }}>{pol?.rule}</span></span></div>
                    <div className="kv"><span className="k">사유</span><span className="v">{r.firedReason}</span></div>
                    {r.eff.length > 0 && <div className="kv"><span className="k">막은 변화</span><span className="v ghost">{r.eff.map((e) => `${SIM.signed(e.d)} ${e.sym}`).join(" · ")} <span className="cf">— 이 변화가 안 일어남</span></span></div>}
                  </div>
                )}
              </div>
            );
          })}
        </React.Fragment>
      )}
    </div>
  );
}

/* ── POLICY (프리셋 접힘 기본 · 펼칠 때만 개별) — #3: 행에서 뱃지·상태·하이라이트 제거 ── */
function PolRow2({ p, active, onToggle }) {
  return (
    <div className="p2-row">
      <div className={"p2-toggle" + (active ? " on" : "")} onClick={(e) => { e.stopPropagation(); onToggle(p.id); }}></div>
      <div className="p2-m">
        <div className="nm">{p.name}</div>
        <div className="rid">{p.rule}</div>
      </div>
    </div>
  );
}

function PresetGroup({ pre, sim, activeSet, fireOf, onToggle, onTogglePreset, defaultOpen }) {
  const [open, setOpen] = useState(defaultOpen);
  const members = pre.members.map(SIM.byId);
  const onN = pre.members.filter((m) => activeSet.has(m)).length;
  const state = onN === 0 ? "" : onN === pre.members.length ? "on" : "mixed";
  return (
    <div className={"p2-grp" + (open ? " open" : "")}>
      <div className="p2-grp-h">
        <div className={"pre-box " + state} onClick={(e) => { e.stopPropagation(); onTogglePreset(pre.id); }}>
          <Icon name={state === "mixed" ? "minus" : "check"} size={13} />
        </div>
        <div className="p2-grp-m" onClick={() => setOpen((v) => !v)}>
          <div className="nm">{pre.name}</div>
          <div className="ds">{pre.en} · {pre.members.length} 정책</div>
        </div>
        <span className="p2-grp-cnt">{onN}/{pre.members.length}</span>
        <span className="p2-grp-car" onClick={() => setOpen((v) => !v)}><Icon name="chevron" size={14} /></span>
      </div>
      {open && <div className="p2-grp-body">{members.map((p) => <PolRow2 key={p.id} p={p} active={activeSet.has(p.id)} onToggle={onToggle} />)}</div>}
    </div>
  );
}

function Policy2({ sim, active, onToggle, onTogglePreset, onSeed, hl }) {
  const [q, setQ] = useState("");
  const activeSet = new Set(active);
  const fireOf = (pid) => { for (const r of sim.results) { const h = r.applied.find((x) => x.id === pid); if (h) return h.outcome; } return null; };
  const searchResults = q ? SIM.policies.filter((p) => p.rule.toLowerCase().includes(q.toLowerCase()) || p.name.includes(q)) : null;
  // a preset is open by default if it has a firing (forbid/warn) member
  const preHasFire = (pre) => pre.members.some((m) => { const f = fireOf(m); return f === "forbid" || f === "warn"; });

  return (
    <div className="p2-pane">
      {sim.conflicts.length > 0 && (
        <div className="p2-conflicts">
          <div className="p2-sec-t conflict"><Icon name="conflict" size={13} />충돌<span className="n">{sim.conflicts.length}</span></div>
          {sim.conflicts.map((c) => (
            <div key={c.id} className={"p2-cflt " + (c.grade === "error" ? "error" : "")}>
              <div className="h"><span className="type">{c.type}</span><span className="grade">{c.grade}</span></div>
              <div className="why">{c.why}</div>
              <div className="pair">
                <span className="pol">{SIM.byId(c.pair[0]).rule}</span>
                <span className="x">⇋</span>
                <span className="pol">{SIM.byId(c.pair[1]).rule}</span>
              </div>
            </div>
          ))}
        </div>
      )}

      <div className="p2-sec-t">프리셋 · 묶음<span className="hint">펼쳐서 개별 정책</span></div>
      {SIM.presets.map((pre) => (
        <PresetGroup key={pre.id} pre={pre} sim={sim} activeSet={activeSet} fireOf={fireOf}
          onToggle={onToggle} onTogglePreset={onTogglePreset} defaultOpen={preHasFire(pre)} />
      ))}

      <div className="p2-search"><Icon name="search" size={14} /><input placeholder="무관 정책 검색 (rule · 이름)…" value={q} onChange={(e) => setQ(e.target.value)} />{q && <button onClick={() => setQ("")}><Icon name="x" size={13} /></button>}</div>
      {searchResults && (
        <div className="p2-grp-body" style={{ paddingTop: 4 }}>
          {searchResults.length === 0 ? <div className="p2-empty">일치하는 정책 없음</div>
            : searchResults.map((p) => <PolRow2 key={p.id} p={p} active={activeSet.has(p.id)} onToggle={onToggle} />)}
        </div>
      )}
    </div>
  );
}

window.SimPanels2 = { Verdict2, Policy2 };
