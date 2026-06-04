// mypolicy-main.jsx — 앱 루트: 트리 상태(이동/복사/떼어내기/새패키지/토글) + 목록↔에디터 라우팅

const { useReducer: useRdM, useState: useSM, useEffect: useEM } = React;

let UID = 1;
function nextUid(p) { return (p || "u") + "-" + (UID++) + "-" + Date.now().toString(36); }

// 초기 트리 빌드
function buildTree() {
  const loose = MP.SINGLES.map(s => ({ uid: s.id, id: s.id, name: s.name, cat: s.cat, method: s.method,
    formCapable: s.formCapable, slug: s.slug, sev: s.sev, dupKey: s.dupKey, threshold: s.threshold,
    life: s.state.life, on: s.state.on, source: "mine" }));
  const packages = MP.PACKAGES.map(p => ({
    id: p.id, source: p.source, featured: p.featured, provenance: p.provenance, cat: p.cat, name: p.name, tagline: p.tagline, version: p.version,
    members: p.members.map(m => ({ uid: p.id + ":" + m.id, id: m.id, name: m.name, cat: m.cat || p.cat, method: m.method,
      slug: m.slug, sev: m.sev, dupKey: m.dupKey, threshold: m.threshold, on: m.on, source: p.source }))
  }));
  return { packages, loose };
}

function findRow(tree, uid) {
  const li = tree.loose.findIndex(r => r.uid === uid);
  if (li >= 0) return { row: tree.loose[li], container: "loose", idx: li };
  for (const pkg of tree.packages) {
    const mi = pkg.members.findIndex(m => m.uid === uid);
    if (mi >= 0) return { row: pkg.members[mi], container: pkg.id, idx: mi, pkg };
  }
  return null;
}
function cloneRow(r, asMine) {
  const c = { ...r, uid: nextUid("copy"), threshold: r.threshold };
  if (asMine) { c.source = "mine"; }
  return c;
}

function treeReducer(state, a) {
  const tree = { packages: state.packages.map(p => ({ ...p, members: p.members.slice() })), loose: state.loose.slice() };
  switch (a.type) {
    case "TOGGLE": {
      const f = findRow(tree, a.uid); if (!f) return state;
      if (f.container === "loose") {
        if (f.row.life === "draft") return state; // draft는 토글 불가
        tree.loose[f.idx] = { ...f.row, on: !f.row.on };
      } else {
        const pkg = tree.packages.find(p => p.id === f.container);
        pkg.members = pkg.members.map(m => m.uid === a.uid ? { ...m, on: !m.on } : m);
      }
      return tree;
    }
    case "EXTRACT": {
      const f = findRow(tree, a.uid); if (!f || f.container === "loose") return state;
      const pkg = tree.packages.find(p => p.id === f.container);
      const copy = a.copy || pkg.readOnly;
      const newRow = { ...f.row, uid: nextUid("ext"), source: "mine", life: "publish", on: true };
      if (!copy) pkg.members = pkg.members.filter(m => m.uid !== a.uid);
      tree.loose = [newRow, ...tree.loose];
      return tree;
    }
    case "MOVE_TO_PKG": {
      const f = findRow(tree, a.uid); if (!f) return state;
      const dest = tree.packages.find(p => p.id === a.to); if (!dest || dest.readOnly) return state;
      const srcReadOnly = f.container !== "loose" && (tree.packages.find(p => p.id === f.container) || {}).readOnly;
      const copy = a.copy || srcReadOnly;
      const moved = { ...f.row, uid: copy ? nextUid("mv") : f.row.uid, source: "mine" };
      if (moved.life) { moved.on = moved.life === "publish" ? moved.on : true; } // 단일 → 멤버: on 유지
      if (!copy) {
        if (f.container === "loose") tree.loose = tree.loose.filter(r => r.uid !== a.uid);
        else { const sp = tree.packages.find(p => p.id === f.container); sp.members = sp.members.filter(m => m.uid !== a.uid); }
      }
      dest.members = [...dest.members, moved];
      return tree;
    }
    case "NEW_PKG": {
      const rows = [];
      a.uids.forEach(uid => {
        const f = findRow(tree, uid); if (!f) return;
        const srcReadOnly = f.container !== "loose" && (tree.packages.find(p => p.id === f.container) || {}).readOnly;
        const copy = a.copy || srcReadOnly;
        rows.push({ ...f.row, uid: copy ? nextUid("np") : f.row.uid, source: "mine" });
        if (!copy) {
          if (f.container === "loose") tree.loose = tree.loose.filter(r => r.uid !== uid);
          else { const sp = tree.packages.find(p => p.id === f.container); sp.members = sp.members.filter(m => m.uid !== uid); }
        }
      });
      if (!rows.length) return state;
      const npkg = { id: nextUid("pkg"), source: "mine", readOnly: false, cat: rows[0].cat,
        name: { ko: "새 패키지", en: "New package" }, tagline: { ko: "직접 묶은 정책 세트", en: "Hand-picked policy set" }, version: null,
        members: rows };
      tree.packages = [...tree.packages, npkg];
      return tree;
    }
    case "ADD_SINGLE": {
      tree.loose = [a.row, ...tree.loose];
      return tree;
    }
    case "SET_MANY": {
      const set = new Set(a.uids);
      tree.loose = tree.loose.map(r => (set.has(r.uid) && r.life !== "draft") ? { ...r, on: a.on } : r);
      tree.packages = tree.packages.map(p => ({ ...p, members: p.members.map(m => set.has(m.uid) ? { ...m, on: a.on } : m) }));
      return tree;
    }
    case "RENAME": {
      const nm = { ko: a.name, en: a.name };
      const f = findRow(tree, a.uid); if (!f) return state;
      if (f.container === "loose") tree.loose[f.idx] = { ...f.row, name: nm };
      else { const pkg = tree.packages.find(p => p.id === f.container); pkg.members = pkg.members.map(m => m.uid === a.uid ? { ...m, name: nm } : m); }
      return tree;
    }
    default: return state;
  }
}

function App() {
  const [tree, dispatch] = useRdM(treeReducer, null, buildTree);
  const [loc, setLoc] = useSM("ko");
  const [screen, setScreen] = useSM({ name: "list" });
  const [curPolicy, setCurPolicy] = useSM(null);
  const [toast, setToast] = useSM(null);
  const [newModal, setNewModal] = useSM(false);
  const [uploadPkg, setUploadPkg] = useSM(null);

  const fireToast = (msg, mono) => { setToast({ msg, mono }); clearTimeout(window.__mpT); window.__mpT = setTimeout(() => setToast(null), 2600); };

  const onOpen = (row) => { setCurPolicy(row); setScreen({ name: "editor", uid: row.uid }); };
  const onBack = () => { setScreen({ name: "list" }); };

  const onNewPolicy = () => setNewModal(true);
  const createPolicy = (method) => {
    const row = { uid: nextUid("new"), id: nextUid("p"), name: { ko: "새 정책 (제목 없음)", en: "New policy (untitled)" },
      cat: "core", method, formCapable: method === "form", slug: "untitled-policy", sev: "warn",
      life: "draft", on: false, source: "mine", _startView: method };
    dispatch({ type: "ADD_SINGLE", row });
    setNewModal(false); setCurPolicy(row); setScreen({ name: "editor", uid: row.uid });
    fireToast(loc === "en" ? "New draft created — auto-excluded until published" : "새 draft 생성 — 발행 전까지 평가 제외");
  };

  // 정책 제목 인라인 편집 → 트리 + 현재 정책 동기화
  const onRename = (p, name) => {
    dispatch({ type: "RENAME", uid: p.uid, name });
    setCurPolicy(c => c ? { ...c, name: { ko: name, en: name } } : c);
  };


  // 키보드: '/' 검색 포커스, Esc 뒤로
  useEM(() => {
    const h = (e) => {
      if (e.key === "Escape" && screen.name === "editor") onBack();
      if (e.key === "/" && screen.name === "list" && !/input|textarea/i.test((e.target.tagName || ""))) {
        const el = document.querySelector(".mp-search input"); if (el) { e.preventDefault(); el.focus(); }
      }
    };
    window.addEventListener("keydown", h); return () => window.removeEventListener("keydown", h);
  }, [screen]);

  const inEditor = screen.name === "editor" && curPolicy;
  const T = loc === "en"
    ? { title: "My Policy", sub: "· organize · edit", back: "All policies", newp: "New policy" }
    : { title: "My Policy", sub: "· 정리 · 편집", back: "전체 정책", newp: "새 정책" };

  return (
    <div className="mp-shell">
      <NavRail onNewPolicy={onNewPolicy} loc={loc} />
      <div className="mp-root">
        {/* topbar */}
        <div className="mp-top">
          <div className="mp-tb">
            {inEditor
              ? <button className="mp-back" onClick={onBack}>{MPI.back()}{T.back}</button>
              : <div className="mp-ident"><span className="mp-mark">{MPI.shield()}</span><span className="mp-title">{T.title} <span className="sub">{T.sub}</span></span></div>}
            <span className="mp-spc" />
            {!inEditor && <span className="mp-count">{tree.loose.length + tree.packages.reduce((a, p) => a + p.members.length, 0)} {loc === "en" ? "rules" : "규칙"}</span>}
            <span className="mp-vbar" />
            <div className="mp-loc"><button className={loc === "ko" ? "on" : ""} onClick={() => setLoc("ko")}>KO</button><button className={loc === "en" ? "on" : ""} onClick={() => setLoc("en")}>EN</button></div>
            {!inEditor && <button className="mp-pri" onClick={onNewPolicy}>{MPI.plus()}{T.newp}</button>}
          </div>
        </div>

        {inEditor
          ? <PolicyEditor policy={curPolicy} loc={loc} onRename={onRename} toast={fireToast} />
          : <div className="mp-body"><PolicyList tree={tree} loc={loc} dispatch={dispatch} onOpen={onOpen} toast={fireToast} onUpload={setUploadPkg} /></div>}
      </div>

      {/* 새 정책: 방식 선택 chooser (폼 / 블록 / Cedar — 한 정책 = 한 방식 고정) */}
      {newModal && <NewPolicyChooser loc={loc} onPick={createPolicy} onClose={() => setNewModal(false)} />}

      {/* 마켓 업로드 흐름 (비식별 → 이름·설명 → 공개) */}
      {uploadPkg && <UploadModal pkg={uploadPkg} loc={loc} onClose={() => setUploadPkg(null)} toast={fireToast} />}

      {toast && <div className="mp-toast">{MPI.check()}<span>{toast.msg}{toast.mono && <> · <span className="mono">{toast.mono}</span></>}</span></div>}
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")).render(<App />);
