// editor-v6-state.js
// v6 — Block palette as a .cedarschema directory tree (single `actions/` root).
//
// Builds on editor-v5-state.js (loaded first): reuses v5Reduce for node graph
// ops and the Cedar serializer / evaluator. v6 adds:
//   • V6_TREE          — nested system directory rooted at actions/ (read-only)
//   • V6_BLOCKS        — flat path → block-definition lookup
//   • node.style       — { tone, fillHex, borderHex, shape, tag } block style
//   • customTree       — user-editable enrichment files/folders (in state)
//   • new actions: ADD_SCHEMA_BLOCK, SET_BLOCK_STYLE, ADD_CUSTOM_FILE,
//                  ADD_CUSTOM_FOLDER, TOGGLE_CUSTOM_FOLDER

// ─── system directory (read-only) ────────────────────────────────────────────
// V6_TREE (actions/ → category → action → field blocks, subtypes nested) and
// V6_BLOCKS (path → real field def w/ param·fieldKind·op·value) are GENERATED in
// editor-v6-schema.js (loaded before this file) from cedar_blocks.filtered_flat.json.

// walk every file node
function v6WalkFiles(node, cb) {
  if (node.type === 'file') { cb(node); return; }
  (node.children || []).forEach(c => v6WalkFiles(c, cb));
}
// find a folder node by path
function v6FindFolder(path, node = V6_TREE) {
  if (node.type !== 'folder') return null;
  if (node.path === path) return node;
  for (const c of node.children || []) {
    if (c.type === 'folder') { const r = v6FindFolder(path, c); if (r) return r; }
  }
  return null;
}
// count files under a folder (recursive)
function v6CountFiles(node) {
  let n = 0; v6WalkFiles(node, () => n++); return n;
}

// flat lookup V6_BLOCKS (path → field def) is generated in editor-v6-schema.js.

// ─── block-style helpers ─────────────────────────────────────────────────────
// Only the three policy-meaning signal tones are named presets. Everything
// else is a custom hex pair (monotone by default).
const V6_TONES = {
  fail: { fill: 'var(--fail-100)', border: 'var(--fail-500)', text: 'var(--fail-800)', label: '차단' },
  warn: { fill: 'var(--warn-50)',  border: 'var(--warn-500)', text: 'var(--warn-800)', label: '경고' },
  pass: { fill: 'var(--pass-100)', border: 'var(--pass-500)', text: 'var(--pass-800)', label: '정상' },
};
const V6_SHAPES = ['rect', 'rounded', 'diamond', 'hex'];
const V6_SHAPE_LABEL = { rect: '사각형', rounded: '둥근 사각형', diamond: '마름모', hex: '육각형' };

// ─── README §9.4: block FILL = domain identity (base palette only) ───────────
// Domain (Sage/Slate/Cyan) decides the block's fill+border. Severity NEVER
// touches the fill — it rides on the 4px status edge + corner pill + banner.
const V6_DOMAIN = {
  sage:  { fill: 'var(--sage-100)',  border: 'var(--sage-600)', text: 'var(--slate-900)', label: '입력',        en: 'Input' },
  slate: { fill: 'var(--slate-100)', border: 'var(--slate-400)', text: 'var(--slate-900)', label: '출력·검증',   en: 'Output·validity' },
  cyan:  { fill: 'var(--cyan-100)',  border: 'var(--cyan-600)', text: 'var(--slate-900)', label: '수신자·메타', en: 'Recipient·meta' },
};
// status severity — small signals only (edge + pill + banner). README §3: status
// 색은 면적 점유 금지. The 4px edge matches §2's allowed "좌측 4px status border" channel.
const V6_SEV = {
  fail: { edge: 'var(--fail-500)', pillBg: 'var(--fail-500)', pillTx: 'var(--fog-50)', label: '차단' },
  warn: { edge: 'var(--warn-500)', pillBg: 'var(--warn-500)', pillTx: 'var(--fog-50)', label: '경고' },
  pass: { edge: 'var(--pass-500)', pillBg: 'var(--pass-500)', pillTx: 'var(--fog-50)', label: '정상' },
};
// param → domain (Sage=Input · Slate=Output·validity · Cyan=Recipient·meta)
const V6_PARAM_DOMAIN = {
  'context.recipient': 'cyan', 'meta.from': 'cyan', 'meta.to': 'cyan',
  'enrichment.recipientIsContract': 'cyan', 'enrichment.totalInputUsd': 'cyan',
  'context.slippageBp': 'slate', 'context.priceImpactBp': 'slate',
  'enrichment.validityDeltaSec': 'slate', 'enrichment.effectiveRateVsOracleBps': 'slate',
};
// the block's domain — from the signal's top segment (conditions) or neutral (logic)
function v6DomainOf(node) {
  if (!node) return 'slate';
  if (node.kind === 'group') return 'slate';   // logic node = neutral identity
  if (node.param) {
    if (V6_PARAM_DOMAIN[node.param]) return V6_PARAM_DOMAIN[node.param];
    if (/recipient|totalInput|amountIn|inputAmount/i.test(node.param)) return /recipient/i.test(node.param) ? 'cyan' : 'sage';
    return 'slate';
  }
  return node.sigId ? v5ColorFor(node.sigId) : 'slate';
}

// neutral monotone fallback (hex used only by legacy custom-color override UI)
const V6_NEUTRAL_FILL = '#EFF0F2';   // slate-50
const V6_NEUTRAL_BORDER = '#9099A5'; // slate-300
// default styles carry NO fill/border hex → domain drives the color
function v6DefaultStyle() { return { tone: null, shape: 'rounded', tag: null, fillHex: null, borderHex: null }; }
function v6GroupStyle() { return { tone: null, shape: 'hex', tag: null, fillHex: null, borderHex: null }; }
function v6ResolveStyle(style, domain) {
  const s = style || v6DefaultStyle();
  const dom = V6_DOMAIN[domain] || V6_DOMAIN.slate;
  const sev = (s.tone && V6_SEV[s.tone]) ? V6_SEV[s.tone] : null;
  return {
    fill: s.fillHex || dom.fill,
    border: s.borderHex || dom.border,
    text: dom.text,
    shape: s.shape || 'rounded',
    tag: s.tag || null,
    tone: s.tone || null,        // severity, kept as metadata only
    sev,                          // { edge, pillBg, pillTx, label } | null
    domain: domain || 'slate',
    domainLabel: dom.label,
  };
}

// ─── custom (enrichment) tree seed ───────────────────────────────────────────
function v6SeedCustomTree() {
  return [
    { id: 'cf_validityDeltaSec', kind: 'file', name: 'validityDeltaSec.cedarschema',
      leafType: 'seconds', tone: 'warn', shape: 'rounded', absence: 'false' },
    { id: 'cf_recipientIsContract', kind: 'file', name: 'recipientIsContract.cedarschema',
      leafType: 'boolean', tone: null, shape: 'hex', absence: 'false' },
    { id: 'cfolder_oracle', kind: 'folder', name: 'oracle', open: true, children: [
      { id: 'cf_effectiveRateVsOracleBps', kind: 'file', name: 'effectiveRateVsOracleBps.cedarschema',
        leafType: 'bps', tone: 'warn', shape: 'rounded', absence: 'false' },
      { id: 'cf_totalInputUsd', kind: 'file', name: 'totalInputUsd.cedarschema',
        leafType: 'usd', tone: null, shape: 'rounded', absence: 'false' },
    ] },
  ];
}

// ─── REAL schema catalog (Amm::Swap scope) — context.* / meta.* / enrichment.* ──
// fieldKind decides operators + value widget. enrichment.* = host-populated (dashed).
const V6_FIELD = {
  'context.recipient':                   { label: 'Recipient',             fieldKind: 'primitive.String',  src: 'context',    op: 'neq', value: { kind: 'ref', text: '@meta.from' }, shape: 'rect' },
  'context.slippageBp':                  { label: 'Slippage (bp)',         fieldKind: 'primitive.Long',    src: 'context',    op: 'gte', value: { kind: 'num', text: '100', unit: 'bp' }, shape: 'rounded' },
  'context.priceImpactBp':               { label: 'Price impact (bp)',     fieldKind: 'primitive.Long',    src: 'context',    op: 'gt',  value: { kind: 'num', text: '50', unit: 'bp' }, shape: 'rounded' },
  'meta.from':                           { label: 'Caller (from)',         fieldKind: 'primitive.String',  src: 'meta',       op: 'neq', value: { kind: 'str', text: '0x…' }, shape: 'rect' },
  'meta.to':                             { label: 'Target (to)',           fieldKind: 'primitive.String',  src: 'meta',       op: 'eq',  value: { kind: 'str', text: '0x…' }, shape: 'rect' },
  'enrichment.recipientIsContract':      { label: 'Recipient is contract', fieldKind: 'primitive.Bool',    src: 'enrichment', optional: false, op: 'isTrue', value: { kind: 'bool', text: 'true' }, shape: 'hex' },
  'enrichment.validityDeltaSec':         { label: 'Time to deadline (sec)',fieldKind: 'primitive.Long',    src: 'enrichment', optional: true,  op: 'lt',  value: { kind: 'num', text: '30', unit: 'sec' }, shape: 'rounded' },
  'enrichment.effectiveRateVsOracleBps': { label: 'Slippage vs oracle (bp)',fieldKind: 'primitive.Long',   src: 'enrichment', optional: true,  op: 'gt',  value: { kind: 'num', text: '100', unit: 'bp' }, shape: 'rounded' },
  'enrichment.totalInputUsd':            { label: 'Input value (USD)',     fieldKind: 'primitive.decimal', src: 'enrichment', optional: true,  op: 'gt',  value: { kind: 'num', text: '10000', unit: 'USD' }, shape: 'rounded' },
};
// operators allowed per fieldKind (fixtures evaluationContract.operatorsByFieldKind)
const V6_OPS = {
  'primitive.String':  ['eq', 'neq', 'in', 'notIn', 'startsWith', 'contains'],
  'primitive.Long':    ['eq', 'neq', 'lt', 'lte', 'gt', 'gte'],
  'primitive.decimal': ['eq', 'neq', 'lt', 'lte', 'gt', 'gte'],
  'primitive.Bool':    ['isTrue', 'isFalse'],
  'ref':               ['eq', 'neq', 'in', 'notIn'],
  'collection':        ['contains', 'containsAny', 'containsAll', 'isEmpty', 'sizeEq', 'sizeGt', 'sizeLt'],
  'record':            [],
};
const V6_OP_SYM = {
  eq: '==', neq: '≠', lt: '<', lte: '≤', gt: '>', gte: '≥',
  in: '∈ allow', notIn: '∉ deny', startsWith: 'starts', contains: 'contains',
  isTrue: '= true', isFalse: '= false',
  isEmpty: 'isEmpty', containsAny: '⊇ any', containsAll: '⊇ all', sizeEq: '#==', sizeGt: '#>', sizeLt: '#<',
};
function v6FieldMeta(param) { return V6_FIELD[param] || { label: param, fieldKind: 'primitive.Long', src: param.split('.')[0], op: 'gt', value: { kind: 'num', text: '0' }, shape: 'rounded' }; }
function v6IsCustomParam(param) { return /^enrichment\./.test(param || ''); }

// ─── golden fixtures (swap_policy_test_fixtures.json · ALL OK) ────────────────
const V6_FIXTURES = [
  { id: 'tx-calm', label: 'Calm swap · USDC→WETH',
    tx: {
      meta: { from: '0xA1c4000000000000000000000000000000007e29', to: '0xE592427A0AEce92De3Edee1F18E0157C05861564', selector: '0x414bf389', chainId: 1, value: '0x0', nonce: 42, blockTimestamp: 1748649600, isSimulated: true },
      enrichment: { validityDeltaSec: 300, recipientIsContract: false, effectiveRateVsOracleBps: 8, totalInputUsd: 4800.0 },
      context: { venue: 'uniswap_v3', tokenIn: 'USDC', tokenOut: 'WETH', recipient: '0xA1c4000000000000000000000000000000007e29', slippageBp: 50, priceImpactBp: 12 },
    },
    expected: { verdict: 'ALLOW', guards: { g1: false, g2: false, g3: false } } },
  { id: 'tx-market-expiry', label: 'Market swap · 만료 임박',
    tx: {
      meta: { from: '0xA1c4000000000000000000000000000000007e29', to: '0xE592427A0AEce92De3Edee1F18E0157C05861564', selector: '0x414bf389', chainId: 1, value: '0x0', nonce: 43, blockTimestamp: 1748649600, isSimulated: true },
      enrichment: { validityDeltaSec: 18, recipientIsContract: false, effectiveRateVsOracleBps: 140, totalInputUsd: 4800.0 },
      context: { venue: 'uniswap_v3', tokenIn: 'USDC', tokenOut: 'WETH', recipient: '0xA1c4000000000000000000000000000000007e29', slippageBp: 150, priceImpactBp: 60 },
    },
    expected: { verdict: 'DENY', matchedGuard: 'g3', guards: { g1: false, g2: true, g3: true } } },
  { id: 'tx-send-to-contract', label: 'Send to contract',
    tx: {
      meta: { from: '0xA1c4000000000000000000000000000000007e29', to: '0xE592427A0AEce92De3Edee1F18E0157C05861564', selector: '0x414bf389', chainId: 1, value: '0x0', nonce: 44, blockTimestamp: 1748649600, isSimulated: true },
      enrichment: { validityDeltaSec: 200, recipientIsContract: true, effectiveRateVsOracleBps: 5, totalInputUsd: 4800.0 },
      context: { venue: 'uniswap_v3', tokenIn: 'USDC', tokenOut: 'WETH', recipient: '0xBEEF000000000000000000000000000000001234', slippageBp: 40, priceImpactBp: 10 },
    },
    expected: { verdict: 'DENY', matchedGuard: 'g1', guards: { g1: true, g2: false, g3: false } } },
];

// ─── evaluation engine ───────────────────────────────────────────────────────
function v6ReadPath(tx, path) {           // 'context.recipient' / 'meta.from' / 'enrichment.x'
  if (!path) return undefined;
  const clean = path.replace(/^@/, '');
  const [src, ...rest] = clean.split('.');
  let cur = tx[src];
  for (const k of rest) { if (cur == null) return undefined; cur = cur[k]; }
  return cur;
}
function v6ResolveValue(tx, value) {       // literal or @-ref
  if (!value) return undefined;
  if (value.kind === 'ref' || (typeof value.text === 'string' && value.text[0] === '@')) return v6ReadPath(tx, value.text);
  if (value.kind === 'num') return Number(value.text);
  if (value.kind === 'bool') return value.text === 'true';
  if (value.kind === 'list') return (value.items || []);
  return value.text;
}
function v6ApplyOp(op, lhs, rhs) {
  switch (op) {
    case 'eq':  return lhs === rhs;
    case 'neq': return lhs !== rhs;
    case 'lt':  return Number(lhs) <  Number(rhs);
    case 'lte': return Number(lhs) <= Number(rhs);
    case 'gt':  return Number(lhs) >  Number(rhs);
    case 'gte': return Number(lhs) >= Number(rhs);
    case 'isTrue':  return lhs === true;
    case 'isFalse': return lhs === false;
    case 'in':       return Array.isArray(rhs) && rhs.includes(lhs);
    case 'notIn':    return Array.isArray(rhs) && !rhs.includes(lhs);
    case 'startsWith': return typeof lhs === 'string' && lhs.startsWith(String(rhs));
    case 'contains':   return Array.isArray(lhs) ? lhs.includes(rhs) : (typeof lhs === 'string' && lhs.includes(String(rhs)));
    case 'containsAny': return Array.isArray(lhs) && Array.isArray(rhs) && rhs.some(x => lhs.includes(x));
    case 'containsAll': return Array.isArray(lhs) && Array.isArray(rhs) && rhs.every(x => lhs.includes(x));
    case 'isEmpty':  return Array.isArray(lhs) ? lhs.length === 0 : (lhs == null || lhs === '');
    case 'sizeEq':   return Array.isArray(lhs) && lhs.length === Number(rhs);
    case 'sizeGt':   return Array.isArray(lhs) && lhs.length >  Number(rhs);
    case 'sizeLt':   return Array.isArray(lhs) && lhs.length <  Number(rhs);
    default: return false;
  }
}
function v6EvalCond(node, tx) {
  const lhs = v6ReadPath(tx, node.param);
  const optional = v6IsCustomParam(node.param) && (v6FieldMeta(node.param).optional !== false);
  if ((lhs === undefined || lhs === null) && optional) {
    const a = node.absence || 'treatAsFalse';
    if (a === 'treatAsTrue') return true;
    if (a === 'skip') return true;          // skip = does not block its AND
    return false;                            // treatAsFalse (default)
  }
  const needsRhs = !(node.op === 'isTrue' || node.op === 'isFalse' || node.op === 'isEmpty');
  const rhs = needsRhs ? v6ResolveValue(tx, node.value) : undefined;
  return v6ApplyOp(node.op || 'eq', lhs, rhs);
}
function v6EvalNode(state, id, tx, matched) {
  const n = state.nodes[id]; if (!n) return false;
  if (n.kind === 'condition') { const ok = v6EvalCond(n, tx); if (ok) matched.push(n.id); return ok; }
  if (!n.childIds || n.childIds.length === 0) return n.combinator === 'AND';
  let res;
  if (n.combinator === 'NOT') res = !v6EvalNode(state, n.childIds[0], tx, matched);
  else if (n.combinator === 'OR') res = n.childIds.some(c => v6EvalNode(state, c, tx, matched));
  else res = n.childIds.every(c => v6EvalNode(state, c, tx, matched));
  if (res) matched.push(n.id);
  return res;
}
function v6ToneOf(node) { const t = node.style && node.style.tone; return t === 'fail' ? 'deny' : t === 'warn' ? 'warn' : 'normal'; }
// deny > warn > normal precedence over the wired top-level guards
function v6Evaluate(state, tx) {
  const matched = [];
  const wiredIds = state.wires.map(w => w.to).filter(id => state.nodes[id]);
  const guards = wiredIds.map(id => {
    const sub = [];
    const result = v6EvalNode(state, id, tx, sub);
    sub.forEach(x => { if (!matched.includes(x)) matched.push(x); });
    const n = state.nodes[id];
    return { id, label: (n.label || n.id), tone: v6ToneOf(n), result };
  });
  if (wiredIds.length === 0)
    return { matchedLeafIds: [], guards, decision: { kind: 'Allow', reason: '루트에 연결된 가드 없음', severity: 'PASS' }, verdictTone: 'normal', tripped: false };

  const denyTrue = guards.filter(g => g.tone === 'deny' && g.result);
  const warnTrue = guards.filter(g => g.tone === 'warn' && g.result);
  let decision, verdictTone, primary = null;
  if (denyTrue.length)      { primary = denyTrue[0]; decision = { kind: 'Deny',  reason: state.denyMessage || 'policy violated', severity: 'FAIL' }; verdictTone = 'deny'; }
  else if (warnTrue.length) { primary = warnTrue[0]; decision = { kind: 'Warn',  reason: '경고 가드 발동 — 검토 필요',         severity: 'WARN' }; verdictTone = 'warn'; }
  else                      {                        decision = { kind: 'Allow', reason: '일치하는 차단/경고 가드 없음',        severity: 'PASS' }; verdictTone = 'normal'; }
  return { matchedLeafIds: matched, guards, decision, verdictTone, tripped: !!primary, primaryGuardId: primary ? primary.id : null };
}

// ─── Cedar serializer (real paths) ───────────────────────────────────────────
function v6CondCedar(node) {
  const m = v6FieldMeta(node.param);
  const sym = { eq: '==', neq: '!=', lt: '<', lte: '<=', gt: '>', gte: '>=' }[node.op];
  if (node.op === 'isTrue')  return `${node.param} == true`;
  if (node.op === 'isFalse') return `${node.param} == false`;
  let rhs;
  const v = node.value || {};
  if (v.kind === 'ref' || (v.text && v.text[0] === '@')) rhs = String(v.text).replace(/^@/, '');
  else if (v.kind === 'num') rhs = v.text;
  else rhs = `"${v.text}"`;
  const expr = `${node.param} ${sym || node.op} ${rhs}`;
  if (v6IsCustomParam(node.param) && m.optional !== false) return `(${node.param.split('.')[0]} has ${node.param.split('.').slice(1).join('.')} && ${expr})`;
  return expr;
}
function v6SerializeNode(state, id) {
  const n = state.nodes[id]; if (!n) return [];
  if (n.kind === 'condition') return [{ text: v6CondCedar(n), guardId: n.id, custom: v6IsCustomParam(n.param) }];
  if (!n.childIds || n.childIds.length === 0) return [{ text: n.combinator === 'AND' ? 'true' : 'false', guardId: n.id }];
  if (n.combinator === 'NOT') return v6SerializeNode(state, n.childIds[0]).map(s => ({ text: '!(' + s.text + ')', guardId: s.guardId, custom: s.custom }));
  const joiner = n.combinator === 'AND' ? '&&' : '||';
  const segs = [];
  n.childIds.forEach((cid, i) => {
    const child = state.nodes[cid]; if (!child) return;
    const sub = v6SerializeNode(state, cid);
    const wrap = child.kind === 'group' && child.childIds.length > 1;
    sub.forEach((s, j) => {
      let prefix = (i === 0 && j === 0) ? '' : (j === 0 ? joiner + ' ' : '   ');
      let suffix = '';
      if (wrap && sub.length === 1) { prefix += '('; suffix = ')'; }
      else if (wrap && j === 0) prefix += '(';
      else if (wrap && j === sub.length - 1) suffix = ')';
      segs.push({ text: prefix + s.text + suffix, guardId: s.guardId, custom: s.custom });
    });
  });
  return segs;
}
function v6ToCedar(state) {
  const lines = []; let n = 0;
  const push = (text, meta = {}) => { n++; lines.push({ n, text, ...meta }); };
  push('// ' + (state.policyName || 'Swap baseline') + ' · action Amm::Swap', { kind: 'cmt' });
  push('forbid (', { kind: 'kw' });
  push('  principal,', { kind: 'arg' });
  push('  action == Action::"Amm::Swap",', { kind: 'arg' });
  push('  resource', { kind: 'arg' });
  push(')', { kind: 'punct' });
  push('when {', { kind: 'kw' });
  const root = state.nodes[state.rootId];
  const wiredIds = state.wires.map(w => w.to).filter(id => state.nodes[id]);
  if (wiredIds.length === 0) push('  // (no guards wired to root — policy inactive)', { kind: 'cmt' });
  else {
    const joiner = root.combinator === 'AND' ? '&&' : '||';
    wiredIds.forEach((id, i) => {
      const sub = v6SerializeNode(state, id);
      const child = state.nodes[id];
      const wrap = child && child.kind === 'group' && child.childIds.length > 1;
      sub.forEach((s, j) => {
        let prefix = '', suffix = '';
        if (i > 0 && j === 0) prefix = joiner + ' '; else if (j > 0) prefix = '   ';
        if (wrap && sub.length === 1) { prefix += '('; suffix = ')'; }
        else if (wrap && j === 0) prefix += '(';
        else if (wrap && j === sub.length - 1) suffix = ')';
        push('  ' + prefix + s.text + suffix, { kind: 'guard', guardId: s.guardId, custom: s.custom });
      });
    });
  }
  push('};', { kind: 'kw' });
  push(`// → forbid "${state.denyMessage || 'swap baseline violated'}"`, { kind: 'cmt' });
  const drafts = v5DraftIds(state).length;
  if (drafts) push(`// ⚠ 미연결 노드 ${drafts}개 — 평가 제외`, { kind: 'cmt' });
  return { lines };
}

// ─── baseline = golden samplePolicy (g1/g2/g3 + 2 unconnected) ────────────────
function v6BuildBaseline() {
  const nodes = {};
  const wires = [];
  const put = (nd) => { nodes[nd.id] = nd; return nd; };
  const cond = (id, param, op, value, note, parentId, shapeOverride) => {
    const m = v6FieldMeta(param);
    return put({
      id, kind: 'condition', param, fieldKind: m.fieldKind, label: m.label,
      op, value: value === undefined ? (m.value ? JSON.parse(JSON.stringify(m.value)) : { kind: 'num', text: '0' }) : value,
      custom: v6IsCustomParam(param), absence: v6IsCustomParam(param) && m.optional !== false ? 'treatAsFalse' : null,
      note, parentId: parentId || null, x: 0, y: 0,
      style: { tone: null, shape: shapeOverride || m.shape || 'rounded', tag: null, fillHex: null, borderHex: null },
    });
  };

  put({ id: 'root1', kind: 'root', combinator: 'OR', x: 40, y: 180 });

  // g1 — swap-and-send 차단 (deny, AND of 2)
  const g1 = put({ id: 'g1', kind: 'group', combinator: 'AND', parentId: null, childIds: [], x: 360, y: 60, label: 'swap-and-send 차단',
    style: { tone: 'fail', shape: 'hex', tag: '차단', fillHex: null, borderHex: null } });
  const g1a = cond('g1a', 'context.recipient', 'neq', { kind: 'ref', text: '@meta.from' }, '수신자 ≠ 호출자', g1.id);
  const g1b = cond('g1b', 'enrichment.recipientIsContract', 'isTrue', { kind: 'bool', text: 'true' }, '컨트랙트 수신', g1.id);
  g1.childIds = [g1a.id, g1b.id];

  // g2 — 무방어 시장가 (warn, AND of 1)
  const g2 = put({ id: 'g2', kind: 'group', combinator: 'AND', parentId: null, childIds: [], x: 360, y: 235, label: '무방어 시장가',
    style: { tone: 'warn', shape: 'hex', tag: null, fillHex: null, borderHex: null } });
  const g2a = cond('g2a', 'context.slippageBp', 'gte', { kind: 'num', text: '100', unit: 'bp' }, '슬리피지 가드 과대', g2.id);
  g2.childIds = [g2a.id];

  // g3 — 만료 임박 (deny, AND of 2)
  const g3 = put({ id: 'g3', kind: 'group', combinator: 'AND', parentId: null, childIds: [], x: 360, y: 380, label: '만료 임박',
    style: { tone: 'fail', shape: 'hex', tag: '차단', fillHex: null, borderHex: null } });
  const g3a = cond('g3a', 'enrichment.validityDeltaSec', 'lt', { kind: 'num', text: '30', unit: 'sec' }, '마감 임박', g3.id);
  const g3b = cond('g3b', 'context.priceImpactBp', 'gt', { kind: 'num', text: '50', unit: 'bp' }, '가격 충격 큼', g3.id);
  g3.childIds = [g3a.id, g3b.id];

  wires.push({ id: v5Id('w'), from: 'root1', to: g1.id });
  wires.push({ id: v5Id('w'), from: 'root1', to: g2.id });
  wires.push({ id: v5Id('w'), from: 'root1', to: g3.id });

  // 2 unconnected example guards (evaluated out — drafts)
  cond('u1', 'enrichment.effectiveRateVsOracleBps', 'gt', { kind: 'num', text: '100', unit: 'bp' }, 'Oracle 대비 슬리피지', null);
  nodes['u1'].x = 360; nodes['u1'].y = 540;
  cond('u2', 'enrichment.totalInputUsd', 'gt', { kind: 'num', text: '10000', unit: 'USD' }, '대형 거래', null);
  nodes['u2'].x = 720; nodes['u2'].y = 540;

  return {
    nodes, wires, rootId: 'root1',
    policyName: 'Swap baseline', action: 'Amm::Swap', denyMessage: 'swap baseline violated',
    manifestHash: '#fc20a91',
    customTree: v6SeedCustomTree(),
    breadcrumb: ['actions', 'amm', 'swap.cedarschema'],
    pan: { x: 0, y: 0 }, zoom: 1,
  };
}

// ─── reducer (delegates to v5Reduce, adds v6 actions) ────────────────────────
function v6Reduce(state, a) {
  switch (a.type) {
    case 'ADD_SCHEMA_BLOCK': {
      const def = a.def;
      const id = v5Id('g');
      const rawName = (def.name || def.id || 'field').replace(/\.cedarschema$/, '');
      const param = def.param || ((def.custom ? 'enrichment.' : 'context.') + rawName);
      const known = !!V6_FIELD[param];
      const fieldKind = def.fieldKind || (known ? V6_FIELD[param].fieldKind : 'primitive.Long');
      const ops = V6_OPS[fieldKind] || ['eq'];
      const op = def.op || (known ? V6_FIELD[param].op : ops[0]);
      const value = def.value ? JSON.parse(JSON.stringify(def.value))
        : known ? JSON.parse(JSON.stringify(V6_FIELD[param].value))
        : (fieldKind === 'primitive.Bool' ? { kind: 'bool', text: 'true' } : { kind: 'num', text: '0' });
      const isCustom = def.custom !== undefined ? !!def.custom : v6IsCustomParam(param);
      const node = {
        id, kind: 'condition', param, fieldKind,
        label: rawName,
        op, value,
        custom: isCustom,
        absence: isCustom ? (/^(treatAsTrue|skip)$/.test(def.absence) ? def.absence : 'treatAsFalse') : null,
        note: null,
        parentId: a.parentId || null,
        x: a.x ?? 320, y: a.y ?? 240,
        style: { tone: def.tone || null, shape: def.shape || (known ? V6_FIELD[param].shape : 'rounded'), tag: null, fillHex: null, borderHex: null },
      };
      const nodes = { ...state.nodes, [id]: node };
      if (a.parentId && nodes[a.parentId]) {
        const p = nodes[a.parentId];
        nodes[a.parentId] = { ...p, childIds: [...p.childIds, id] };
      }
      return { ...state, nodes, lastChangeId: id };
    }
    case 'SET_BLOCK_STYLE': {
      const n = state.nodes[a.id]; if (!n) return state;
      const style = { ...(n.style || v6DefaultStyle()), ...a.patch };
      return { ...state, nodes: { ...state.nodes, [a.id]: { ...n, style } }, lastChangeId: a.id };
    }
    case 'ADD_CUSTOM_FILE': {
      const file = a.file;
      const node = { id: 'cf_' + (++__v6cid), kind: 'file', ...file };
      const tree = v6InsertCustom(state.customTree || [], node, a.parentId || null);
      return { ...state, customTree: tree, lastCustomId: node.id };
    }
    case 'ADD_CUSTOM_FOLDER': {
      const node = { id: 'cfolder_' + (++__v6cid), kind: 'folder', name: a.name, open: true, children: [] };
      const tree = v6InsertCustom(state.customTree || [], node, a.parentId || null);
      return { ...state, customTree: tree, lastCustomId: node.id };
    }
    case 'TOGGLE_CUSTOM_FOLDER': {
      const tree = v6MapCustom(state.customTree || [], (n) =>
        n.id === a.id && n.kind === 'folder' ? { ...n, open: !n.open } : n);
      return { ...state, customTree: tree };
    }
    case 'SET_BREADCRUMB': {
      return { ...state, breadcrumb: a.crumb };
    }
    case 'ADD_GROUP': {
      const next = v5Reduce(state, a);
      const id = next.lastChangeId;
      if (id && next.nodes[id]) {
        next.nodes = { ...next.nodes, [id]: { ...next.nodes[id], style: v6GroupStyle() } };
      }
      return next;
    }
    default:
      return v5Reduce(state, a);
  }
}

let __v6cid = 500;
function v6InsertCustom(tree, node, parentId) {
  if (!parentId) return [...tree, node];
  return tree.map(n => {
    if (n.id === parentId && n.kind === 'folder') return { ...n, open: true, children: [...(n.children || []), node] };
    if (n.kind === 'folder' && n.children) return { ...n, children: v6InsertCustom(n.children, node, parentId) };
    return n;
  });
}
function v6MapCustom(tree, fn) {
  return tree.map(n => {
    const mapped = fn(n);
    if (mapped.kind === 'folder' && mapped.children) return { ...mapped, children: v6MapCustom(mapped.children, fn) };
    return mapped;
  });
}
function v6FlattenCustomFiles(tree, acc = []) {
  for (const n of tree || []) {
    if (n.kind === 'file') acc.push(n);
    else if (n.kind === 'folder' && n.children) v6FlattenCustomFiles(n.children, acc);
  }
  return acc;
}

Object.assign(window, {
  V6_TREE, V6_BLOCKS, V6_TONES, V6_SHAPES, V6_SHAPE_LABEL,
  V6_DOMAIN, V6_SEV, v6DomainOf,
  V6_FIELD, V6_OPS, V6_OP_SYM, v6FieldMeta, v6IsCustomParam,
  V6_FIXTURES, v6Evaluate, v6ToCedar, v6ReadPath, v6ResolveValue,
  V6_NEUTRAL_FILL, V6_NEUTRAL_BORDER,
  v6DefaultStyle, v6GroupStyle, v6ResolveStyle, v6BuildBaseline, v6Reduce,
  v6FlattenCustomFiles, v6SeedCustomTree, v6WalkFiles, v6FindFolder, v6CountFiles,
});
