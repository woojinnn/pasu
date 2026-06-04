// mypolicy-list.jsx — My Policy 목록 (좌우 2단 · v5)
// 왼쪽 = 패키지 패널(전체/단일 정책/내 팩 · 필터+드래그 타깃). 오른쪽 = 칼럼 정렬 정책 목록(하이브리드).
// 에디터(폼/블록/Cedar)는 별도 화면(mypolicy-editor.jsx) — 여기선 손대지 않음.

const { useState: useSL, useRef: useRL, useMemo: useML, useEffect: useEL } = React;

function rowOn(r) {
  if (r.life) return r.life === "publish" && r.on;
  return r.on !== false;
}
function isDraft(r) { return r.life === "draft"; }
function isOff(r) { return r.life === "publish" ? !r.on : r.on === false; }

// 결정론적 "마지막 수정" 라벨 (목업용 — uid 해시 기반, 안정적)
function hashStr(s) { let h = 0; for (let i = 0; i < s.length; i++) { h = (h * 31 + s.charCodeAt(i)) | 0; } return Math.abs(h); }
function mtimeLabel(r, loc) {
  const h = hashStr(r.uid || r.id || "x");
  if (isDraft(r)) { const m = 2 + (h % 40); return loc === "en" ? `${m}m ago` : `${m}분 전`; }
  const bucket = h % 100;
  if (bucket < 22) { const v = 1 + (h % 11); return loc === "en" ? `${v}h ago` : `${v}시간 전`; }
  if (bucket < 70) { const v = 1 + (h % 13); return loc === "en" ? `${v}d ago` : `${v}일 전`; }
  const v = 1 + (h % 8); return loc === "en" ? `${v}w ago` : `${v}주 전`;
}

// ── 상태 배지 (예외만 신호) ──
function StateBadge({ r, loc }) {
  if (isDraft(r)) {
    return <span className="mp-badge-draft" title={loc === "en" ? "Editing — auto-excluded from evaluation" : "수정 중 — 평가에서 자동 제외"}>{MPI.draft()}{loc === "en" ? "Draft" : "수정중"}</span>;
  }
  return null;
}

// ── 정책 행 (칼럼 정렬 · 표처럼 세로로 훑김) ──
function RuleRow({ r, loc, dispatch, onOpen, onDragStart, dragId, selected, onSelect, conflict, scopeIsAll }) {
  const draft = isDraft(r);
  const off = isOff(r);
  const cs = catStyle(r.cat);
  const cls = ["mp-trow", dragId === r.uid ? "dragging" : "", off ? "off" : "", draft ? "draft" : "",
    conflict ? "conflict" : "", selected ? "sel" : ""].filter(Boolean).join(" ");
  const canToggle = !draft;
  const sevTxt = r.sev === "fail" ? (loc === "en" ? "Block" : "차단") : (loc === "en" ? "Warn" : "경고");
  return (
    <div className={cls} data-row={r.uid}
      onClick={(e) => { if (e.target.closest("button,.mp-grip,.mp-tg,.mp-selbox")) return; if (e.shiftKey) { onSelect(r.uid); } else { onOpen(r); } }}>
      {/* col1 — select + grip */}
      <div className="mp-c-sel">
        <button className={`mp-selbox ${selected ? "on" : ""}`} onClick={(e) => { e.stopPropagation(); onSelect(r.uid); }}
          title={loc === "en" ? "Select" : "선택"}>{selected && MPI.check()}</button>
        <span className="mp-grip" title={loc === "en" ? "Drag to a package" : "팩으로 끌어 옮기기"}
          onPointerDown={(e) => onDragStart(e, r)}>{MPI.grip()}</span>
      </div>
      {/* col2 — icon + name + slug */}
      <div className="mp-c-name">
        <span className="mp-cat-ic" style={cs.iconWrap}><CatIcon cat={r.cat} /></span>
        <div className="mp-nm-wrap">
          <div className="mp-nm-line"><span className="nm-t">{MP.t(r.name, loc)}</span>{draft && <StateBadge r={r} loc={loc} />}</div>
          {r.slug && <div className="mp-nm-slug">{r.slug}</div>}
        </div>
      </div>
      {/* col3 — category */}
      <div className="mp-c-cat"><span className="mp-cat-tag" style={cs.tag}>{MP.t(MP.CAT[r.cat], loc)}</span></div>
      {/* col4 — severity */}
      <div className="mp-c-sev"><span className={`mp-sevtag ${r.sev}`}><span className="dt" />{sevTxt}</span></div>
      {/* col5 — flags (conflict / update) */}
      <div className="mp-c-flag">
        {conflict && <span className="mp-fl-conflict" title={loc === "en" ? "Duplicate conflict" : "충돌 중인 중복"}>{MPI.warn()}{loc === "en" ? "Conflict" : "충돌"}</span>}
        {!conflict && r.pkgUpdate && <span className="mp-fl-upd" title={loc === "en" ? "Pack update available" : "팩 업데이트 있음"}>{MPI.warn()}{loc === "en" ? "Update" : "업데이트"}</span>}
      </div>
      {/* col6 — last modified */}
      <div className="mp-c-time">{mtimeLabel(r, loc)}</div>
      {/* col7 — toggle + open */}
      <div className="mp-c-act">
        <button className={`mp-tg ${rowOn(r) ? "on" : ""}`} disabled={!canToggle}
          onClick={(e) => { e.stopPropagation(); dispatch({ type: "TOGGLE", uid: r.uid, container: r._pkgId || "loose" }); }}
          title={canToggle ? (loc === "en" ? "Toggle on/off" : "켜기/끄기") : (loc === "en" ? "Draft can't be toggled" : "draft는 토글 불가")}>
          <span className="sw" />
        </button>
        <button className="mp-open" onClick={(e) => { e.stopPropagation(); onOpen(r); }} title={loc === "en" ? "Open editor" : "에디터 열기"}>{MPI.caret()}</button>
      </div>
    </div>
  );
}

// ── 왼쪽 패키지 패널 항목 ──
function PackItem({ active, drop, hot, onClick, icon, name, sub, right, badge, source }) {
  return (
    <button className={`mp-pk ${active ? "active" : ""} ${hot ? "drop-hot" : ""}`} data-drop={drop} onClick={onClick}>
      <span className="mp-pk-ic">{icon}</span>
      <span className="mp-pk-body">
        <span className="mp-pk-nm">{name}{badge}</span>
        {sub && <span className="mp-pk-sub">{sub}</span>}
        {source && <span className="mp-pk-src">{source}</span>}
      </span>
      {right && <span className="mp-pk-right">{right}</span>}
    </button>
  );
}

// ── 메인 목록 ──
function PolicyList({ tree, loc, dispatch, onOpen, toast, onUpload }) {
  const [query, setQuery] = useSL("");
  const [statusFilter, setStatusFilter] = useSL("all"); // all | on | draft | off
  const [catFilter, setCatFilter] = useSL("all");
  const [scope, setScope] = useSL({ type: "all" }); // {type:'all'} | {type:'loose'} | {type:'pkg', id}
  const [selection, setSelection] = useSL(new Set());
  const [density, setDensity] = useSL("cozy"); // cozy | compact

  const [drag, setDrag] = useSL(null);
  const [hotDrop, setHotDrop] = useSL(null);
  const dragData = useRL(null);
  const ghostRef = useRL(null);
  const T = loc === "en" ? EN : KO;

  const onSelect = (uid) => setSelection(s => { const n = new Set(s); n.has(uid) ? n.delete(uid) : n.add(uid); return n; });

  const q = query.trim().toLowerCase();
  const totalRules = tree.loose.length + tree.packages.reduce((a, p) => a + p.members.length, 0);

  // 충돌 계산: dupKey 같고 둘 다 유효 활성인 사본 ≥2
  const conflict = useML(() => {
    const groups = {};
    const add = (r, pkg) => {
      if (!r.dupKey) return;
      (groups[r.dupKey] = groups[r.dupKey] || []).push({ uid: r.uid, active: rowOn(r), name: r.name,
        where: pkg ? MP.t(pkg.name, loc) : (loc === "en" ? "Single" : "단일 정책"), threshold: r.threshold });
    };
    tree.loose.forEach(r => add(r, null));
    tree.packages.forEach(pkg => pkg.members.forEach(m => add(m, pkg)));
    const ids = new Set(); const details = [];
    Object.values(groups).forEach(arr => {
      const act = arr.filter(x => x.active);
      if (act.length >= 2) { act.forEach(x => ids.add(x.uid)); details.push(act); }
    });
    return { ids, details };
  }, [tree, loc]);

  // 전체 플랫 목록 (loose + 모든 멤버) — augmented 필드 부여
  const allRows = useML(() => {
    const out = [];
    tree.loose.forEach(r => out.push({ ...r, _pkgId: null, fromMarket: false, pkgUpdate: false, _pkgName: null }));
    tree.packages.forEach(pkg => {
      const upd = !!(pkg.provenance && pkg.provenance.update);
      const fromMkt = pkg.source === "market";
      pkg.members.forEach(m => out.push({ ...m, _pkgId: pkg.id, fromMarket: fromMkt, pkgUpdate: upd, _pkgName: MP.t(pkg.name, loc) }));
    });
    return out;
  }, [tree, loc]);

  // 카테고리 칩 (현재 데이터에 존재하는 것만)
  const cats = useML(() => {
    const present = new Set();
    allRows.forEach(r => present.add(r.cat));
    return MP.CAT_ORDER.filter(c => present.has(c));
  }, [allRows]);

  // 스코프 적용 → 필터 적용
  const rows = useML(() => {
    let base;
    if (scope.type === "loose") base = allRows.filter(r => r._pkgId === null);
    else if (scope.type === "pkg") base = allRows.filter(r => r._pkgId === scope.id);
    else base = allRows;
    if (q) base = base.filter(r => MP.t(r.name, "ko").toLowerCase().includes(q) || MP.t(r.name, "en").toLowerCase().includes(q) || (r.slug || "").toLowerCase().includes(q));
    if (catFilter !== "all") base = base.filter(r => r.cat === catFilter);
    if (statusFilter === "on") base = base.filter(rowOn);
    if (statusFilter === "draft") base = base.filter(isDraft);
    if (statusFilter === "off") base = base.filter(isOff);
    return base;
  }, [allRows, scope, q, catFilter, statusFilter]);

  const activePkg = scope.type === "pkg" ? tree.packages.find(p => p.id === scope.id) : null;

  // ── 드래그 (행 → 왼쪽 패널 타깃) ──
  const onDragStart = (e, r) => {
    e.preventDefault();
    const isTouch = e.pointerType === "touch";
    const multi = selection.has(r.uid) && selection.size > 1;
    const startX = e.clientX, startY = e.clientY;
    let started = false, longTimer = null;
    const from = r._pkgId || "loose";
    const begin = () => {
      started = true;
      dragData.current = { uid: r.uid, from, multi, ids: multi ? Array.from(selection) : [r.uid] };
      setDrag({ uid: r.uid, name: r.name, cat: r.cat, multi, count: multi ? selection.size : 1 });
      document.body.style.userSelect = "none"; document.body.style.cursor = "grabbing";
    };
    const move = (ev) => {
      const dx = ev.clientX - startX, dy = ev.clientY - startY;
      if (!started) { if (isTouch) return; if (Math.hypot(dx, dy) < 6) return; begin(); }
      if (ghostRef.current) ghostRef.current.style.transform = `translate(${ev.clientX + 14}px, ${ev.clientY + 10}px)`;
      const el = document.elementFromPoint(ev.clientX, ev.clientY);
      const dz = el && el.closest("[data-drop]");
      setHotDrop(dz ? dz.getAttribute("data-drop") : null);
    };
    const up = (ev) => {
      clearTimeout(longTimer);
      window.removeEventListener("pointermove", move); window.removeEventListener("pointerup", up);
      document.body.style.userSelect = ""; document.body.style.cursor = "";
      if (started) {
        const el = document.elementFromPoint(ev.clientX, ev.clientY);
        const dz = el && el.closest("[data-drop]");
        resolveDrop(dragData.current, dz ? dz.getAttribute("data-drop") : null);
      }
      setDrag(null); setHotDrop(null); dragData.current = null;
    };
    if (isTouch) longTimer = setTimeout(begin, 320);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const resolveDrop = (dd, target) => {
    if (!dd || !target) return;
    const { from, ids, uid } = dd;
    if (target === "newpkg") {
      dispatch({ type: "NEW_PKG", uids: ids, from, copy: false });
      setSelection(new Set()); toast(loc === "en" ? "New package created" : "새 패키지를 만들었어요"); return;
    }
    if (target === "scope:loose") {
      ids.forEach(id => dispatch({ type: "EXTRACT", uid: id, copy: false }));
      setSelection(new Set()); toast(loc === "en" ? "Moved to single policies" : "단일 정책으로 옮겼어요"); return;
    }
    if (target === "scope:all") return;
    if (target.startsWith("pkg:")) {
      const toPkg = target.slice(4);
      if (toPkg === from) return;
      ids.forEach(id => dispatch({ type: "MOVE_TO_PKG", uid: id, from, to: toPkg, copy: false }));
      setSelection(new Set());
      const dp = tree.packages.find(p => p.id === toPkg);
      toast((loc === "en" ? "Moved to " : "팩으로 이동 · ") + (dp ? MP.t(dp.name, loc) : "")); return;
    }
  };

  const cs = drag ? catStyle(drag.cat) : null;

  // 왼쪽 팩 요약
  const minePkgs = tree.packages;
  const scopeTitle = scope.type === "all" ? T.allPolicies
    : scope.type === "loose" ? T.singles
    : (activePkg ? MP.t(activePkg.name, loc) : "");

  return (
    <div className="mp-2col">
      {/* ───────── LEFT — package panel ───────── */}
      <aside className="mp-left">
        <div className="mp-left-scroll">
          <div className="mp-left-grp">
            <PackItem active={scope.type === "all"} drop="scope:all" hot={false}
              onClick={() => setScope({ type: "all" })}
              icon={MPI.layers ? MPI.layers() : MPI.shield()} name={T.allPolicies}
              right={<span className="mp-pk-ct">{totalRules}</span>} />
            <PackItem active={scope.type === "loose"} drop="scope:loose" hot={hotDrop === "scope:loose"}
              onClick={() => setScope({ type: "loose" })}
              icon={MPI.dot()} name={T.singles} sub={T.singlesSub}
              right={<span className="mp-pk-ct">{tree.loose.length}</span>} />
          </div>

          <div className="mp-left-sec"><span className="t">{T.myPacks}</span><span className="ct">{minePkgs.length}</span></div>

          <div className="mp-left-grp">
            {minePkgs.map(pkg => {
              const onCt = pkg.members.filter(rowOn).length;
              const prov = pkg.provenance;
              const cstyle = catStyle(pkg.cat);
              return (
                <PackItem key={pkg.id} active={scope.type === "pkg" && scope.id === pkg.id}
                  drop={`pkg:${pkg.id}`} hot={hotDrop === `pkg:${pkg.id}`}
                  onClick={() => setScope({ type: "pkg", id: pkg.id })}
                  icon={<span style={{ color: cstyle.hex, display: "grid", placeItems: "center" }}>{MPI.folder()}</span>}
                  name={MP.t(pkg.name, loc)}
                  badge={prov && prov.update ? <span className="mp-pk-upd" title={loc === "en" ? "Update available" : "업데이트 있음"}>{MPI.warn()}</span> : (pkg.featured ? <span className="mp-pk-feat">{loc === "en" ? "Featured" : "대표"}</span> : null)}
                  sub={<><b>{onCt}</b>/{pkg.members.length} {loc === "en" ? "on" : "켜짐"}</>}
                  source={prov
                    ? <>{MPI.shield()}{loc === "en" ? "From market" : "마켓에서 가져옴"} · {prov.ver}{prov.modified > 0 ? (loc === "en" ? ` · ${prov.modified} edited` : ` · ${prov.modified}개 수정`) : ""}</>
                    : <>{MPI.pencil()}{loc === "en" ? "Built by me" : "내가 만듦"}</>}
                />
              );
            })}
          </div>

          <div className={`mp-newpkg ${hotDrop === "newpkg" ? "drop-hot" : ""}`} data-drop="newpkg">
            <span className="ico">{MPI.plus()}{T.newPkgZone}</span>
          </div>
        </div>
      </aside>

      {/* ───────── RIGHT — policy list ───────── */}
      <section className="mp-right">
        {/* control bar */}
        <div className="mp-ctrl">
          <div className="mp-search">
            {MPI.search()}
            <input value={query} onChange={e => setQuery(e.target.value)} placeholder={T.searchPh} />
            {!query && <kbd>/</kbd>}
          </div>
          <span className="mp-spc" />
          <div className="mp-seg">
            <button className={statusFilter === "all" ? "on" : ""} onClick={() => setStatusFilter("all")}>{T.all}</button>
            <button className={statusFilter === "on" ? "on" : ""} onClick={() => setStatusFilter("on")}>{T.onOnly}</button>
            <button className={statusFilter === "draft" ? "on" : ""} onClick={() => setStatusFilter("draft")}>{T.draftOnly}</button>
            <button className={statusFilter === "off" ? "on" : ""} onClick={() => setStatusFilter("off")}>{T.offOnly}</button>
          </div>
          <div className="mp-density" title={loc === "en" ? "Row density" : "행 밀도"}>
            <button className={density === "cozy" ? "on" : ""} onClick={() => setDensity("cozy")}>{T.cozy}</button>
            <button className={density === "compact" ? "on" : ""} onClick={() => setDensity("compact")}>{T.compact}</button>
          </div>
        </div>

        {/* category chips */}
        <div className="mp-catbar">
          <button className={`mp-catchip ${catFilter === "all" ? "on" : ""}`} onClick={() => setCatFilter("all")}>{T.allCat}</button>
          {cats.map(c => (
            <button key={c} className={`mp-catchip ${catFilter === c ? "on" : ""}`} onClick={() => setCatFilter(c)}>
              <span className="dot" style={{ background: catStyle(c).hex }} />{MP.t(MP.CAT[c], loc)}
            </button>
          ))}
        </div>

        {/* scope header */}
        <div className="mp-scopehd">
          <div className="mp-scope-title">
            <span className="t">{scopeTitle}</span>
            <span className="ct">{rows.length}{loc === "en" ? "" : "개"}</span>
            {activePkg && activePkg.provenance && <span className="mp-scope-prov">{MPI.shield()}{loc === "en" ? "From market" : "마켓에서 가져옴"} · {MP.t(activePkg.provenance.from, loc)} {activePkg.provenance.ver}</span>}
            {activePkg && !activePkg.provenance && <span className="mp-scope-prov mine">{MPI.pencil()}{loc === "en" ? "Built by me" : "내가 만듦"}</span>}
          </div>
          <span className="mp-spc" />
          {activePkg && activePkg.source === "mine" && (
            <button className="mp-upload-cta" onClick={() => onUpload(activePkg)} title={loc === "en" ? "Publish to market" : "마켓에 올리기"}>
              {MPI.shield()}{loc === "en" ? "Publish to market" : "마켓에 올리기"}
            </button>
          )}
          {scope.type !== "all" && <button className="mp-scope-clear" onClick={() => setScope({ type: "all" })}>{MPI.x()}{T.clearScope}</button>}
        </div>

        <div className="mp-scroll">
          {/* 충돌 배너 (contextual) */}
          {conflict.details.length > 0 && (
            <div className="mp-conflict-banner">
              <div className="cb-h">{MPI.warn()}<b>{T.conflictTitle}</b><span className="cb-sub">{T.conflictBody}</span></div>
              {conflict.details.map((grp, gi) => (
                <div key={gi} className="cb-grp">
                  {grp.map((x, i) => (
                    <span key={i} className="cb-chip"><span className="nm">{MP.t(x.name, loc)}</span><span className="wh">{x.where}</span>{x.threshold && <span className="thr">{MP.t(x.threshold, loc)}</span>}</span>
                  ))}
                  <button className="cb-act" onClick={() => dispatch({ type: "TOGGLE", uid: grp[0].uid })}>{T.turnOffOne}</button>
                </div>
              ))}
            </div>
          )}

          {/* table */}
          <div className={`mp-table ${density}`}>
            <div className="mp-thead">
              <div className="mp-c-sel">
                <button className={`mp-selbox head ${selection.size > 0 ? "on" : ""}`}
                  onClick={() => { if (selection.size > 0) setSelection(new Set()); else setSelection(new Set(rows.map(r => r.uid))); }}
                  title={loc === "en" ? "Select all in view" : "보이는 항목 전체 선택"}>{selection.size > 0 && MPI.check()}</button>
              </div>
              <div className="mp-c-name">{T.hName}</div>
              <div className="mp-c-cat">{T.hCat}</div>
              <div className="mp-c-sev">{T.hSev}</div>
              <div className="mp-c-flag">{T.hFlag}</div>
              <div className="mp-c-time">{T.hTime}</div>
              <div className="mp-c-act">{T.hState}</div>
            </div>

            {rows.map(r => (
              <RuleRow key={r.uid} r={r} loc={loc} dispatch={dispatch} onOpen={onOpen}
                onDragStart={onDragStart} dragId={drag && drag.uid}
                selected={selection.has(r.uid)} onSelect={onSelect}
                conflict={conflict.ids.has(r.uid)} scopeIsAll={scope.type === "all"} />
            ))}

            {rows.length === 0 && (
              <div className="mp-empty"><div className="big">{T.emptyBig}</div><div className="sm">{T.emptySm}</div></div>
            )}
          </div>
        </div>

        {/* 선택 세트 액션 바 (다중선택 → 일괄 on/off + 패키지로 묶기) */}
        {selection.size > 0 && (
          <div className="mp-selbar">
            <span className="ct"><b>{selection.size}</b> {T.selected}</span>
            <button className="ghost" onClick={() => setSelection(new Set())}>{T.clearSel}</button>
            <span className="spc" />
            <button onClick={() => { dispatch({ type: "SET_MANY", uids: Array.from(selection), on: true }); toast(loc === "en" ? `${selection.size} turned on` : `${selection.size}개 켰어요`); }}>
              {T.bulkOn.replace("{n}", selection.size)}
            </button>
            <button onClick={() => { dispatch({ type: "SET_MANY", uids: Array.from(selection), on: false }); toast(loc === "en" ? `${selection.size} turned off` : `${selection.size}개 껐어요`); }}>
              {T.bulkOff.replace("{n}", selection.size)}
            </button>
            <button className="go" onClick={() => { dispatch({ type: "NEW_PKG", uids: Array.from(selection), from: "loose", copy: false }); setSelection(new Set()); toast(loc === "en" ? "New private package created" : "새 패키지(비공개)를 만들었어요"); }}>
              {MPI.folder({ style: { width: 14, height: 14 } })} {T.makePkg}
            </button>
          </div>
        )}
      </section>

      {/* 드래그 고스트 */}
      {drag && (
        <div className="mp-ghost" ref={ghostRef} style={{ transform: "translate(-400px,-400px)" }}>
          <span className="gcat" style={cs.iconWrap}><CatIcon cat={drag.cat} /></span>
          <span className="gn">{MP.t(drag.name, loc)}</span>
          {drag.multi && <span className="mp-ghost-multi">{drag.count}</span>}
        </div>
      )}
    </div>
  );
}

const KO = {
  searchPh: "정책 이름·slug 검색…", all: "전체", draftOnly: "수정중", onOnly: "켜진 것", offOnly: "꺼짐",
  allCat: "모든 카테고리", cozy: "여유", compact: "촘촘",
  allPolicies: "전체", singles: "단일 정책", singlesSub: "어느 팩에도 안 든 내 정책", myPacks: "내 패키지",
  newPkgZone: "여기로 끌어와 새 패키지",
  clearScope: "전체 보기", clearSel: "해제", selected: "개 선택됨",
  bulkOn: "{n}개 켜기", bulkOff: "{n}개 끄기", makePkg: "패키지로 묶기",
  hName: "정책", hCat: "카테고리", hSev: "심각도", hFlag: "알림", hSrc: "출처", hTime: "마지막 수정", hState: "상태",
  emptyBig: "표시할 정책이 없어요", emptySm: "필터를 바꾸거나 다른 팩을 골라보세요",
  conflictTitle: "중복 충돌", conflictBody: "같은 동작 두 사본이 둘 다 켜져 verdict가 충돌합니다. 하나를 끄세요.",
  turnOffOne: "한쪽 끄기"
};
const EN = {
  searchPh: "Search name or slug…", all: "All", draftOnly: "Draft", onOnly: "On", offOnly: "Off",
  allCat: "All categories", cozy: "Cozy", compact: "Compact",
  allPolicies: "All policies", singles: "Single policies", singlesSub: "My policies in no pack", myPacks: "My packages",
  newPkgZone: "Drag here to make a package",
  clearScope: "Show all", clearSel: "Clear", selected: "selected",
  bulkOn: "Turn on {n}", bulkOff: "Turn off {n}", makePkg: "Make a package",
  hName: "Policy", hCat: "Category", hSev: "Severity", hFlag: "Alert", hSrc: "Source", hTime: "Last edited", hState: "State",
  emptyBig: "No policies to show", emptySm: "Change filters or pick another pack",
  conflictTitle: "Duplicate conflict", conflictBody: "Two copies of the same action are both on — verdicts conflict. Turn one off.",
  turnOffOne: "Turn one off"
};

Object.assign(window, { PolicyList });
