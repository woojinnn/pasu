// editor-v6-palette.jsx — Block palette as a nested .cedarschema tree.
//   • Single `actions/` root, categories nested inside.
//   • Docked left panel (resizable by parent). No top toggle bar.
//   • Caret toggles expand/collapse; clicking a FOLDER NAME enters focus mode
//     (isolates that folder); files stay select/drag.
//   • System section read-only (lock + disabled actions); custom section editable.

const { useState: useSPal, useEffect: useEPal, useRef: useRPal, useMemo: useMPal } = React;

function v6Hl(text, q) {
  if (!q) return text;
  const i = text.toLowerCase().indexOf(q.toLowerCase());
  if (i < 0) return text;
  return (<>{text.slice(0, i)}<mark>{text.slice(i, i + q.length)}</mark>{text.slice(i + q.length)}</>);
}
function v6FileName(name, q) {
  const m = name.match(/^(.*?)(\.cedarschema)$/);
  const base = m ? m[1] : name; const ext = m ? m[2] : '';
  return (<>{v6Hl(base, q)}{ext && <span className="ext">{ext}</span>}</>);
}

function V6PaletteActions({ system, allExpanded, onToggleAll, onFilter, onFile, onFolder, onSync, onCollapse }) {
  if (system) {
    return (
      <div className="tr-acts">
        <button className="tr-act" title={allExpanded ? '전체 접기' : '전체 펼치기'} onClick={onToggleAll}>{allExpanded ? <V6I.collapseAll /> : <V6I.expandAll />}</button>
        <button className="tr-act" title="검색으로 필터" onClick={onFilter}><V5I.search /></button>
      </div>
    );
  }
  return (
    <div className="tr-acts">
      <button className="tr-act" title="파일 추가 (커스텀 블록 정의)" onClick={onFile}><V6I.filePlus /></button>
      <button className="tr-act" title="폴더 추가" onClick={onFolder}><V6I.folderPlus /></button>
      <button className="tr-act" title="외부 manifest 소스 재로드" onClick={onSync}><V6I.sync /></button>
      <button className="tr-act" title="전체 접기" onClick={onCollapse}><V6I.collapseAll /></button>
    </div>
  );
}

function V6Palette({
  onAddBlock, customTree, dispatch,
  onOpenFileModal, onOpenFolderModal, onSync,
  focusPath, setFocusPath, revealPath,
}) {
  const [q, setQ] = useSPal('');
  const scrollRef = useRPal(null);
  const searchRef = useRPal(null);
  const rowRefs = useRPal({});

  // all system folder paths open by default (expanded main mockup)
  const allFolderPaths = useMPal(() => {
    const out = []; (function w(n) { if (n.type === 'folder') { out.push(n.path); (n.children || []).forEach(w); } })(V6_TREE); return out;
  }, []);
  const [openPaths, setOpenPaths] = useSPal(() => new Set(allFolderPaths.filter(p => p.split('/').length <= 2)));
  const toggleOpen = (path) => setOpenPaths(s => { const n = new Set(s); n.has(path) ? n.delete(path) : n.add(path); return n; });
  const allExpanded = openPaths.size >= allFolderPaths.length;
  const toggleAllSystem = () => setOpenPaths(allExpanded ? new Set() : new Set(allFolderPaths));

  // auto-scroll the tree to the selected canvas block's source file
  useEPal(() => {
    if (!revealPath) return;
    if (V6_BLOCKS[revealPath]) {
      const parts = revealPath.split('/'); let acc = ''; const anc = [];
      parts.slice(0, -1).forEach(p => { acc = acc ? acc + '/' + p : p; anc.push(acc); });
      setOpenPaths(s => { const n = new Set(s); anc.forEach(a => n.add(a)); return n; });
    }
    const tt = setTimeout(() => {
      const el = rowRefs.current[revealPath]; const sc = scrollRef.current;
      if (el && sc) { sc.scrollTo({ top: Math.max(0, el.offsetTop - 96), behavior: 'smooth' }); el.classList.add('tr-flash'); setTimeout(() => el.classList.remove('tr-flash'), 1200); }
    }, 90);
    return () => clearTimeout(tt);
  }, [revealPath]);

  const ql = q.trim().toLowerCase();
  const searching = ql.length > 0;

  const drag = (e, def) => { e.dataTransfer.setData('text/v6-block', JSON.stringify(def)); e.dataTransfer.effectAllowed = 'copy'; };

  // does a node (or descendant) match the query?
  const nodeMatches = (node) => {
    if (node.type === 'file') return node.seg.toLowerCase().includes(ql) || node.path.toLowerCase().includes(ql);
    return (node.children || []).some(nodeMatches);
  };

  // ── render a system tree node ──
  const renderNode = (node, depth) => {
    if (searching && !nodeMatches(node)) return null;
    if (node.type === 'file') {
      const def = V6_BLOCKS[node.path];
      return (
        <div key={node.path} className="tr-row file sys" draggable
          ref={el => { if (el) rowRefs.current[node.path] = el; }}
          style={{ paddingLeft: 8 + depth * 16 }}
          onDragStart={(e) => drag(e, def)}
          onClick={() => onAddBlock(def)}
          title="드래그하여 캔버스에 드롭 · 클릭하여 추가">
          <span className="tr-caret spacer" />
          <span className="tr-ic file sys"><V6I.file /></span>
          <span className="tr-name">{v6FileName(node.seg, ql)}</span>
          <V6I.lock className="tr-lock" />
        </div>
      );
    }
    const open = searching ? true : openPaths.has(node.path);
    return (
      <div key={node.path}>
        <div className="tr-row folder" style={{ paddingLeft: 8 + depth * 16, top: 40 + depth * 30, zIndex: 24 - depth }}>
          <button className={`tr-caret btn ${open ? '' : 'closed'}`} title={open ? '접기' : '펼치기'}
            onClick={(e) => { e.stopPropagation(); toggleOpen(node.path); }}>
            <V5I.caretDown style={{ width: 12, height: 12 }} />
          </button>
          <span className="tr-folder-hit" title="이 폴더만 보기 (focus)" onClick={() => setFocusPath(node.path)}>
            <span className="tr-ic folder sys">{open ? <V6I.folderOpen /> : <V6I.folder />}</span>
            <span className="tr-name">{v6Hl(node.seg, ql)}</span>
          </span>
          <span className="tr-focusable" title="이 폴더만 보기" onClick={() => setFocusPath(node.path)}><V6I.focusIn /></span>
          <span className="tr-count">{v6CountFiles(node)}</span>
        </div>
        {open && <div className="tr-children sys">{(node.children || []).map(c => renderNode(c, depth + 1))}</div>}
      </div>
    );
  };

  // ── system section content (focus-aware) ──
  let systemContent;
  if (searching) {
    const r = renderNode(V6_TREE, 0);
    systemContent = r || <div className="tr-noresult">‘<b>{q}</b>’ 와(과) 일치하는 schema 없음</div>;
  } else if (focusPath) {
    const f = v6FindFolder(focusPath);
    systemContent = f ? (f.children || []).map(c => renderNode(c, 0)) : null;
  } else {
    systemContent = renderNode(V6_TREE, 0);
  }

  // focus breadcrumb segments
  const focusSegs = focusPath ? focusPath.split('/') : [];

  // ── custom tree ──
  const customMatches = (node) => {
    if (!ql) return true;
    if (node.kind === 'file') return node.name.toLowerCase().includes(ql);
    return (node.children || []).some(customMatches);
  };
  const renderCustomNode = (node, depth) => {
    if (ql && !customMatches(node)) return null;
    if (node.kind === 'folder') {
      const open = ql ? true : node.open !== false;
      return (
        <div key={node.id}>
          <div className="tr-row folder" style={{ paddingLeft: 8 + depth * 16, top: 40 + depth * 30, zIndex: 24 - depth }} onClick={() => dispatch({ type: 'TOGGLE_CUSTOM_FOLDER', id: node.id })}>
            <span className={`tr-caret ${open ? '' : 'closed'}`}><V5I.caretDown style={{ width: 12, height: 12 }} /></span>
            <span className="tr-ic folder">{open ? <V6I.folderOpen /> : <V6I.folder />}</span>
            <span className="tr-name">{v6Hl(node.name, ql)}</span>
            <span className="tr-count">{(node.children || []).length}</span>
          </div>
          {open && <div className="tr-children">{(node.children || []).map(c => renderCustomNode(c, depth + 1))}</div>}
        </div>
      );
    }
    const tone = (node.tone && V6_TONES[node.tone]) ? V6_TONES[node.tone].border : 'var(--slate-300)';
    return (
      <div key={node.id} className="tr-row file" draggable style={{ paddingLeft: 8 + depth * 16 }}
        ref={el => { if (el) rowRefs.current['custom/' + node.id] = el; }}
        onDragStart={(e) => drag(e, v6CustomDef(node))}
        onClick={() => onAddBlock(v6CustomDef(node))}
        title="드래그하여 캔버스에 드롭 · 클릭하여 추가">
        <span className="tr-caret spacer" />
        <span className="tr-tone-dot" style={{ background: tone }} />
        <span className="tr-ic file"><V6I.file /></span>
        <span className="tr-name">{v6FileName(node.name, ql)}</span>
        <V6I.grab className="tr-grab" />
      </div>
    );
  };

  return (
    <div className="pal-inner">
      <div className="trh">
        <V6I.palette style={{ width: 16, height: 16, color: 'var(--slate-500)' }} />
        <span className="trh-t">Block palette</span>
      </div>

      <div className="tr-search">
        <V5I.search style={{ width: 13, height: 13, color: 'var(--slate-400)' }} />
        <input ref={searchRef} value={q} onChange={(e) => setQ(e.target.value)} placeholder="schema 검색 · swap, perp, erc20…" />
        {q && <button className="tr-clear" onClick={() => setQ('')}><V5I.x style={{ width: 12, height: 12 }} /></button>}
      </div>

      {/* focus-mode breadcrumb */}
      {focusPath && !searching && (
        <div className="tr-focusbar">
          <nav className="tr-fb">
            {focusSegs.map((seg, i) => (
              <React.Fragment key={i}>
                {i > 0 && <span className="tr-fb-sep">/</span>}
                <button className={`tr-fb-seg ${i === focusSegs.length - 1 ? 'leaf' : ''}`}
                  onClick={() => { if (i === 0) setFocusPath(null); else setFocusPath(focusSegs.slice(0, i + 1).join('/')); }}>
                  {seg}
                </button>
              </React.Fragment>
            ))}
          </nav>
          <button className="tr-fb-exit" onClick={() => setFocusPath(null)} title="focus 해제"><V5I.x style={{ width: 11, height: 11 }} />Exit</button>
        </div>
      )}

      <div className="tr-scroll" ref={scrollRef}>
        {/* SYSTEM */}
        <div className="tr-sec-h">
          <span className="tr-sec-lab">
            <span className="tr-sec-t">기본 필드 · schema</span>
            <span className="tr-sec-lock"><V6I.lock />read-only</span>
          </span>
          <V6PaletteActions system allExpanded={allExpanded} onToggleAll={toggleAllSystem} onFilter={() => searchRef.current && searchRef.current.focus()} />
        </div>
        <div className="tr-list">{systemContent}</div>

        <div className="tr-sec-div" />

        {/* CUSTOM */}
        <div className="tr-sec-h">
          <span className="tr-sec-lab">
            <span className="tr-sec-t">커스텀 · enrichment</span>
            <span className="tr-sec-editable"><V6I.edit2 />editable</span>
          </span>
          <V6PaletteActions
            onFile={() => onOpenFileModal(null)}
            onFolder={() => onOpenFolderModal(null)}
            onSync={onSync}
            onCollapse={() => { (function walk(list) { (list || []).forEach(n => { if (n.kind === 'folder') { if (n.open !== false) dispatch({ type: 'TOGGLE_CUSTOM_FOLDER', id: n.id }); walk(n.children); } }); })(customTree); }}
          />
        </div>
        <div className="tr-list">
          {(customTree || []).length === 0 ? (
            <div className="tr-empty-custom">아직 커스텀 enrichment 블록이 없습니다.<br /><b>파일 추가</b> 로 manifest 블록을 정의하세요.</div>
          ) : (customTree || []).map(n => renderCustomNode(n, 0))}
        </div>
      </div>

      <div className="tr-foot">
        <V6I.grab style={{ width: 13, height: 13 }} />
        <span>파일을 캔버스로 드래그하면 블록이 생성됩니다.</span>
      </div>
    </div>
  );
}

function v6CustomDef(node) {
  return { id: 'custom/' + node.id, name: node.name, leafType: node.leafType || 'schema',
    tone: node.tone || null, shape: node.shape || 'rounded', custom: true, absence: node.absence || 'false' };
}

Object.assign(window, { V6Palette, v6CustomDef, v6Hl, v6FileName });
