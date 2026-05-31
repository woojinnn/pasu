// editor-v5-state.js
// IR with explicit wires from the root output socket to top-level group/condition
// input sockets. A node is "included" (in policy) iff reachable from root via
// (wires ∪ group.childIds).
//
//   nodes:  Map<id, Node>
//   wires:  Array<{ id, from: rootId, to: nodeId }>
//   rootId: string
//
// Node shapes:
//   root      { id, kind:'root', combinator, x, y }
//   group     { id, kind:'group', combinator: 'AND'|'OR'|'NOT',
//               parentId, childIds, x, y }
//   condition { id, kind:'condition', sigId, label, leafType, operator, value,
//               custom, absence, note, parentId, x, y }

// ─── catalog ────────────────────────────────────────────────────────────────
const V5_SIGS = {
  base: [
    { id: 'recipient', label: { ko: 'Recipient', en: 'Recipient' },
      leafType: 'address', shape: 'rect', defaultOp: '!=', defaultVal: { kind: 'ref', text: 'root.from' } },
    { id: 'swapMode', label: { ko: 'Swap direction', en: 'Swap direction' },
      leafType: 'enum', shape: 'hex', options: ['market', 'limit'],
      defaultOp: '==', defaultVal: { kind: 'enum', text: 'market' } },
    { id: 'feeBps', label: { ko: 'Fee (bps)', en: 'Fee (bps)' },
      leafType: 'bps', shape: 'pill', defaultOp: '>', defaultVal: { kind: 'num', text: '50', unit: 'bps' } },
    { id: 'inputAmount', label: { ko: 'Input amount', en: 'Input amount' },
      leafType: 'tokenNative', shape: 'pill', defaultOp: '>', defaultVal: { kind: 'num', text: '1.0' } },
    { id: 'outputAmount', label: { ko: 'Output amount', en: 'Output amount' },
      leafType: 'tokenNative', shape: 'pill', defaultOp: '<', defaultVal: { kind: 'num', text: '0' } },
    { id: 'validityExpiresAt', label: { ko: 'Deadline', en: 'Deadline' },
      leafType: 'unix', shape: 'rect', defaultOp: '<', defaultVal: { kind: 'num', text: '0' } },
  ],
  custom: [
    { id: 'validityDeltaSec', label: { ko: '만료까지 (sec)', en: 'Time to deadline' },
      leafType: 'seconds', shape: 'pill', custom: true,
      defaultOp: '<', defaultVal: { kind: 'num', text: '30', unit: 'sec' } },
    { id: 'recipientIsContract', label: { ko: '수신자가 컨트랙트', en: 'Recipient is contract' },
      leafType: 'boolean', shape: 'hex', custom: true,
      defaultOp: '==', defaultVal: { kind: 'bool', text: 'true' } },
    { id: 'effectiveRateVsOracleBps', label: { ko: 'Oracle 대비 슬리피지', en: 'Slippage vs oracle' },
      leafType: 'bps', shape: 'pill', custom: true,
      defaultOp: '>', defaultVal: { kind: 'num', text: '100', unit: 'bps' } },
    { id: 'totalInputUsd', label: { ko: '입력 가치 (USD)', en: 'Input value (USD)' },
      leafType: 'usd', shape: 'pill', custom: true,
      defaultOp: '>', defaultVal: { kind: 'num', text: '10000', unit: 'USD' } },
    { id: 'totalInputFractionOfPortfolioBps', label: { ko: '입력 ÷ 포트폴리오', en: 'Input ÷ portfolio' },
      leafType: 'bps', shape: 'pill', custom: true,
      defaultOp: '>', defaultVal: { kind: 'num', text: '2500', unit: 'bps' } },
  ],
  logic: [
    { id: 'AND', label: { ko: 'AND · 모두 참',     en: 'AND · all'  } },
    { id: 'OR',  label: { ko: 'OR · 하나라도 참',   en: 'OR · any'   } },
    { id: 'NOT', label: { ko: 'NOT · 부정 (단일)',  en: 'NOT · negate' } },
  ],
};

const V5_OPS = {
  address: ['==', '!='], enum: ['==', '!='], boolean: ['==', '!='],
  bps:     ['<', '<=', '>', '>=', '==', '!='],
  seconds: ['<', '<=', '>', '>=', '=='],
  usd:     ['<', '<=', '>', '>=', '=='],
  tokenNative: ['<', '<=', '>', '>=', '==', '!='],
  number:  ['<', '<=', '>', '>=', '==', '!='],
  unix:    ['<', '<=', '>', '>='],
};
const V5_COLOR = {
  recipient: 'cyan', recipientIsContract: 'cyan',
  swapMode: 'slate', feeBps: 'slate', validityExpiresAt: 'slate',
  validityDeltaSec: 'slate', effectiveRateVsOracleBps: 'slate',
  inputAmount: 'sage', outputAmount: 'slate',
  totalInputUsd: 'cyan', totalInputFractionOfPortfolioBps: 'cyan',
};
function v5ColorFor(id) { return V5_COLOR[id] || 'cyan'; }
function v5SigById(id) {
  return V5_SIGS.base.find(s => s.id === id) || V5_SIGS.custom.find(s => s.id === id) || null;
}

let __v5id = 300;
function v5Id(p) { return `${p}${++__v5id}`; }

// ─── initial seed ───────────────────────────────────────────────────────────
function v5BuildBaseline() {
  const nodes = {};
  const wires = [];
  const put = (n) => { nodes[n.id] = n; return n; };

  const root = put({ id: 'root1', kind: 'root', combinator: 'OR', x: 60, y: 60 });

  const mkCond = (id, sigId, op, value, note, x, y, parentId = null) => {
    const s = v5SigById(sigId);
    return put({
      id, kind: 'condition',
      sigId, label: s ? s.label.ko : sigId,
      leafType: s ? s.leafType : 'number',
      operator: op, value,
      custom: !!(s && s.custom),
      absence: s && s.custom ? 'false' : null,
      note,
      parentId, x, y,
    });
  };

  // 2 top-level conditions wired to root
  const c1 = mkCond('g1', 'recipient', '!=', { kind: 'ref', text: 'root.from' }, 'swap-and-send 차단', 360, 60);
  const c2 = mkCond('g2', 'swapMode',  '==', { kind: 'enum', text: 'market' },    '무방어 시장가',     360, 160);
  wires.push({ id: v5Id('w'), from: root.id, to: c1.id });
  wires.push({ id: v5Id('w'), from: root.id, to: c2.id });

  // an AND group with two children, also wired to root
  const grp = put({ id: 'and1', kind: 'group', combinator: 'AND', parentId: null, childIds: [], x: 360, y: 260 });
  const c3 = mkCond('g3', 'validityDeltaSec',    '<',  { kind: 'num', text: '30', unit: 'sec' }, '만료 임박',     0, 0, grp.id);
  const c4 = mkCond('g4', 'recipientIsContract', '==', { kind: 'bool', text: 'true' },           '컨트랙트 수신자', 0, 0, grp.id);
  grp.childIds = [c3.id, c4.id];
  wires.push({ id: v5Id('w'), from: root.id, to: grp.id });

  // Two unwired drafts (floating)
  put({
    id: v5Id('g'), kind: 'condition',
    sigId: 'effectiveRateVsOracleBps',
    label: v5SigById('effectiveRateVsOracleBps').label.ko,
    leafType: 'bps',
    operator: '>', value: { kind: 'num', text: '100', unit: 'bps' },
    custom: true, absence: 'false', note: '검토 중',
    parentId: null, x: 360, y: 460,
  });
  put({
    id: v5Id('g'), kind: 'condition',
    sigId: 'totalInputUsd',
    label: v5SigById('totalInputUsd').label.ko,
    leafType: 'usd',
    operator: '>', value: { kind: 'num', text: '10000', unit: 'USD' },
    custom: true, absence: 'false', note: '대규모만',
    parentId: null, x: 700, y: 460,
  });

  return {
    nodes, wires, rootId: 'root1',
    decision: { kind: 'Deny', reason: 'swap baseline violated', severity: 'FAIL' },
    manifestHash: '#fc20a91',
    signalCounts: { base: V5_SIGS.base.length, custom: V5_SIGS.custom.length },
    pan: { x: 0, y: 0 }, zoom: 1,
  };
}

// ─── reachability ───────────────────────────────────────────────────────────
function v5Included(state, id) {
  if (id === state.rootId) return true;
  // Walk up the parent chain to a top-level ancestor; check it's wired to root.
  let cur = state.nodes[id];
  while (cur && cur.parentId) cur = state.nodes[cur.parentId];
  if (!cur) return false;
  if (cur.id === state.rootId) return true;
  return state.wires.some(w => w.to === cur.id);
}
function v5TopLevelIncludedIds(state) {
  return state.wires.map(w => w.to).filter(id => state.nodes[id]);
}
function v5DraftIds(state) {
  // Top-level non-root nodes not wired in.
  const wiredTos = new Set(state.wires.map(w => w.to));
  return Object.values(state.nodes)
    .filter(n => n.id !== state.rootId && !n.parentId && !wiredTos.has(n.id))
    .map(n => n.id);
}

// ─── reducer ────────────────────────────────────────────────────────────────
function v5Reduce(state, a) {
  switch (a.type) {
    case 'ADD_CONDITION': {
      const s = v5SigById(a.sigId); if (!s) return state;
      const id = v5Id('g');
      const node = {
        id, kind: 'condition',
        sigId: s.id, label: s.label.ko, leafType: s.leafType,
        operator: s.defaultOp || '==',
        value: s.defaultVal ? JSON.parse(JSON.stringify(s.defaultVal)) : { kind: 'num', text: '0' },
        custom: !!s.custom, absence: s.custom ? 'false' : null, note: null,
        parentId: a.parentId || null,
        x: a.x ?? 220, y: a.y ?? 220,
      };
      const nodes = { ...state.nodes, [id]: node };
      if (a.parentId && nodes[a.parentId]) {
        const p = nodes[a.parentId];
        nodes[a.parentId] = { ...p, childIds: [...p.childIds, id] };
      }
      return { ...state, nodes, lastChangeId: id };
    }
    case 'ADD_GROUP': {
      const id = v5Id('grp');
      const node = {
        id, kind: 'group', combinator: a.combinator,
        parentId: a.parentId || null, childIds: [],
        x: a.x ?? 220, y: a.y ?? 220,
      };
      const nodes = { ...state.nodes, [id]: node };
      if (a.parentId && nodes[a.parentId]) {
        const p = nodes[a.parentId];
        nodes[a.parentId] = { ...p, childIds: [...p.childIds, id] };
      }
      return { ...state, nodes, lastChangeId: id };
    }
    case 'MOVE': {
      const n = state.nodes[a.id]; if (!n) return state;
      return { ...state, nodes: { ...state.nodes, [a.id]: { ...n, x: a.x, y: a.y } } };
    }
    case 'ADD_WIRE': {
      // root → top-level node only. Disallow if already wired or node has parent.
      const target = state.nodes[a.to];
      if (!target || target.parentId) return state;
      if (state.wires.some(w => w.to === a.to)) return state;
      return { ...state, wires: [...state.wires, { id: v5Id('w'), from: state.rootId, to: a.to }], lastChangeId: a.to };
    }
    case 'REMOVE_WIRE': {
      return { ...state, wires: state.wires.filter(w => w.id !== a.id), lastChangeId: null };
    }
    case 'ATTACH_TO_GROUP': {
      // Move node into a group. If currently wired to root, remove that wire.
      const nodes = { ...state.nodes };
      const node = nodes[a.id]; if (!node) return state;
      if (a.id === a.parentId) return state;
      if (isDescendantOf(state, a.parentId, a.id)) return state;
      const oldParentId = node.parentId;
      if (oldParentId && nodes[oldParentId]) {
        const op = nodes[oldParentId];
        nodes[oldParentId] = { ...op, childIds: op.childIds.filter(x => x !== a.id) };
      }
      const np = nodes[a.parentId];
      const newKids = np.childIds.slice();
      const idx = typeof a.index === 'number' ? a.index : newKids.length;
      newKids.splice(idx, 0, a.id);
      nodes[a.parentId] = { ...np, childIds: newKids };
      nodes[a.id] = { ...node, parentId: a.parentId };
      // Remove any wire that pointed to this node (no longer top-level)
      const wires = state.wires.filter(w => w.to !== a.id);
      return { ...state, nodes, wires, lastChangeId: a.id };
    }
    case 'DETACH_TO_TOP': {
      const nodes = { ...state.nodes };
      const node = nodes[a.id]; if (!node || !node.parentId) return state;
      const op = nodes[node.parentId];
      nodes[node.parentId] = { ...op, childIds: op.childIds.filter(x => x !== a.id) };
      nodes[a.id] = { ...node, parentId: null, x: a.x ?? node.x, y: a.y ?? node.y };
      return { ...state, nodes, lastChangeId: a.id };
    }
    case 'DELETE': {
      const nodes = { ...state.nodes };
      const toRemove = collectDescendants(state, a.id);
      const node = nodes[a.id];
      if (node && node.parentId && nodes[node.parentId]) {
        const op = nodes[node.parentId];
        nodes[node.parentId] = { ...op, childIds: op.childIds.filter(x => x !== a.id) };
      }
      toRemove.forEach(id => { delete nodes[id]; });
      const wires = state.wires.filter(w => !toRemove.includes(w.to));
      return { ...state, nodes, wires, lastChangeId: a.id };
    }
    case 'PATCH_CONDITION': {
      const n = state.nodes[a.id]; if (!n) return state;
      const next = { ...n, ...a.patch };
      if (a.patch.sigId) {
        const s = v5SigById(a.patch.sigId);
        if (s) { next.label = s.label.ko; next.leafType = s.leafType; next.custom = !!s.custom; }
      }
      return { ...state, nodes: { ...state.nodes, [a.id]: next }, lastChangeId: a.id };
    }
    case 'PATCH_GROUP': {
      const n = state.nodes[a.id]; if (!n) return state;
      return { ...state, nodes: { ...state.nodes, [a.id]: { ...n, ...a.patch } }, lastChangeId: a.id };
    }
    case 'PATCH_DECISION': {
      return { ...state, decision: { ...state.decision, ...a.patch } };
    }
    case 'SET_VIEW': {
      return { ...state, pan: a.pan ?? state.pan, zoom: a.zoom ?? state.zoom };
    }
    default: return state;
  }
}

function isDescendantOf(state, candidate, ancestorId) {
  let cur = state.nodes[candidate];
  while (cur) { if (cur.id === ancestorId) return true; cur = state.nodes[cur.parentId]; }
  return false;
}
function collectDescendants(state, id) {
  const out = [id]; const n = state.nodes[id]; if (!n) return out;
  for (const c of (n.childIds || [])) out.push(...collectDescendants(state, c));
  return out;
}

// ─── Cedar serializer ───────────────────────────────────────────────────────
function v5ToCedar(state) {
  const lines = []; let n = 0;
  const push = (text, meta = {}) => { n++; lines.push({ n, text, ...meta }); };
  push('// Swap baseline · manifest ' + state.manifestHash, { kind: 'cmt' });
  push(state.decision.kind === 'Deny' ? 'forbid (' : 'permit (', { kind: 'kw' });
  push('  principal,', { kind: 'arg' });
  push('  action == Action::"swap",', { kind: 'arg' });
  push('  resource', { kind: 'arg' });
  push(')', { kind: 'punct' });
  push('when {', { kind: 'kw' });

  const root = state.nodes[state.rootId];
  const wiredIds = state.wires.map(w => w.to);
  if (wiredIds.length === 0) {
    push('  // (no top-level conditions wired to root — policy inactive)', { kind: 'cmt' });
  } else {
    const joiner = root.combinator === 'AND' ? '&&' : '||';
    wiredIds.forEach((id, i) => {
      const sub = serializeNode(state, id);
      const child = state.nodes[id];
      const needsParen = child && child.kind === 'group' && child.childIds.length > 1;
      sub.forEach((s, j) => {
        let prefix = '';
        let suffix = '';
        if (i > 0 && j === 0) prefix = joiner + ' ';
        else if (j > 0) prefix = '   ';
        if (needsParen && sub.length === 1) { prefix += '('; suffix = ')'; }
        else if (needsParen && j === 0) prefix += '(';
        else if (needsParen && j === sub.length - 1) suffix = ')';
        push('  ' + prefix + s.text + suffix, { kind: 'guard', guardId: s.guardId, custom: s.custom });
      });
    });
  }
  push('};', { kind: 'kw' });
  push(`// → ${state.decision.kind} "${state.decision.reason}"  (severity: ${state.decision.severity})`, { kind: 'cmt' });

  const drafts = v5DraftIds(state).length;
  if (drafts) push(`// ⚠ 미연결 노드 ${drafts}개 — 정책에 포함되지 않음`, { kind: 'cmt' });
  return { lines };
}
function serializeNode(state, id) {
  const n = state.nodes[id]; if (!n) return [];
  if (n.kind === 'condition') return [{ text: condToCedar(n), guardId: n.id, custom: n.custom }];
  // group
  if (n.childIds.length === 0) return [{ text: n.combinator === 'AND' ? 'true' : 'false', guardId: n.id }];
  if (n.combinator === 'NOT') {
    const sub = serializeNode(state, n.childIds[0]);
    return sub.map(s => ({ text: '!(' + s.text + ')', guardId: s.guardId, custom: s.custom }));
  }
  const joiner = n.combinator === 'AND' ? '&&' : '||';
  const segs = [];
  n.childIds.forEach((cid, i) => {
    const child = state.nodes[cid]; if (!child) return;
    const sub = serializeNode(state, cid);
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
function condToCedar(leaf) {
  const path = leaf.custom ? `context.custom.${leaf.sigId}` : `context.${leaf.sigId}`;
  const v = leaf.value || {};
  let val;
  if (v.kind === 'ref')       val = (v.text || '').replace(/^root\./, 'context.');
  else if (v.kind === 'enum') val = `"${v.text}"`;
  else if (v.kind === 'bool') val = v.text;
  else if (v.kind === 'num')  val = v.text;
  else val = `"${v.text}"`;
  const note = leaf.note ? `  // ${leaf.note}` : '';
  if (leaf.custom) return `(context.custom has ${leaf.sigId} && ${path} ${leaf.operator} ${val})${note}`;
  return `${path} ${leaf.operator} ${val}${note}`;
}

// ─── Evaluator ──────────────────────────────────────────────────────────────
function v5Evaluate(state, tx) {
  const matched = [];
  const root = state.nodes[state.rootId];
  const wiredIds = state.wires.map(w => w.to);
  if (wiredIds.length === 0) return { matchedLeafIds: [], decision: { kind: 'Allow', reason: '루트에 연결된 조건 없음', severity: 'PASS' }, triggered: false };
  const fn = root.combinator === 'AND' ? 'every' : 'some';
  const triggered = wiredIds[fn](id => evalNode(state, id, tx, matched));
  return {
    matchedLeafIds: matched,
    decision: triggered ? state.decision : { kind: 'Allow', reason: '정책에 일치하는 가드 없음', severity: 'PASS' },
    triggered,
  };
}
function evalNode(state, id, tx, matched) {
  const n = state.nodes[id]; if (!n) return false;
  if (n.kind === 'condition') {
    const ok = evalCond(n, tx); if (ok) matched.push(n.id); return ok;
  }
  if (n.childIds.length === 0) return n.combinator === 'AND';
  if (n.combinator === 'NOT') return !evalNode(state, n.childIds[0], tx, matched);
  if (n.combinator === 'AND') return n.childIds.every(c => evalNode(state, c, tx, matched));
  return n.childIds.some(c => evalNode(state, c, tx, matched));
}
function evalCond(leaf, tx) {
  const lhs = tx[leaf.sigId];
  if (leaf.custom && (lhs === undefined || lhs === null)) return leaf.absence === 'true';
  const v = leaf.value;
  if (v.kind === 'ref')  { const r = (v.text || '').replace(/^root\./, ''); return cmp(leaf.operator, lhs, tx[r]); }
  if (v.kind === 'enum') return cmp(leaf.operator, String(lhs), v.text);
  if (v.kind === 'bool') return cmp(leaf.operator, !!lhs, v.text === 'true');
  if (v.kind === 'num')  return cmp(leaf.operator, Number(lhs), Number(v.text));
  return false;
}
function cmp(op, a, b) {
  switch (op) {
    case '==': return a === b; case '!=': return a !== b;
    case '<':  return a <  b;  case '<=': return a <= b;
    case '>':  return a >  b;  case '>=': return a >= b;
    default:   return false;
  }
}

// ─── fixtures ───────────────────────────────────────────────────────────────
const V5_FIXTURES = [
  { id: 'fx1', label: 'Calm swap · USDC→WETH',
    tx: { from: '0xA1c4...7e29', recipient: '0xA1c4...7e29', swapMode: 'limit',
          inputAmount: 0.5, outputAmount: 1512, feeBps: 30,
          validityDeltaSec: 612, recipientIsContract: false,
          effectiveRateVsOracleBps: 22, totalInputUsd: 900,
          totalInputFractionOfPortfolioBps: 90 } },
  { id: 'fx2', label: 'Market swap · 만료 임박',
    tx: { from: '0xA1c4...7e29', recipient: '0xA1c4...7e29', swapMode: 'market',
          inputAmount: 0.5, outputAmount: 1512, feeBps: 60,
          validityDeltaSec: 18, recipientIsContract: false,
          effectiveRateVsOracleBps: 145, totalInputUsd: 4800,
          totalInputFractionOfPortfolioBps: 420 } },
  { id: 'fx3', label: 'Send to contract',
    tx: { from: '0xA1c4...7e29', recipient: '0x8c2f...4910', swapMode: 'limit',
          inputAmount: 1000, outputAmount: 0.34, feeBps: 25,
          validityDeltaSec: 480, recipientIsContract: true,
          effectiveRateVsOracleBps: 18, totalInputUsd: 1000,
          totalInputFractionOfPortfolioBps: 100 } },
];

Object.assign(window, {
  V5_SIGS, V5_OPS, v5ColorFor, v5SigById, v5Id, v5BuildBaseline,
  v5Included, v5TopLevelIncludedIds, v5DraftIds,
  v5Reduce, v5ToCedar, v5Evaluate, V5_FIXTURES,
});
