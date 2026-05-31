// editor-v3-state.js
// v3 IR: free-canvas with positions + magnetic-snap connections.
//
// Model:
//   state.nodes  — Map<id, Node>   (every block/container lives here)
//   state.rootId — id of the root container (always in policy)
//
// Node shape:
//   { id, kind: 'leaf'|'container', op?: 'AND'|'OR',
//     sigId?, leafType?, operator?, value?, custom?, absence?, note?,
//     parentId: id|null,        // null + not root  → DRAFT (excluded from policy)
//     childIds: id[],            // for containers
//     x, y,                      // canvas position (for drafts AND root)
//     w?, h? }
//
// Connection rule: a node is IN POLICY iff it can reach rootId via parentId chain.
// Drafts are excluded from Cedar serialization and from evaluation.

// ────────────────── Signal catalog ──────────────────
const V3_SIGS = {
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
};

const V3_OPS = {
  address:     ['==', '!='],
  enum:        ['==', '!='],
  boolean:     ['==', '!='],
  bps:         ['<', '<=', '>', '>=', '==', '!='],
  seconds:     ['<', '<=', '>', '>=', '=='],
  usd:         ['<', '<=', '>', '>=', '=='],
  tokenNative: ['<', '<=', '>', '>=', '==', '!='],
  number:      ['<', '<=', '>', '>=', '==', '!='],
  unix:        ['<', '<=', '>', '>='],
};

const V3_COLOR = {
  recipient: 'cyan', recipientIsContract: 'cyan',
  swapMode: 'slate', feeBps: 'slate', validityExpiresAt: 'slate',
  validityDeltaSec: 'slate', effectiveRateVsOracleBps: 'slate',
  inputAmount: 'sage', outputAmount: 'sage',
  totalInputUsd: 'cyan', totalInputFractionOfPortfolioBps: 'cyan',
};
function v3ColorFor(id) { return V3_COLOR[id] || 'cyan'; }
function v3SigById(id) {
  return V3_SIGS.base.find(s => s.id === id) || V3_SIGS.custom.find(s => s.id === id) || null;
}

// ────────────────── ID factory ──────────────────
let __v3id = 200;
function v3Id(p) { return `${p}${++__v3id}`; }

// ────────────────── Initial seed ──────────────────
// Two drafts on canvas + one root OR container with 4 children.
function v3BuildBaseline() {
  const nodes = {};
  const put = (n) => { nodes[n.id] = n; return n; };

  const root = put({
    id: 'rootC', kind: 'container', op: 'OR',
    parentId: null, childIds: [],
    x: 280, y: 60, w: 760,
  });
  const leaf = (id, sigId, op, value, note) => {
    const s = v3SigById(sigId);
    return put({
      id, kind: 'leaf',
      sigId, label: s ? s.label.ko : sigId,
      leafType: s ? s.leafType : 'number',
      operator: op, value,
      custom: !!(s && s.custom),
      absence: s && s.custom ? 'false' : null,
      note,
      parentId: 'rootC', childIds: [],
      x: 0, y: 0,
    });
  };
  const g1 = leaf('g1', 'recipient',           '!=', { kind: 'ref',  text: 'root.from' },        'swap-and-send 차단');
  const g2 = leaf('g2', 'swapMode',            '==', { kind: 'enum', text: 'market' },           '무방어 시장가');
  const g3 = leaf('g3', 'validityDeltaSec',    '<',  { kind: 'num',  text: '30', unit: 'sec' },  '만료 임박');
  const g4 = leaf('g4', 'recipientIsContract', '==', { kind: 'bool', text: 'true' },             '컨트랙트 수신자');
  root.childIds = [g1.id, g2.id, g3.id, g4.id];

  // Two drafts floating on canvas — to demo the unconnected state
  put({
    id: v3Id('d'), kind: 'leaf',
    sigId: 'effectiveRateVsOracleBps',
    label: v3SigById('effectiveRateVsOracleBps').label.ko,
    leafType: 'bps',
    operator: '>', value: { kind: 'num', text: '100', unit: 'bps' },
    custom: true, absence: 'false', note: '검토 중',
    parentId: null, childIds: [],
    x: 80, y: 540,
  });
  put({
    id: v3Id('d'), kind: 'leaf',
    sigId: 'totalInputUsd',
    label: v3SigById('totalInputUsd').label.ko,
    leafType: 'usd',
    operator: '>', value: { kind: 'num', text: '10000', unit: 'USD' },
    custom: true, absence: 'false', note: '대규모만',
    parentId: null, childIds: [],
    x: 360, y: 560,
  });

  return {
    nodes,
    rootId: 'rootC',
    decision: { kind: 'Deny', reason: 'swap baseline violated', severity: 'FAIL' },
    manifestHash: '#fc20a91',
    signalCounts: { base: 6, custom: 5 },
  };
}

// ────────────────── Helpers ──────────────────
function v3InPolicy(state, id) {
  let cur = state.nodes[id];
  while (cur && cur.parentId) cur = state.nodes[cur.parentId];
  return cur && cur.id === state.rootId;
}
function v3DraftIds(state) {
  return Object.keys(state.nodes).filter(id => {
    const n = state.nodes[id];
    if (id === state.rootId) return false;
    return n.parentId === null;
  });
}
function v3HasOR(state) {
  return Object.values(state.nodes).some(n => n.kind === 'container' && n.op === 'OR' && v3InPolicy(state, n.id));
}

// ────────────────── Reducer ──────────────────
function v3Reduce(state, a) {
  switch (a.type) {
    case 'ADD_FROM_PALETTE': {
      const s = v3SigById(a.sigId);
      if (!s) return state;
      const id = v3Id('g');
      const node = {
        id, kind: 'leaf',
        sigId: s.id, label: s.label.ko,
        leafType: s.leafType,
        operator: s.defaultOp || '==',
        value: s.defaultVal ? JSON.parse(JSON.stringify(s.defaultVal)) : { kind: 'num', text: '0' },
        custom: !!s.custom, absence: s.custom ? 'false' : null, note: null,
        parentId: a.parentId || null,
        childIds: [],
        x: a.x ?? 120, y: a.y ?? 120,
      };
      const nodes = { ...state.nodes, [id]: node };
      if (a.parentId && nodes[a.parentId]) {
        const p = nodes[a.parentId];
        nodes[a.parentId] = { ...p, childIds: [...p.childIds, id] };
      }
      return { ...state, nodes, lastChangeId: id };
    }
    case 'ADD_CONTAINER': {
      const id = v3Id('c');
      const node = {
        id, kind: 'container', op: a.op,
        parentId: a.parentId || null,
        childIds: [],
        x: a.x ?? 140, y: a.y ?? 140, w: 360,
      };
      const nodes = { ...state.nodes, [id]: node };
      if (a.parentId && nodes[a.parentId]) {
        const p = nodes[a.parentId];
        nodes[a.parentId] = { ...p, childIds: [...p.childIds, id] };
      }
      return { ...state, nodes, lastChangeId: id };
    }
    case 'MOVE': {
      const n = state.nodes[a.id];
      if (!n) return state;
      return { ...state, nodes: { ...state.nodes, [a.id]: { ...n, x: a.x, y: a.y } } };
    }
    case 'ATTACH': {
      // Move a.id into container a.parentId at position a.index (or end).
      // Detach from previous parent first.
      const nodes = { ...state.nodes };
      const node = nodes[a.id]; if (!node) return state;
      // Don't allow attaching to self or to a descendant
      if (a.id === a.parentId) return state;
      if (isDescendantOf(state, a.parentId, a.id)) return state;
      if (node.parentId === a.parentId) {
        // Just reorder
        const p = nodes[a.parentId];
        const cur = p.childIds.indexOf(a.id);
        const rest = p.childIds.filter(x => x !== a.id);
        const idx = typeof a.index === 'number' ? a.index : rest.length;
        rest.splice(idx, 0, a.id);
        nodes[a.parentId] = { ...p, childIds: rest };
        return { ...state, nodes, lastChangeId: a.id };
      }
      // Remove from old parent
      if (node.parentId && nodes[node.parentId]) {
        const op = nodes[node.parentId];
        nodes[node.parentId] = { ...op, childIds: op.childIds.filter(x => x !== a.id) };
      }
      // Insert into new parent
      const np = nodes[a.parentId];
      const newKids = np.childIds.slice();
      const idx = typeof a.index === 'number' ? a.index : newKids.length;
      newKids.splice(idx, 0, a.id);
      nodes[a.parentId] = { ...np, childIds: newKids };
      nodes[a.id] = { ...node, parentId: a.parentId };
      return { ...state, nodes, lastChangeId: a.id };
    }
    case 'DETACH': {
      // Pop a.id out to canvas at (a.x, a.y) as a draft.
      const nodes = { ...state.nodes };
      const node = nodes[a.id]; if (!node) return state;
      if (node.parentId && nodes[node.parentId]) {
        const op = nodes[node.parentId];
        nodes[node.parentId] = { ...op, childIds: op.childIds.filter(x => x !== a.id) };
      }
      nodes[a.id] = { ...node, parentId: null, x: a.x ?? node.x, y: a.y ?? node.y };
      return { ...state, nodes, lastChangeId: a.id };
    }
    case 'DELETE': {
      // Recursive delete
      const nodes = { ...state.nodes };
      const toRemove = collectDescendants(state, a.id);
      const node = nodes[a.id];
      if (node && node.parentId && nodes[node.parentId]) {
        const op = nodes[node.parentId];
        nodes[node.parentId] = { ...op, childIds: op.childIds.filter(x => x !== a.id) };
      }
      toRemove.forEach(id => { delete nodes[id]; });
      return { ...state, nodes, lastChangeId: a.id };
    }
    case 'PATCH_LEAF': {
      const n = state.nodes[a.id]; if (!n) return state;
      const next = { ...n, ...a.patch };
      if (a.patch.sigId) {
        const s = v3SigById(a.patch.sigId);
        if (s) {
          next.label = s.label.ko;
          next.leafType = s.leafType;
          next.custom = !!s.custom;
        }
      }
      return { ...state, nodes: { ...state.nodes, [a.id]: next }, lastChangeId: a.id };
    }
    case 'PATCH_CONT': {
      const n = state.nodes[a.id]; if (!n) return state;
      return { ...state, nodes: { ...state.nodes, [a.id]: { ...n, ...a.patch } }, lastChangeId: a.id };
    }
    case 'PATCH_DECISION': {
      return { ...state, decision: { ...state.decision, ...a.patch } };
    }
    case 'RESIZE_ROOT': {
      const r = state.nodes[state.rootId];
      return { ...state, nodes: { ...state.nodes, [state.rootId]: { ...r, w: a.w } } };
    }
    default: return state;
  }
}

function isDescendantOf(state, candidateAncestor, ofNode) {
  let cur = state.nodes[candidateAncestor];
  while (cur) {
    if (cur.id === ofNode) return true;
    cur = state.nodes[cur.parentId];
  }
  return false;
}
function collectDescendants(state, id) {
  const out = [id];
  const n = state.nodes[id];
  if (!n) return out;
  for (const c of (n.childIds || [])) out.push(...collectDescendants(state, c));
  return out;
}

// ────────────────── Cedar serializer ──────────────────
function v3ToCedar(state) {
  const lines = [];
  let n = 0;
  const push = (text, meta = {}) => { n++; lines.push({ n, text, ...meta }); };

  push('// Swap baseline · manifest ' + state.manifestHash, { kind: 'cmt' });
  push(state.decision.kind === 'Deny' ? 'forbid (' : 'permit (', { kind: 'kw' });
  push('  principal,', { kind: 'arg' });
  push('  action == Action::"swap",', { kind: 'arg' });
  push('  resource', { kind: 'arg' });
  push(')', { kind: 'punct' });
  push('when {', { kind: 'kw' });

  const body = serializeNode(state, state.rootId, 1);
  body.forEach(seg => push('  ' + seg.text, { kind: 'guard', guardId: seg.guardId, custom: seg.custom }));

  push('};', { kind: 'kw' });
  push(`// → ${state.decision.kind} "${state.decision.reason}"  (severity: ${state.decision.severity})`, { kind: 'cmt' });

  const draftCount = v3DraftIds(state).length;
  if (draftCount) {
    push(`// ⚠ 미연결 블록 ${draftCount}개 — 정책에 포함되지 않음`, { kind: 'cmt' });
  }
  return { lines };
}

function serializeNode(state, id, depth) {
  const n = state.nodes[id];
  if (!n) return [];
  if (n.kind === 'leaf') return [{ text: leafToCedar(n), guardId: n.id, custom: n.custom }];

  if (n.childIds.length === 0) {
    return [{ text: n.op === 'AND' ? 'true /* AND (empty) */' : 'false /* OR (empty) */', guardId: n.id }];
  }
  const joiner = n.op === 'AND' ? '&&' : '||';
  const segs = [];
  n.childIds.forEach((cid, i) => {
    const child = state.nodes[cid];
    if (!child) return;
    const sub = serializeNode(state, cid, depth + 1);
    const wrapNested = child.kind === 'container' && child.childIds.length > 1 && id !== state.rootId;
    if (wrapNested) {
      if (sub.length === 1) {
        segs.push({ text: (i === 0 ? '' : joiner + ' ') + '(' + sub[0].text + ')', guardId: sub[0].guardId, custom: sub[0].custom });
      } else {
        sub.forEach((s, j) => {
          const prefix = (i === 0 && j === 0) ? '(' : (j === 0 ? joiner + ' (' : '   ');
          const suffix = (j === sub.length - 1) ? ')' : '';
          segs.push({ text: prefix + s.text + suffix, guardId: s.guardId, custom: s.custom });
        });
      }
    } else {
      sub.forEach((s, j) => {
        const prefix = (i === 0 && j === 0) ? '' : (j === 0 ? joiner + ' ' : '   ');
        segs.push({ text: prefix + s.text, guardId: s.guardId, custom: s.custom });
      });
    }
  });
  return segs;
}

function leafToCedar(leaf) {
  const path = leaf.custom ? `context.custom.${leaf.sigId}` : `context.${leaf.sigId}`;
  const op = leaf.operator;
  let val;
  const v = leaf.value || {};
  if (v.kind === 'ref')       val = (v.text || '').replace(/^root\./, 'context.');
  else if (v.kind === 'enum') val = `"${v.text}"`;
  else if (v.kind === 'bool') val = v.text;
  else if (v.kind === 'num')  val = v.text;
  else val = `"${v.text}"`;
  const note = leaf.note ? `  // ${leaf.note}` : '';
  if (leaf.custom) return `(context.custom has ${leaf.sigId} && ${path} ${op} ${val})${note}`;
  return `${path} ${op} ${val}${note}`;
}

// ────────────────── Evaluator ──────────────────
function v3Evaluate(state, tx) {
  const matched = [];
  const triggered = evalNode(state, state.rootId, tx, matched);
  return {
    matchedLeafIds: matched,
    decision: triggered ? state.decision : { kind: 'Allow', reason: '정책에 일치하는 가드 없음', severity: 'PASS' },
    triggered,
  };
}
function evalNode(state, id, tx, matched) {
  const n = state.nodes[id]; if (!n) return false;
  if (n.kind === 'leaf') {
    const ok = evalLeaf(n, tx);
    if (ok) matched.push(n.id);
    return ok;
  }
  if (n.childIds.length === 0) return n.op === 'AND';
  if (n.op === 'AND') return n.childIds.every(c => evalNode(state, c, tx, matched));
  return n.childIds.some(c => evalNode(state, c, tx, matched));
}
function evalLeaf(leaf, tx) {
  const lhs = tx[leaf.sigId];
  if (leaf.custom && (lhs === undefined || lhs === null)) return leaf.absence === 'true';
  const v = leaf.value;
  if (v.kind === 'ref') {
    const r = (v.text || '').replace(/^root\./, '');
    return cmp(leaf.operator, lhs, tx[r]);
  }
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

// ────────────────── Test fixtures ──────────────────
const V3_FIXTURES = [
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
  V3_SIGS, V3_OPS, v3ColorFor, v3SigById, v3BuildBaseline, v3InPolicy, v3DraftIds, v3HasOR,
  v3Reduce, v3ToCedar, v3Evaluate, v3Id, V3_FIXTURES,
});
