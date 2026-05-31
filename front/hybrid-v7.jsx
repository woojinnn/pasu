// hybrid-v7.jsx — bidirectional hybrid: read view = H outline (정책 문서),
// edit view = E puzzle blocks. Global toggle + per-clause flip. Same doc model
// (v7BuildDoc); edits in E reflect live in H (round-trip).

const { useState: useSH, useMemo: useMH } = React;

const I = {
  read: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M4 5h16M4 12h16M4 19h10"/></svg>,
  edit: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><rect x="3" y="4" width="8" height="6" rx="2"/><rect x="13" y="14" width="8" height="6" rx="2"/><path d="M7 10v4h6"/></svg>,
  flipE: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><rect x="3" y="4" width="8" height="6" rx="2"/><rect x="13" y="14" width="8" height="6" rx="2"/><path d="M7 10v4h6"/></svg>,
  flipH: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><path d="M4 5h16M4 12h16M4 19h10"/></svg>,
  key: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" {...p}><circle cx="8" cy="8" r="5"/><path d="M11.5 11.5L21 21M17 17l2-2"/></svg>,
  sw: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" {...p}><rect x="2" y="7" width="20" height="10" rx="5"/><circle cx="8" cy="12" r="3" fill="currentColor" stroke="none"/></svg>,
  hash: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" {...p}><path d="M4 9h16M4 15h16M10 3L8 21M16 3l-2 18"/></svg>,
  tok: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" {...p}><circle cx="12" cy="12" r="8"/><path d="M9.5 12l1.8 1.8 3.5-3.6" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  clock: (p) => <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" {...p}><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></svg>,
  dot: (p) => <svg viewBox="0 0 24 24" fill="currentColor" {...p}><circle cx="12" cy="12" r="4"/></svg>,
};
const ROLE_ICON = { numeric: I.hash, address: I.key, ref: I.tok, enum: I.sw, auth: I.clock, misc: I.dot };
function rstyle(param, fk) { const r = v7RoleOf(param, fk); return { '--r-fill': `var(--role-${r}-fill)`, '--r-bd': `var(--role-${r}-bd)`, '--r-ink': `var(--role-${r}-ink)` }; }
function opWord(op, locale) {
  if (locale === 'ko') { const m = { eq: '와 같고', neq: '와 다르고', lt: '미만이고', lte: '이하이고', gt: '초과이고', gte: '이상이고', isTrue: '이며 (참)', isFalse: '이며 (거짓)' }; return m[op] || (V7_OPSYM[op] || op); }
  return V7_OPSYM[op] || op;
}
function valText(v) { if (!v) return ''; return v.kind === 'ref' ? v.text : (v.text != null ? v.text : ''); }

// ── flatten a guard's predicates for H read view (NOT/AND → bullet list) ──
function collectPreds(doc, id, acc) {
  const n = doc.nodes[id]; if (!n) return acc;
  if (n.type === 'predicate') acc.push(n);
  else (n.childIds || []).forEach(c => collectPreds(doc, c, acc));
  return acc;
}

// ── Korean NLG (§9.1) — per-operator templates + 조사(받침) auto-select + value substitution ──
function jong(w) { if (!w) return false; w = w.replace(/\([^)]*\)\s*$/, '').trimEnd(); if (!w) return false; const ch = w.charCodeAt(w.length - 1); if (ch >= 0xAC00 && ch <= 0xD7A3) return (ch - 0xAC00) % 28 !== 0; return /[1-9lmnr03]$/i.test(w); }
function J(w, a, b) { return w + (jong(w) ? a : b); }
function valDisp(node, locale) {
  const v = node.value || {};
  if (v.kind === 'ref' || (typeof v.text === 'string' && v.text[0] === '@')) {
    const ko = { '@meta.from': '보낸 지갑', '@meta.to': '대상 컨트랙트' };
    const en = { '@meta.from': 'the sender', '@meta.to': 'the target' };
    return (locale === 'en' ? en : ko)[v.text] || v.text.replace(/^@/, '');
  }
  return v.text != null ? String(v.text) : '';
}
function nameOf(param, locale) { return v7Display(param, locale).replace(/\s*\([^)]*\)\s*$/, ''); }
// mode: 'final' (종결) | 'conn' (연결 -고) | 'adn' (관형 -ㄴ)
function phrase(node, mode, locale) {
  const F = nameOf(node.param, locale);
  const op = node.op, v = node.value || {}, unit = v7Unit(node.param, locale) || (v.unit || '');
  if (locale === 'en') {
    const sym = { lt: '<', lte: '≤', gt: '>', gte: '≥', eq: '=', neq: '≠', in: '∈', isTrue: 'is true', isFalse: 'is false', startsWith: 'starts with' }[op] || op;
    if (op === 'isTrue') return F;
    if (op === 'isFalse') return `not ${F}`;
    return `${F} ${sym} ${valDisp(node, 'en')}${unit}`;
  }
  const mi = mode === 'conn' ? 1 : (mode === 'adn' ? 2 : 0);
  const NUM = { lt: ['미만', '미만이고', '미만인'], lte: ['이하', '이하이고', '이하인'], gt: ['초과', '초과이고', '초과인'], gte: ['이상', '이상이고', '이상인'] };
  if (NUM[op]) return `${J(F, '이', '가')} ${v.text}${unit} ${NUM[op][mi]}`;
  if (op === 'isTrue') return `${F}${['임', '이며', '인'][mi]}`;
  if (op === 'isFalse') return `${F}${[' 아님', ' 아니며', ' 아닌'][mi]}`;
  const vd = valDisp(node, 'ko');
  if (op === 'neq') return `${J(F, '이', '가')} ${J(vd, '과', '와')} ${['다름', '다르고', '다른'][mi]}`;
  if (op === 'eq') return `${J(F, '이', '가')} ${J(vd, '과', '와')} ${['같음', '같고', '같은'][mi]}`;
  if (op === 'in') return `${J(F, '이', '가')} ${vd} ${['중 하나', '중 하나이고', '중 하나인'][mi]}`;
  if (op === 'startsWith') return `${J(F, '이', '가')} ${J(vd, '으로', '로')} ${['시작', '시작하고', '시작하는'][mi]}`;
  return `${J(F, '이', '가')} ${vd}`;
}

// builds the precise NLG sentence for a guard (used in the collapsible ③ tier)
function clauseSentence(doc, node, locale) {
  const isNot = node.type === 'logic' && node.op === 'NOT';
  const preds = collectPreds(doc, node.id, []);
  if (!isNot) return preds.map((p, i) => phrase(p, i === preds.length - 1 ? 'final' : 'conn', locale)).join(' ');
  if (locale === 'en') return 'Exclude transactions where ' + preds.map(p => phrase(p, 'final', 'en')).join(' AND ');
  const parts = preds.map((p, i) => phrase(p, i === preds.length - 1 ? 'adn' : 'conn', locale));
  return parts.join(' ') + ' 거래는 제외';
}
// ── H read: one clause as a generated sentence (precise tier) ──
function ReadClause({ doc, node, locale }) {
  const preds = collectPreds(doc, node.id, []);
  return (
    <div className="H-sent-wrap">
      <div className="H-sentence">{clauseSentence(doc, node, locale)}</div>
      <div className="H-canons">
        {preds.map(p => (
          <span key={p.id} className="H-canon" style={rstyle(p.param, p.fieldKind)}>
            <span className="dot" />{p.param}{v7IsLive(p.param) && <span className="lv">live</span>}
          </span>
        ))}
      </div>
    </div>
  );
}

// ── E edit: blocks (draggable / reorderable / re-nestable) ──
function EditNode({ doc, node, locale, patch, dnd }) {
  if (!node) return null;
  if (node.type === 'predicate') {
    const live = v7IsLive(node.param);
    const Icon = ROLE_ICON[v7RoleOf(node.param, node.fieldKind)] || I.dot;
    const v = node.value || {};
    const noVal = node.op === 'isTrue' || node.op === 'isFalse';
    const dragging = dnd && dnd.dragId === node.id;
    return (
      <span className={`E-pred ${live ? 'live' : ''} ${dragging ? 'dragging' : ''}`} style={rstyle(node.param, node.fieldKind)}
        draggable={!!dnd}
        onDragStart={(e) => { if (dnd) { dnd.setDragId(node.id); e.dataTransfer.effectAllowed = 'move'; e.dataTransfer.setData('text/plain', node.id); } }}
        onDragEnd={() => dnd && dnd.clear()}>
        <span className="grip" title="드래그하여 이동"><svg viewBox="0 0 24 24" fill="currentColor"><circle cx="9" cy="6" r="1.6"/><circle cx="15" cy="6" r="1.6"/><circle cx="9" cy="12" r="1.6"/><circle cx="15" cy="12" r="1.6"/><circle cx="9" cy="18" r="1.6"/><circle cx="15" cy="18" r="1.6"/></svg></span>
        <span className="ic"><Icon /></span>
        <span className="nm">{v7Display(node.param, locale)}</span>
        <span className="op">{V7_OPSYM[node.op] || node.op}</span>
        {!noVal && (v.kind === 'ref'
          ? <span className="vl ref">{v.text}</span>
          : <><input value={v.text != null ? v.text : ''} onChange={(e) => patch(node.id, { value: { ...v, text: e.target.value } })} onMouseDown={(e) => e.stopPropagation()} />{v.unit && <span className="u">{v.unit}</span>}</>
        )}
      </span>
    );
  }
  // logic
  const kids = (node.childIds || []).map(c => doc.nodes[c]).filter(Boolean);
  const Slot = (
    <div className="E-stack"
      onDragOver={(e) => { if (dnd && dnd.dragId) { e.preventDefault(); if (kids.length === 0) dnd.over(node.id, 0); } }}
      onDrop={(e) => { if (dnd) { e.preventDefault(); e.stopPropagation(); dnd.drop(node.id); } }}>
      {kids.map((k, i) => (
        <div key={k.id} className="E-slot"
          onDragOver={(e) => { if (dnd && dnd.dragId) { e.preventDefault(); const r = e.currentTarget.getBoundingClientRect(); dnd.over(node.id, (e.clientY - r.top) > r.height / 2 ? i + 1 : i); } }}>
          {dnd && dnd.line(node.id, i) && <div className="E-dropline" />}
          <EditNode doc={doc} node={k} locale={locale} patch={patch} dnd={dnd} />
        </div>
      ))}
      {dnd && dnd.line(node.id, kids.length) && <div className="E-dropline" />}
      {kids.length === 0 && <div className="E-empty">여기에 블록을 놓기</div>}
    </div>
  );
  if (node.op === 'NOT') {
    return (
      <div className="E-wrap">
        <div className="ehead"><span className="kw">NOT</span> 다음 패턴이면 제외</div>
        {kids.map(k => <EditNode key={k.id} doc={doc} node={k} locale={locale} patch={patch} dnd={dnd} />)}
      </div>
    );
  }
  return (
    <div className="E-and">
      <div className="ehead"><span className="kw">{node.op}</span><span className="hint">블록을 끌어 순서·위치 변경</span></div>
      {Slot}
    </div>
  );
}

function App() {
  const [doc, setDoc] = useSH(() => v7BuildDoc());
  const [mode, setMode] = useSH('read');           // global default
  const [openIds, setOpenIds] = useSH(() => new Set());  // clauses shown as E blocks
  const [locale, setLocale] = useSH('ko');

  const root = doc.nodes[doc.rootId];
  const guards = (root.childIds || []).map(id => doc.nodes[id]).filter(Boolean);

  const patch = (id, p) => setDoc(d => { const n = { ...d, nodes: { ...d.nodes } }; n.nodes[id] = { ...n.nodes[id], ...p }; return n; });

  // ── block drag-and-drop (move / reorder / re-nest) ──
  const [dragId, setDragId] = useSH(null);
  const [dropAt, setDropAt] = useSH(null);   // { parentId, index }
  const isAncestor = (d, anc, id) => { let n = d.nodes[id]; while (n && n.parentId) { if (n.parentId === anc) return true; n = d.nodes[n.parentId]; } return false; };
  const moveNode = (id, parentId, index) => setDoc(d => {
    if (!id || !d.nodes[id] || !d.nodes[parentId]) return d;
    if (id === parentId || isAncestor(d, id, parentId)) return d;   // no drop into self/descendant
    const nodes = { ...d.nodes };
    const moving = { ...nodes[id] };
    const oldP = moving.parentId && nodes[moving.parentId];
    if (oldP) { const op = { ...oldP, childIds: oldP.childIds.filter(x => x !== id) }; nodes[op.id] = op; }
    const np = { ...nodes[parentId] };
    const kids = (np.childIds || []).filter(x => x !== id);
    let idx = index; if (idx == null || idx > kids.length) idx = kids.length;
    kids.splice(idx, 0, id);
    np.childIds = kids; nodes[parentId] = np;
    moving.parentId = parentId; nodes[id] = moving;
    return { ...d, nodes };
  });
  const dnd = {
    dragId, setDragId,
    over: (parentId, index) => setDropAt({ parentId, index }),
    clear: () => { setDragId(null); setDropAt(null); },
    line: (parentId, index) => dragId && dropAt && dropAt.parentId === parentId && dropAt.index === index,
    drop: (parentId) => { if (dragId && dropAt && dropAt.parentId === parentId) moveNode(dragId, parentId, dropAt.index); setDragId(null); setDropAt(null); },
  };

  const setGlobal = (m) => {
    setMode(m);
    setOpenIds(m === 'edit' ? new Set(guards.map(g => g.id)) : new Set());
  };
  const flip = (id) => setOpenIds(s => { const n = new Set(s); n.has(id) ? n.delete(id) : n.add(id); return n; });

  const actLabel = (doc.action || 'Amm::Swap').split('::').pop();

  return (
    <div className="hy-root">
      <div className="hy-top">
        <span className="hy-title">Scopeball <span className="sub">· 하이브리드 — 읽기 H ⇄ 편집 E</span></span>
        <span className="hy-spc" />
        <div className="hy-modes">
          <button className={`hy-mode ${mode === 'read' ? 'on' : ''}`} onClick={() => setGlobal('read')}><I.read /> 읽기 · 문서</button>
          <button className={`hy-mode ${mode === 'edit' ? 'on' : ''}`} onClick={() => setGlobal('edit')}><I.edit /> 편집 · 블록</button>
        </div>
        <div className="hy-loc"><button className={locale === 'en' ? 'on' : ''} onClick={() => setLocale('en')}>EN</button><button className={locale === 'ko' ? 'on' : ''} onClick={() => setLocale('ko')}>KO</button></div>
      </div>

      <div className="hy-body">
        <div className="hy-doc">
          <div className="hy-hint"><span>전역 토글로 전체 전환 · 각 조건의 <kbd>블록으로 열기 ⇄ 문장으로 접기</kbd>로 절 단위 양방향 전환 · E에서 값 편집 → 읽기뷰에 즉시 반영</span></div>
          <div className="hy-sheet">
            <div className="hy-intro">{doc.readingHeader || '이 정책을 허용하려면 — 아래를 모두 만족해야 합니다'}</div>

            {guards.map(g => {
              const isNot = g.type === 'logic' && g.op === 'NOT';
              const open = openIds.has(g.id);
              const uc = locale === 'ko' ? g.userCopy : null;
              const headline = (uc && uc.headline) || clauseSentence(doc, g, locale);
              const plain = uc && uc.plain;
              return (
                <div key={g.id} className={`hy-clause ${open ? 'editing' : ''}`}>
                  {open ? (
                    <>
                      <div className="hy-clause-bar">
                        <span className="hy-cid">{g.guardId}</span>
                        <span className={`hy-ckw ${isNot ? '' : 'cond'}`}>{isNot ? 'NOT' : '조건'}</span>
                        <span className="hy-clab">{g.label || v7Display(g.param, locale)}</span>
                        <button className="hy-flip on" onClick={() => flip(g.id)}><I.flipH /> 문장으로 접기</button>
                      </div>
                      <div style={{ paddingLeft: 4 }}><EditNode doc={doc} node={g} locale={locale} patch={patch} dnd={dnd} /></div>
                    </>
                  ) : (
                    <div className="hy-read">
                      <div className="hy-read-top">
                        <span className={`hy-read-dot ${isNot ? 'block' : 'allow'}`} />
                        <div className="hy-read-headline">{headline}</div>
                        <button className="hy-flip" onClick={() => flip(g.id)}><I.flipE /> 블록으로 열기</button>
                      </div>
                      {plain && <div className="hy-read-plain">{plain}</div>}
                      <details className="hy-precise">
                        <summary>정밀 조건 · canonical</summary>
                        <ReadClause doc={doc} node={g} locale={locale} />
                      </details>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}
ReactDOM.createRoot(document.getElementById('root')).render(<App />);
