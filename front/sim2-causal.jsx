/* sim2-causal.jsx — STATE 패널, 5차 피드백.
   기본 = 최종 결과 state만 (provenance 접힘). TX 칩 선택 → 그 TX가 건드린 state·기여만 (focus view).
   위험 배너·대기 의도·변경 이력 제거 · 승인은 표시만(revoke 제거) · perp 제거 · 가로 압축.
   인과(provenance)는 기본은 행 펼침, 선택 시 자동 focus. Exposes window.CausalPanel.
   Hosts shared React-hook + lit/dimmed globals (loaded before sim2-panels.jsx). */

const { useState, useRef, useEffect, useCallback, useMemo } = React;
const lit = (hl, kind, id) => hl && hl.active && hl[kind] && hl[kind].has(id);
const dimmed = (hl, kind, id) => hl && hl.active && !(hl[kind] && hl[kind].has(id));

/* ── generic priority card ─────────────────────────────────────────── */
function StateCard({ tier, icon, title, sub, badges, defaultOpen, children, headRight, open: openProp, onOpenChange }) {
  const fixed = tier === "req";
  const [openS, setOpenS] = useState(fixed ? true : !!defaultOpen);
  // 조건부 표시: defaultOpen(예: 정책 활성)이 바뀌면 펼침/접힘 동기화 (#7-1)
  useEffect(() => { if (!fixed && openProp == null) setOpenS(!!defaultOpen); }, [defaultOpen]);
  const open = openProp != null ? openProp : openS;
  const setOpen = (v) => { if (onOpenChange) onOpenChange(v); else setOpenS(v); };
  const showOpen = fixed || open;
  return (
    <div className={"scard tier-" + tier + (showOpen ? " open" : "")}>
      <div className={"scard-h" + (fixed ? " fixed" : "")} onClick={() => { if (!fixed) setOpen(!open); }}>
        <span className="scard-tier">{fixed ? "필수" : "권장"}</span>
        <div className="scard-ic"><Icon name={icon} size={14} /></div>
        <div className="scard-ti"><div className="ttl">{title}</div>{sub && <div className="sub">{sub}</div>}</div>
        <div className="scard-badges">{badges}{headRight}</div>
        {!fixed && <span className="scard-car"><Icon name="chevron" size={14} /></span>}
      </div>
      {showOpen && <div className="scard-b">{children}</div>}
    </div>
  );
}
const Badge = ({ kind = "n", children }) => <span className={"sbadge " + kind}>{children}</span>;

/* ── provenance lines (행 펼침 — 기본 뷰의 수동 분해) ───────────────── */
function SourceLine({ s, onFocusAction }) {
  return (
    <div className="src" onClick={(e) => { e.stopPropagation(); onFocusAction(s.aid); }}>
      <span className="src-aid">{s.aid}</span>
      <span className="src-verb">{s.verb}<span className="vtag">{SIM.catLabel[s.cat]}</span></span>
      {s.cut && <span className="src-cut">{s.cut.rule} {SIM.signed(s.cut.d)}</span>}
      <span className={"src-d " + (s.d > 0 ? "up" : "down")}>{SIM.signed(s.d)}</span>
    </div>
  );
}
function GhostLine({ g, sym, onFocusAction }) {
  return (
    <div className="src ghost" onClick={(e) => { e.stopPropagation(); onFocusAction(g.aid); }}>
      <span className="src-aid">{g.aid}</span>
      <span className="src-verb"><span className="strike">{g.verb}</span></span>
      <span className="src-d">{SIM.signed(g.d)} {sym}</span>
      <span className="src-block"><Icon name="ghost" size={11} /><span className="rule">{SIM.byId(g.blockedBy)?.rule || "차단"}</span></span>
    </div>
  );
}

/* ── 잔고 행 (기본 뷰) — 명세대로 채운 표 한 행 · 클릭 시 하이라이트(#8) ── */
function BalRow({ b, open, onToggle, onHighlightSym, onHighlightAction, hl }) {
  const changed = b.delta !== 0 || b.ghosts.length > 0;
  const dcls = b.breach ? "breach" : b.delta > 0 ? "up" : b.delta < 0 ? "down" : "zero";
  const label = b.unknown ? "Unknown Token" : b.sym;
  const sub = b.unknown ? b.address : (b.name || b.address);
  const cls = "brow" + (open ? " open" : "") + (b.breach ? " breach" : b.near ? " near" : "") + (lit(hl, "syms", b.sym) ? " lit" : "");
  // 하이라이트된 자산은 출처/유령을 자동 노출 (인과 척추) — 클릭 토글과 별개
  const showBody = changed && (open || lit(hl, "syms", b.sym));
  return (
    <div className={cls}>
      <div className="brow-h" onClick={() => { onHighlightSym && onHighlightSym(b.sym); if (changed) onToggle(b.id); }}>
        <span className="brow-car">{changed ? <Icon name="chevron" size={12} /> : null}</span>
        <span className={"tok-ic" + (b.unknown ? " unk" : "") + (b.kind === "native" ? " native" : "")}>{b.unknown ? "?" : label.charAt(0)}</span>
        <div className="tok-id">
          <div className="t1">
            <span className="sym">{label}</span>
            <span className="chainbadge" title={b.chain}>{b.chainName}</span>
            {b.stale && <span className="stale" title={"가격 동기화 " + b.syncedAgo}>stale</span>}
          </div>
          <div className="t2">{sub}{b.floor != null && <span className="floor"> · floor {b.floor}</span>}</div>
        </div>
        <div className="brow-change">
          {changed ? (
            <React.Fragment>
              <span className={"delta " + dcls}>{SIM.signed(b.delta)}</span>
              <span className="ba">{SIM.fmtAmt(b.before)}<span className="arr">→</span><b className={b.breach ? "breach-t" : ""}>{SIM.fmtAmt(b.after)}</b></span>
            </React.Fragment>
          ) : (
            <span className="qty">{SIM.fmtAmt(b.after)}{b.sym && <span className="u"> {b.sym}</span>}</span>
          )}
        </div>
        <div className="brow-usd">{b.usd != null ? SIM.usd(b.usd) : <span className="noprice">—</span>}</div>
      </div>
      {showBody && (
        <div className="brow-body">
          <div className="prov-lab">출처 분해 · provenance</div>
          {b.sources.map((s, i) => <SourceLine key={"s" + i} s={s} onFocusAction={onHighlightAction} />)}
          {b.ghosts.map((g, i) => <GhostLine key={"g" + i} g={g} sym={b.sym} onFocusAction={onHighlightAction} />)}
        </div>
      )}
    </div>
  );
}

/* ── 포지션 항목 행 (담보/부채) — 접힘, 클릭 시 provenance ─────────── */
function PosLedgerRow({ p, onHighlightAction }) {
  const [open, setOpen] = useState(false);
  const has = p.prov.length > 0;
  const dcls = p.d > 0 ? "up" : p.d < 0 ? "down" : "zero";
  return (
    <div className={"pledger" + (open ? " open" : "")}>
      <div className="pledger-h" onClick={() => has && setOpen((v) => !v)}>
        <span className="pledger-car">{has ? <Icon name="chevron" size={12} /> : null}</span>
        <span className="pledger-k">{p.k}</span>
        <span className="pledger-v"><b>{SIM.fmtAmt(p.v)} {p.sym}</b><span className="usd">{SIM.usd(p.usd)}</span></span>
        <span className={"pledger-d " + dcls}>{p.d === 0 ? "—" : SIM.signed(p.d)}</span>
      </div>
      {has && open && (
        <div className="pledger-body">
          {p.prov.map((x, i) => (
            <div key={i} className={"src" + (x.ghost ? " ghost" : "")} onClick={(e) => { e.stopPropagation(); onHighlightAction && onHighlightAction(x.aid); }}>
              <span className="src-aid">{x.aid}</span>
              <span className="src-verb">{x.ghost ? <span className="strike">{p.k}</span> : p.k}</span>
              <span className={"src-d " + (x.ghost ? "" : x.d > 0 ? "up" : "down")}>{SIM.signed(x.d)} {p.sym}</span>
              {x.ghost && <span className="src-block"><Icon name="ghost" size={11} /><span className="rule">{SIM.byId(x.blockedBy)?.rule || "차단"}</span></span>}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/* ── HF / LTV — 포지션의 구체 지표 게이지 · 클릭 시 하이라이트(#8) ─── */
function MetricGauge({ s, onJumpPolicy, onHighlight, hl }) {
  const [tip, setTip] = useState(false);
  const st = s.breach ? "breach" : s.near ? "near" : "safe";
  const pct = Math.max(5, Math.min(100, (s.after / s.max) * 100));
  const fpct = Math.max(2, Math.min(98, (s.floor / s.max) * 100));
  const cls = "mgauge " + st + (lit(hl, "scalars", s.key) ? " lit" : "");
  return (
    <div className={cls} onClick={() => onHighlight && onHighlight(s.key)}>
      <div className="mg-h">
        <span className="mg-l">{s.label}</span>
        <button className="mg-info" onMouseEnter={() => setTip(true)} onMouseLeave={() => setTip(false)} onClick={(e) => { e.stopPropagation(); setTip((v) => !v); }}><Icon name="info" size={12} /></button>
        {s.policyActive && <span className="mg-rule" onClick={(e) => { e.stopPropagation(); onJumpPolicy && onJumpPolicy(s.policy); }} title="이 지표를 거는 정책">{s.rule}</span>}
        <span className="mg-v">{s.fmtv(s.after)}<span className="b4"> / {s.fmtv(s.before)} 전</span></span>
      </div>
      <div className="mg-meter">
        <div className={"mf " + st} style={{ width: pct + "%" }}></div>
        <div className="mg-fl" style={{ left: fpct + "%" }}><span className="lbl">{s.isCap ? "cap" : "floor"} {s.fmtv(s.floor)}</span></div>
      </div>
      {tip && <div className="mg-tip"><Icon name="info" size={11} />{s.info}</div>}
    </div>
  );
}

/* ── [필수] 잔고 카드 — fungible (USD 정렬) + collectibles ────────── */
function BalanceCard({ sim, open, onToggle, hl, onHighlightSym, onHighlightAction }) {
  const rows = sim.balances;                       // 엔진에서 USD 내림차순 정렬됨
  const totalUsd = sim.balances.reduce((s, x) => s + (x.usd || 0), 0);
  const cols = SIM.state0.collectibles;
  return (
    <StateCard tier="req" icon="wallet" title="토큰 잔고" sub={rows.length + "개 토큰 · " + (SIM.CHAINS ? new Set(rows.map((r) => r.chain)).size : 1) + "체인"}
      headRight={<span className="chip-total">{SIM.usd(totalUsd)}</span>}>
      <div className="balcard">
        <div className="bal-colhead"><span className="c-tok">토큰 · 체인</span><span className="c-chg">변화</span><span className="c-usd">USD</span></div>
        {rows.map((b) => <BalRow key={b.id} b={b} open={!!open[b.id]} onToggle={onToggle} hl={hl} onHighlightSym={onHighlightSym} onHighlightAction={onHighlightAction} />)}
        <div className="coll-sec">
          <div className="coll-lab">Collectibles · NFT<span className="n">{cols.length}</span></div>
          {cols.map((c) => (
            <div key={c.id} className="coll">
              <div className="coll-th"><Icon name="library" size={14} /></div>
              <div className="coll-m"><div className="nm">{c.name}</div><div className="cn">{c.collection}<span className="chainbadge" title={c.chain}>{SIM.CHAINS[c.chain] || c.chain}</span></div></div>
              <div className="coll-fl">{c.floor != null ? <React.Fragment>{c.floor} {c.floorSym}<span className="k">floor</span></React.Fragment> : <span className="owned">Owned</span>}</div>
            </div>
          ))}
        </div>
      </div>
    </StateCard>
  );
}

/* ── [필수] 포지션 카드 — Aave lending only (perp 제거) ───────────── */
function PositionCard({ sim, hl, onHighlightAction, onHighlightScalar, onJumpPolicy }) {
  const ld = sim.positions.lending;
  const summary = `공급 ${SIM.usd(ld.collUsd)} / 대출 ${SIM.usd(ld.debtUsd)} · HF ${ld.hf.fmtv(ld.hf.after)}`;
  const ledger = [
    { k: "담보", sym: ld.collSym, v: ld.collAfter, usd: ld.collUsd, d: ld.collDelta, prov: ld.collProv },
    { k: "부채", sym: ld.debtSym, v: ld.debtAfter, usd: ld.debtUsd, d: ld.debtDelta, prov: ld.debtProv },
  ];
  // HF·LTV 중 하나라도 하이라이트되면 포지션 블록 강조 (인과 척추)
  const posLit = lit(hl, "scalars", "hf") || lit(hl, "scalars", "ltv");
  return (
    <StateCard tier="req" icon="bank" title="포지션" sub="프로토콜 권리">
      <div className="poscard">
        <div className={"pos-sub" + (posLit ? " lit" : "")}>
          <div className="pos-sub-h"><span className="venue">{ld.venue}</span><span className="dot">·</span><span className="market">{ld.market}</span><span className="pos-summary">{summary}</span></div>
          <div className="pos-ledger">{ledger.map((p) => <PosLedgerRow key={p.k} p={p} onHighlightAction={onHighlightAction} />)}</div>
          <div className="pos-metrics">
            <MetricGauge s={ld.hf} hl={hl} onHighlight={onHighlightScalar} onJumpPolicy={onJumpPolicy} />
            <MetricGauge s={ld.ltv} hl={hl} onHighlight={onHighlightScalar} onJumpPolicy={onJumpPolicy} />
          </div>
        </div>
      </div>
    </StateCard>
  );
}

/* ── [권장] 승인 / 권한 — 조건부 표시(#1) + 1줄 좌측정렬(#2) ──────── */
function ApprovalsCard({ apActive }) {
  const ap = SIM.state0.approvals;
  const unl = ap.filter((a) => a.isUnlimited).length;
  const soon = ap.filter((a) => a.permitExpSoon).length;
  const typeLabel = { erc20: "ERC20 approve", permit2: "Permit2", setApprovalForAll: "setApprovalForAll" };
  // 위험 우선 정렬: 무제한 → 만료임박 → 나머지
  const rows = [...ap].sort((a, b) => (Number(b.isUnlimited) - Number(a.isUnlimited)) || (Number(b.permitExpSoon) - Number(a.permitExpSoon)));
  return (
    <StateCard tier="rec" icon="lock" title="승인 · 권한"
      sub={apActive ? "approve.no_unlimited 참조 중" : "외부 컨트랙트 토큰 사용 권한"}
      defaultOpen={apActive}
      badges={<React.Fragment>{unl > 0 && <Badge kind="fail">무제한 {unl}</Badge>}{soon > 0 && <Badge kind="warn">만료임박 {soon}</Badge>}<Badge>{ap.length}</Badge></React.Fragment>}>
      <div className="aprows">
        {rows.map((a) => (
          <div key={a.id} className={"aprow" + (a.isUnlimited ? " unlimited" : a.permitExpSoon ? " soon" : "")}>
            <span className={"ap-ic" + (a.isUnlimited ? " danger" : "")}><Icon name={a.isUnlimited ? "alert" : "lock"} size={12} /></span>
            <div className="ap-line">
              <span className="tok">{a.token}</span><span className="arr">→</span>
              <span className="spn">{a.spender}</span><span className="addr">{a.spenderShort}</span>
            </div>
            <div className="ap-limit">{a.isUnlimited
              ? <span className="ap-unl"><Icon name="warn" size={10} />무제한</span>
              : <span className="ap-amt">{a.amount != null ? (a.token === "USDC" ? SIM.usd(a.amount) : a.amount + " " + a.token) : "—"}</span>}</div>
            <div className="ap-type-cell"><span className="ap-type">{typeLabel[a.type]}{a.permitExpiry && <span className={"ap-exp" + (a.permitExpSoon ? " soon" : "")}> · 만료 {a.permitExpiry}{a.permitExpSoon && " ⚠"}</span>}</span></div>
          </div>
        ))}
      </div>
    </StateCard>
  );
}

/* ══ FOCUS VIEW — TX 선택 시 그 TX가 건드린 state·기여만 (#1·#2) ══ */
function FpBal({ b, selR }) {
  const e = selR.eff.find((x) => x.sym === b.sym);
  const blocked = !selR.executed;
  const d = e ? e.d : 0;
  return (
    <div className={"fpbal" + (blocked ? " blk" : "")}>
      <div className="fp-id"><span className="sym">{b.sym}</span><span className="px">{b.chainName}</span></div>
      <div className="fp-num">
        <span className={"d " + (blocked ? "ghost" : d > 0 ? "up" : "down")}>{blocked ? <span className="strike">{SIM.signed(d)}</span> : SIM.signed(d)}</span>
        {e && e.cut && <span className="fp-cut">{e.cut.rule} {SIM.signed(e.cut.d)}</span>}
        <span className="fp-net">net {SIM.fmtAmt(b.after)}</span>
      </div>
      {blocked && <span className="fp-blk"><Icon name="ghost" size={11} />{SIM.byId(selR.firedBy)?.rule || "차단"}</span>}
    </div>
  );
}

function FocusView({ sim, selR, onClear, onFocusAction, onJumpPolicy }) {
  const selSyms = new Set(selR.eff.map((e) => e.sym));
  const touched = sim.balances.filter((b) => b.chain === "eip155:1" && selSyms.has(b.sym));
  const lend = selR.lend;
  const ld = sim.positions.lending;
  const vd = selR.verdict;
  const vlabel = vd === "forbid" ? "차단" : vd === "warn" ? "검토" : "통과";
  const blocked = !selR.executed;
  const rest = selR.chip.replace(selR.verb, "").trim();
  return (
    <React.Fragment>
      <div className={"focus-bar " + vd}>
        <span className="fb-aid">{selR.id}</span>
        <div className="fb-m"><div className="nm">{selR.verb} {rest}</div><div className="ds">{SIM.catLabel[selR.cat]} · {selR.target}</div></div>
        <span className={"fb-vd " + vd}>{vlabel}</span>
        <button className="fb-clear" onClick={onClear} title="전체 보기"><Icon name="x" size={13} />전체</button>
      </div>
      <div className="focus-note"><Icon name="target" size={12} />이 TX가 건드린 state·판정만 — 빈 곳/같은 칩 클릭 시 전체 복귀</div>

      <StateCard tier="req" icon="wallet" title="잔고 변화" sub={selR.id + " footprint"}>
        <div className="fpcard">
          {touched.length === 0 ? <div className="fp-empty">잔고 변화 없음</div>
            : touched.map((b) => <FpBal key={b.sym} b={b} selR={selR} />)}
        </div>
      </StateCard>

      {lend && (
        <StateCard tier="req" icon="bank" title="포지션 변화" sub={ld.venue + " · " + ld.market}>
          <div className="poscard">
            <div className="pos-sub">
              <div className="fpcard">
                {lend.coll ? <div className={"fpbal" + (blocked ? " blk" : "")}><div className="fp-id"><span className="sym">담보</span><span className="px">{ld.collSym}</span></div><div className="fp-num"><span className={"d " + (blocked ? "ghost" : "up")}>{blocked ? <span className="strike">{SIM.signed(lend.coll)}</span> : SIM.signed(lend.coll)} {ld.collSym}</span><span className="fp-net">net {SIM.fmtAmt(ld.collAfter)}</span></div>{blocked && <span className="fp-blk"><Icon name="ghost" size={11} />{SIM.byId(selR.firedBy)?.rule}</span>}</div> : null}
                {lend.debt ? <div className={"fpbal" + (blocked ? " blk" : "")}><div className="fp-id"><span className="sym">부채</span><span className="px">{ld.debtSym}</span></div><div className="fp-num"><span className={"d " + (blocked ? "ghost" : "down")}>{blocked ? <span className="strike">{SIM.signed(lend.debt)}</span> : SIM.signed(lend.debt)} {ld.debtSym}</span><span className="fp-net">net {SIM.fmtAmt(ld.debtAfter)}</span></div>{blocked && <span className="fp-blk"><Icon name="ghost" size={11} />{SIM.byId(selR.firedBy)?.rule}</span>}</div> : null}
              </div>
              <div className="pos-metrics">
                <MetricGauge s={ld.hf} onJumpPolicy={onJumpPolicy} />
                <MetricGauge s={ld.ltv} onJumpPolicy={onJumpPolicy} />
              </div>
            </div>
          </div>
        </StateCard>
      )}
    </React.Fragment>
  );
}

/* ── 패널 루트 (window.CausalPanel) ───────────────────────────────── */
function CausalPanel({ sim, selTx, apActive, hl, onHighlightSym, onHighlightAction, onHighlightScalar, onSwitchTx, onJumpScalar, onClearSel, tools }) {
  const [open, setOpen] = useState({});
  const toggle = (sym) => setOpen((o) => ({ ...o, [sym]: !o[sym] }));
  const raw = sim.globalVerdict === "raw";
  const totalUsd = sim.balances.reduce((s, x) => s + x.after * x.price, 0);
  const selR = selTx ? sim.results.find((r) => r.id === selTx) : null;

  return (
    <div className="statep">
      <div className="sp-toolbar">
        <div className="sp-tools">
          {(tools || []).map((t) => (
            <button key={t.key} className={"sp-tool" + (t.active ? " on" : "")} onClick={t.onClick} title={t.label}>
              <Icon name={t.icon} size={15} /><span className="lbl">{t.label}</span>
            </button>
          ))}
        </div>
        <span className="sp-s0">S → S′ · <b>{SIM.usd(totalUsd)}</b></span>
      </div>

      <div className="sp-scroll">
        {selR ? (
          <FocusView sim={sim} selR={selR} onClear={onClearSel} onFocusAction={onSwitchTx} onJumpPolicy={onJumpScalar} />
        ) : (
          <React.Fragment>
            {raw && (
              <div className="cz-rawhint">
                <div className="ic"><Icon name="info" size={14} /></div>
                <div><b>정책 없음</b> — 제약 없는 원시 흐름. 우측 <b>정책</b>에서 켜면 인과·차단이 나타납니다.</div>
              </div>
            )}
            <BalanceCard sim={sim} open={open} onToggle={toggle} hl={hl} onHighlightSym={onHighlightSym} onHighlightAction={onHighlightAction} />
            <PositionCard sim={sim} hl={hl} onHighlightAction={onHighlightAction} onHighlightScalar={onHighlightScalar} onJumpPolicy={onJumpScalar} />
            <ApprovalsCard apActive={apActive} />
          </React.Fragment>
        )}
      </div>
    </div>
  );
}
window.CausalPanel = CausalPanel;
