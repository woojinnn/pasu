// editor-block.jsx
// Block mode — visual policy canvas + signal palette (left 18%).
// Per §9.4: color = domain (Sage/Slate/Cyan only), shape = leaf type,
// dashed = custom (manifest enrichment).

const { useState: useStateB, useMemo: useMemoB } = React;

// ─── Block shape primitive (SVG-backed so dashed strokes work cleanly) ──────
function BlockShape({ shape, color, dashed, children, skewed, dragOver }) {
  // color: 'sage' | 'slate' | 'cyan'
  const fill = `var(--${color}-100)`;
  const stroke = skewed ? 'var(--fail-500)' : `var(--${color}-${color === 'slate' ? '500' : '500'})`;
  const stroke2 = dragOver ? 'var(--sage-600)' : stroke;

  // For rect & pill: use CSS (border + border-radius) — geometry is in real px so
  // content padding always clears the curve. SVG strokes get stretched, which
  // pushes the slope wider than the content padding and makes the leftmost SegChip
  // poke out. We only need SVG for hexagon (which CSS can't draw with a stroke).
  if (shape === 'pill' || shape === 'rect') {
    return (
      <div className={`bk bk-${shape} ${dashed ? 'bk-dashed' : ''} ${skewed ? 'bk-skewed' : ''}`}
           style={{
             background: fill,
             border: `1.6px ${dashed ? 'dashed' : 'solid'} ${stroke2}`,
             borderRadius: shape === 'pill' ? 999 : 6,
           }}>
        <div className="bk-body">{children}</div>
        {skewed && (
          <div className="bk-skew-flag" title="신호 정의가 바뀌었습니다">
            <I.warn style={{ width: 11, height: 11 }} />
            skew
          </div>
        )}
      </div>
    );
  }

  // Hexagon: SVG with a very wide viewBox so the slope (which stretches with
  // preserveAspectRatio="none") becomes ~1% of the rendered width — narrow enough
  // that the bk-body's left/right padding always clears it.
  const hexPath = 'M20 1 H1980 L1999 20 L1980 39 H20 L1 20 Z';
  return (
    <div className={`bk bk-hexagon ${dashed ? 'bk-dashed' : ''} ${skewed ? 'bk-skewed' : ''}`}>
      <svg className="bk-bg" viewBox="0 0 2000 40" preserveAspectRatio="none">
        <path d={hexPath} fill={fill} stroke={stroke2} strokeWidth="1.6"
              strokeDasharray={dashed ? '5 4' : null} vectorEffect="non-scaling-stroke" />
      </svg>
      <div className="bk-body">{children}</div>
      {skewed && (
        <div className="bk-skew-flag" title="신호 정의가 바뀌었습니다">
          <I.warn style={{ width: 11, height: 11 }} />
          skew
        </div>
      )}
    </div>
  );
}

// ─── Operator chip and value chip (rendered inside a block) ────────────────
function OpChip({ op }) {
  return <span className="op-chip">{op}</span>;
}
function ValChip({ value, leafType }) {
  // value: { kind, text, unit }
  const cls =
    value.kind === 'ref'  ? 'val-ref' :
    value.kind === 'enum' ? 'val-enum' :
    value.kind === 'bool' ? 'val-bool' :
    value.kind === 'num'  ? 'val-num' : 'val-str';
  return (
    <span className={`val-chip ${cls}`}>
      <span className="val-t">{value.text}</span>
      {value.unit && <span className="val-u">{value.unit}</span>}
    </span>
  );
}
function SegChip({ label, isCustom, isTop }) {
  return (
    <span className={`seg-chip ${isCustom ? 'seg-custom' : ''} ${isTop ? 'seg-top' : ''}`}>
      <span className="seg-t">{label}</span>
      <I.caretDown className="seg-c" style={{ width: 10, height: 10 }} />
    </span>
  );
}

// ─── Single guard block (a leaf condition in the IR tree) ───────────────────
function GuardBlock({ guard, colorScheme, locale, focused, onFocus, matched, isTopSkew }) {
  // Determine color from top segment domain.
  const topSegKey = guard.segments[0].key;
  const color = colorScheme.map[topSegKey] || 'cyan';

  // Shape from value kind / leaf type.
  let shape = 'rect';
  if (guard.value.kind === 'bool' || guard.value.kind === 'enum') shape = 'hexagon';
  else if (guard.value.kind === 'num') shape = 'pill';

  return (
    <div
      className={`guard ${focused ? 'guard-focus' : ''} ${matched ? 'guard-match' : ''}`}
      onClick={onFocus}
    >
      <div className="guard-grip" aria-label="drag" title="드래그">
        <span className="grip-dot" /><span className="grip-dot" /><span className="grip-dot" />
        <span className="grip-dot" /><span className="grip-dot" /><span className="grip-dot" />
      </div>

      <BlockShape shape={shape} color={color} dashed={guard.custom} skewed={isTopSkew}>
        <div className="guard-row">
          {guard.segments.map((s, i) => (
            <SegChip key={s.key} label={s.label} isCustom={guard.custom && i === guard.segments.length - 1} isTop={i === 0} />
          ))}
          <OpChip op={guard.operator} />
          <ValChip value={guard.value} leafType="" />
          <span className="guard-meta-inner">
            {guard.custom && (
              <span className="abs-pill" title="값 부재 시 처리 (custom 신호)">
                <span className="abs-k">absence</span>
                <span className="abs-v">{guard.absence}</span>
              </span>
            )}
            {guard.note && <span className="note-pill">{guard.note}</span>}
          </span>
        </div>
      </BlockShape>

      <button className="guard-x" title="삭제"><I.x style={{ width: 12, height: 12 }} /></button>
    </div>
  );
}

// ─── OR / AND container ─────────────────────────────────────────────────────
function ContainerBlock({ container, children, orStyle }) {
  const isOR = container.op === 'OR';
  const cls = isOR
    ? (orStyle === 'tinted' ? 'ctn-or-tinted' : 'ctn-or-dashed')
    : 'ctn-and';
  return (
    <div className={`ctn ${cls}`}>
      <div className="ctn-head">
        <span className={`ctn-badge ${isOR ? 'ctn-badge-or' : 'ctn-badge-and'}`}>
          {isOR ? 'OR · 하나라도 참' : 'AND · 모두 참'}
        </span>
        <span className="ctn-meta">{React.Children.count(children)}개 조건</span>
        <div style={{ flex: 1 }} />
        <button className="ctn-add">
          <I.plus style={{ width: 12, height: 12 }} />
          조건 추가
        </button>
        <button className="ctn-dots" title="컨테이너 옵션">
          <I.dots style={{ width: 14, height: 14 }} />
        </button>
      </div>
      <div className="ctn-body">
        {children}
      </div>
    </div>
  );
}

// ─── Palette (left 18%) ─────────────────────────────────────────────────────
function PaletteSection({ title, count, items, colorScheme, query, expanded, onToggle, locale }) {
  const open = expanded;
  return (
    <div className="pal-sect">
      <button className="pal-sect-h" onClick={onToggle}>
        <I.caretDown style={{ width: 12, height: 12, transform: open ? 'rotate(0)' : 'rotate(-90deg)', transition: 'transform 120ms' }} />
        <span className="pal-sect-t">{title}</span>
        <span className="pal-sect-c">{count}</span>
      </button>
      {open && (
        <div className="pal-items">
          {items.map(it => {
            const cKey = colorScheme.map[it.id] || 'slate';
            const isCustom = it.custom !== false && (it.dashed || it.isCustom);
            return (
              <div
                key={it.id}
                className={`pal-item pal-color-${cKey} ${it.isCustom ? 'pal-dashed' : ''} ${it.shape ? `pal-shape-${it.shape}` : ''}`}
                title={it.path || it.id}
                draggable={false}
              >
                <span className={`pal-swatch sw-${cKey} ${it.isCustom ? 'sw-dashed' : ''} ${it.shape ? `sw-${it.shape}` : ''}`} />
                <span className="pal-label">{typeof it.label === 'string' ? it.label : it.label[locale] || it.label.en}</span>
                {it.kind === 'group' && <span className="pal-cascade">▸</span>}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

function BlockPalette({ colorScheme, locale }) {
  const [query, setQuery] = useStateB('');
  const [openBase, setOpenBase] = useStateB(true);
  const [openCustom, setOpenCustom] = useStateB(true);

  // Augment with display attrs (shape for swatch)
  const baseItems = SIGNAL_CATALOG.base.map(s => ({
    ...s,
    isCustom: false,
    shape: s.kind === 'group' ? 'rect' : s.shape,
  }));
  const customItems = SIGNAL_CATALOG.custom.map(s => ({
    ...s,
    isCustom: true,
    shape: s.kind === 'group' ? 'rect' : s.shape,
  }));

  const filter = (items) => {
    if (!query) return items;
    const q = query.toLowerCase();
    return items.filter(it => {
      const lbl = typeof it.label === 'string' ? it.label : (it.label[locale] || it.label.en);
      return lbl.toLowerCase().includes(q) || it.id.toLowerCase().includes(q);
    });
  };

  return (
    <aside className="palette" aria-label="Block palette">
      <div className="pal-head">
        <span className="pal-title">Block palette</span>
        <span className="pal-hint">drag onto canvas</span>
      </div>
      <div className="pal-search">
        <I.search style={{ width: 13, height: 13, color: 'var(--slate-400)' }} />
        <input value={query} onChange={e => setQuery(e.target.value)}
               placeholder="라벨 · JSON path · 단위" />
        <span className="kbd-mini">/</span>
      </div>

      <PaletteSection title="기본 필드 (calldata)" count={6}
        items={filter(baseItems)} colorScheme={colorScheme} query={query}
        expanded={openBase} onToggle={() => setOpenBase(!openBase)} locale={locale} />

      <PaletteSection title="커스텀 (manifest enrichment)" count={9}
        items={filter(customItems)} colorScheme={colorScheme} query={query}
        expanded={openCustom} onToggle={() => setOpenCustom(!openCustom)} locale={locale} />

      <div className="pal-foot">
        <div className="pal-legend">
          <div className="pal-leg-row">
            <span className="pal-leg-swatch pal-shape-pill" />
            <span className="pal-leg-t">pill · 숫자</span>
          </div>
          <div className="pal-leg-row">
            <span className="pal-leg-swatch pal-shape-rect" />
            <span className="pal-leg-t">rect · 문자열/주소</span>
          </div>
          <div className="pal-leg-row">
            <span className="pal-leg-swatch pal-shape-hexagon" />
            <span className="pal-leg-t">hex · boolean/enum</span>
          </div>
          <div className="pal-leg-row">
            <span className="pal-leg-swatch pal-shape-rect pal-leg-dashed" />
            <span className="pal-leg-t">dashed · custom</span>
          </div>
        </div>
      </div>
    </aside>
  );
}

// ─── Block-mode canvas ──────────────────────────────────────────────────────
function BlockCanvas({ policy, colorScheme, locale, orStyle, skewedGuard, matchedGuards, focusedGuard, onFocusGuard }) {
  return (
    <div className="bk-canvas" data-screen-label="Block mode canvas">
      <div className="bk-trigger">
        <span className="trg-k">On action</span>
        <span className="trg-eq">=</span>
        <span className="trg-v">swap</span>
        <span className="trg-then">→ then evaluate:</span>
      </div>

      <ContainerBlock container={policy.root} orStyle={orStyle}>
        {policy.root.children.map(g => (
          <GuardBlock
            key={g.id}
            guard={g}
            colorScheme={colorScheme}
            locale={locale}
            focused={focusedGuard === g.id}
            onFocus={() => onFocusGuard(g.id)}
            matched={(matchedGuards || []).includes(g.id)}
            isTopSkew={skewedGuard === g.id}
          />
        ))}
      </ContainerBlock>

      <div className="bk-decision">
        <span className="dec-arrow">↓</span>
        <span className="dec-then">Then</span>
        <span className="dec-deny">Deny</span>
        <span className="dec-reason">"swap baseline violated"</span>
        <span className="dec-sev">severity FAIL</span>
      </div>

      <div className="bk-canvas-foot">
        <button className="cn-mini">+ AND 컨테이너</button>
        <button className="cn-mini">+ OR 컨테이너</button>
        <span style={{ flex: 1 }} />
        <span className="cn-mini-hint">space + drag = pan · 휠 = 줌</span>
      </div>
    </div>
  );
}

Object.assign(window, { BlockPalette, BlockCanvas, BlockShape, GuardBlock });
