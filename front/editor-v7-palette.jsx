// editor-v7-palette.jsx — docked palette. Reuses V6_TREE / V6_BLOCKS (367 fields).
// Search-first (⌘K), category → action → role-colored chip grid, locale toggle.

const { useState: useSPal7, useMemo: useMPal7, useRef: useRPal7, useEffect: useEPal7 } = React;

const CAT_DOT = {
  airdrop: 'var(--sage-500)', amm: 'var(--cyan-500)', launchpad: 'var(--warn-400)',
  lending: 'var(--sage-600)', perp: 'var(--fail-400)', token: 'var(--cyan-600)', core: 'var(--slate-400)',
};

function chipVars(def) { return roleVars(def.param, def.fieldKind); }

function V7Chip({ def, locale, onAdd }) {
  const role = v7RoleOf(def.param, def.fieldKind);
  const Icon = ROLE_ICON[role] || V7I.dot;
  const live = v7IsLive(def.param);
  return (
    <button className={`v7-chip role-${role} ${live ? 'is-live' : ''}`} style={chipVars(def)}
      title={def.param} onClick={() => onAdd(def)} draggable
      onDragStart={(e) => { e.dataTransfer.setData('text/v7block', def.id); }}>
      <Icon />{v7Display(def.param, locale)}{live && <span className="lv">live</span>}
    </button>
  );
}

function V7Action({ node, blocks, locale, onAdd }) {
  const [open, setOpen] = useSPal7(false);
  const fields = useMPal7(() => {
    const out = [];
    const walk = (n) => {
      if (n.type === 'file' && blocks[n.path]) out.push(blocks[n.path]);
      else if (n.children) n.children.forEach(walk);
    };
    (node.children || []).forEach(walk);
    return out;
  }, [node]);
  const actName = node.seg.replace(/\.cedarschema$/, '');
  return (
    <div className={`v7-act ${open ? 'open' : ''}`}>
      <div className="v7-act-h" onClick={() => setOpen(o => !o)}>
        <V7I.caret style={{ width: 12, height: 12, color: 'var(--slate-300)', transform: open ? 'rotate(90deg)' : 'none', transition: 'transform 120ms' }} />
        <span className="v7-act-nm">{actName}</span>
        <span className="v7-act-ct">{fields.length}</span>
      </div>
      {open && (
        <div className="v7-chips">
          {fields.map(d => <V7Chip key={d.id} def={d} locale={locale} onAdd={onAdd} />)}
        </div>
      )}
    </div>
  );
}

function V7Category({ node, blocks, locale, onAdd, defaultOpen }) {
  const [open, setOpen] = useSPal7(defaultOpen);
  const actions = (node.children || []).filter(c => c.type === 'folder');
  return (
    <div className={`v7-cat ${open ? 'open' : ''}`}>
      <div className="v7-cat-h" onClick={() => setOpen(o => !o)}>
        <V7I.caret className="v7-cat-car" style={{ width: 13, height: 13 }} />
        <span className="v7-cat-dot" style={{ background: CAT_DOT[node.seg] || 'var(--slate-400)' }} />
        <span className="v7-cat-nm">{node.seg}</span>
        <span className="v7-cat-ct">{actions.length}</span>
      </div>
      {open && actions.map(a => <V7Action key={a.path} node={a} blocks={blocks} locale={locale} onAdd={onAdd} />)}
    </div>
  );
}

function V7Palette({ locale, onLocale, onAdd }) {
  const [q, setQ] = useSPal7('');
  const inputRef = useRPal7(null);
  const tree = window.V6_TREE, blocks = window.V6_BLOCKS || {};
  const cats = (tree && tree.children || []).filter(c => c.type === 'folder');
  const total = Object.keys(blocks).length;

  useEPal7(() => {
    const h = (e) => { if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') { e.preventDefault(); inputRef.current && inputRef.current.focus(); } };
    window.addEventListener('keydown', h);
    return () => window.removeEventListener('keydown', h);
  }, []);

  const results = useMPal7(() => {
    if (!q.trim()) return null;
    const needle = q.trim().toLowerCase();
    return Object.values(blocks).filter(d => {
      const dn = v7Display(d.param, locale).toLowerCase();
      return d.param.toLowerCase().includes(needle) || dn.includes(needle) || (d.name || '').toLowerCase().includes(needle);
    }).slice(0, 60);
  }, [q, locale]);

  return (
    <aside className="v7-palette">
      <div className="v7-pal-h">
        <div className="v7-pal-title">
          <span className="t">블록 팔레트</span>
          <span className="ct">{total} fields · 45 actions</span>
        </div>
        <div className="v7-pal-search">
          <V7I.search />
          <input ref={inputRef} value={q} onChange={(e) => setQ(e.target.value)} placeholder="필드 검색 (⌘K)" />
          {!q && <kbd>⌘K</kbd>}
        </div>
      </div>
      <div className="v7-pal-scroll">
        {results ? (
          <div className="v7-sr">
            {results.length === 0 && <div style={{ padding: 16, textAlign: 'center', color: 'var(--slate-300)', fontSize: 12 }}>일치하는 필드 없음</div>}
            {results.map(d => {
              const role = v7RoleOf(d.param, d.fieldKind);
              const Icon = ROLE_ICON[role] || V7I.dot;
              return (
                <div key={d.id} className={`v7-sr-item role-${role}`} style={roleVars(d.param, d.fieldKind)}
                  onClick={() => onAdd(d)} draggable onDragStart={(e) => e.dataTransfer.setData('text/v7block', d.id)}>
                  <span className="v7-sr-cap"><Icon /></span>
                  <span className="v7-sr-meta">
                    <div className="v7-sr-nm">{v7Display(d.param, locale)}{v7IsLive(d.param) && <span style={{ color: 'var(--cyan-700)', fontSize: 9, marginLeft: 6, fontFamily: 'var(--ff-mono)' }}>LIVE</span>}</div>
                    <div className="v7-sr-canon">{d.param}</div>
                  </span>
                </div>
              );
            })}
          </div>
        ) : (
          cats.map((c, i) => <V7Category key={c.path} node={c} blocks={blocks} locale={locale} onAdd={onAdd} defaultOpen={c.seg === 'amm'} />)
        )}
      </div>
      <div className="v7-pal-legend">
        {Object.values(V7_ROLES).map(r => (
          <span key={r.key} className={`v7-leg role-${r.key}`} style={{ '--r-fill': `var(--role-${r.key}-fill)`, '--r-bd': `var(--role-${r.key}-bd)` }}>
            <span className="sw" />{locale === 'ko' ? r.ko : r.en}
          </span>
        ))}
      </div>
    </aside>
  );
}

Object.assign(window, { V7Palette, V7Chip });
