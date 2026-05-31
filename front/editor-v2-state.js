// editor-v2-state.js
// Mutable IR + reducer + Cedar serializer + evaluator for the functional editor.
// Plain JS, loaded before React/Babel scripts.

// ───────────────────────────── Signal catalog ───────────────────────────────
// Same shape as v1's editor-policy.js but slimmer + tagged with leafType so the
// canvas can pick the right operator/value widget when the user drags a chip in.

const V2_SIGNAL_CATALOG = {
  base: [
    { id: 'recipient',    label: { ko: 'Recipient',          en: 'Recipient'        },
      leafType: 'address', shape: 'rect',    defaultOp: '!=', defaultVal: { kind: 'ref', text: 'root.from' } },
    { id: 'swapMode',     label: { ko: 'Swap direction',     en: 'Swap direction'   },
      leafType: 'enum',    shape: 'hexagon', options: ['market', 'limit'],
      defaultOp: '==', defaultVal: { kind: 'enum', text: 'market' } },
    { id: 'feeBps',       label: { ko: 'Fee (bps)',          en: 'Fee (bps)'        },
      leafType: 'bps',     shape: 'pill',    defaultOp: '>', defaultVal: { kind: 'num', text: '50', unit: 'bps' } },
    { id: 'inputAmount',  label: { ko: 'Input amount',       en: 'Input amount'     },
      leafType: 'tokenNative', shape: 'pill', defaultOp: '>', defaultVal: { kind: 'num', text: '1.0' } },
    { id: 'outputAmount', label: { ko: 'Output amount',      en: 'Output amount'    },
      leafType: 'tokenNative', shape: 'pill', defaultOp: '<', defaultVal: { kind: 'num', text: '0' } },
    { id: 'validityExpiresAt', label: { ko: 'Deadline',      en: 'Deadline'         },
      leafType: 'unix',    shape: 'rect',    defaultOp: '<', defaultVal: { kind: 'num', text: '0' } },
  ],
  custom: [
    { id: 'validityDeltaSec', label: { ko: '만료까지 (sec)',   en: 'Time to deadline' },
      leafType: 'seconds', shape: 'pill',    custom: true,
      defaultOp: '<', defaultVal: { kind: 'num', text: '30', unit: 'sec' } },
    { id: 'recipientIsContract', label: { ko: '수신자가 컨트랙트', en: 'Recipient is contract' },
      leafType: 'boolean', shape: 'hexagon', custom: true,
      defaultOp: '==', defaultVal: { kind: 'bool', text: 'true' } },
    { id: 'effectiveRateVsOracleBps', label: { ko: 'Oracle 대비 슬리피지', en: 'Slippage vs oracle' },
      leafType: 'bps',     shape: 'pill',    custom: true,
      defaultOp: '>', defaultVal: { kind: 'num', text: '100', unit: 'bps' } },
    { id: 'totalInputUsd', label: { ko: '입력 가치 (USD)',     en: 'Input value (USD)' },
      leafType: 'usd',     shape: 'pill',    custom: true,
      defaultOp: '>', defaultVal: { kind: 'num', text: '10000', unit: 'USD' } },
    { id: 'totalInputFractionOfPortfolioBps', label: { ko: '입력 ÷ 포트폴리오', en: 'Input ÷ portfolio' },
      leafType: 'bps',     shape: 'pill',    custom: true,
      defaultOp: '>', defaultVal: { kind: 'num', text: '2500', unit: 'bps' } },
  ],
};

const V2_OPERATORS_BY_TYPE = {
  address:     ['==', '!='],
  symbol:      ['==', '!=', 'in'],
  enum:        ['==', '!='],
  boolean:     ['==', '!='],
  bps:         ['<', '<=', '>', '>=', '==', '!='],
  seconds:     ['<', '<=', '>', '>=', '=='],
  usd:         ['<', '<=', '>', '>=', '=='],
  tokenNative: ['<', '<=', '>', '>=', '==', '!='],
  number:      ['<', '<=', '>', '>=', '==', '!='],
  unix:        ['<', '<=', '>', '>='],
};

// Color → domain. Three baselines from v1; only "io-balanced" used by default.
const V2_COLOR_MAP = {
  recipient: 'cyan', recipientIsContract: 'cyan',
  swapMode: 'slate', feeBps: 'slate', validityExpiresAt: 'slate', validityDeltaSec: 'slate',
  effectiveRateVsOracleBps: 'slate',
  inputAmount: 'sage', outputAmount: 'sage',
  totalInputUsd: 'cyan', totalInputFractionOfPortfolioBps: 'cyan',
};

function v2ColorFor(sigId) { return V2_COLOR_MAP[sigId] || 'cyan'; }
function v2SigById(id) {
  return V2_SIGNAL_CATALOG.base.find(s => s.id === id) ||
         V2_SIGNAL_CATALOG.custom.find(s => s.id === id) || null;
}

// ───────────────────────────── ID factory ───────────────────────────────────
let __v2NextId = 100;
function v2Id(prefix) { return `${prefix}${++__v2NextId}`; }

// ───────────────────────────── Initial IR ───────────────────────────────────
// The baseline policy — OR root, 4 leaves. Matches v1 exactly so the demo lands
// on the "canvas (nested)" layout immediately.

function v2BuildBaseline() {
  return {
    root: {
      kind: 'container', op: 'OR', id: 'or-root',
      children: [
        leaf('g1', 'recipient',           '!=', { kind: 'ref',  text: 'root.from' },                 'swap-and-send 차단'),
        leaf('g2', 'swapMode',            '==', { kind: 'enum', text: 'market' },                    '무방어 시장가 차단'),
        leaf('g3', 'validityDeltaSec',    '<',  { kind: 'num',  text: '30', unit: 'sec' },           '만료 임박'),
        leaf('g4', 'recipientIsContract', '==', { kind: 'bool', text: 'true' },                      '컨트랙트 수신자'),
      ],
    },
    decision: { kind: 'Deny', reason: 'swap baseline violated', severity: 'FAIL' },
    manifestHash: '#fc20a91',
    signalCounts: { base: 6, custom: 5 },
  };
  function leaf(id, sigId, op, value, note) {
    const s = v2SigById(sigId);
    return {
      kind: 'leaf', id, sigId, label: s ? s.label.ko : sigId,
      leafType: s ? s.leafType : 'number',
      operator: op, value,
      custom: !!(s && s.custom),
      absence: s && s.custom ? 'false' : null,
      note,
    };
  }
}

// A "fresh flat AND" example so reviewers see the adaptive form layout.
function v2BuildFlatAND() {
  return {
    root: {
      kind: 'container', op: 'AND', id: 'and-root',
      children: [
        leaf('h1', 'swapMode',     '==', { kind: 'enum', text: 'market' }, '시장가일 때'),
        leaf('h2', 'feeBps',       '>',  { kind: 'num',  text: '50', unit: 'bps' }, '수수료 과다'),
        leaf('h3', 'totalInputUsd', '>',  { kind: 'num',  text: '10000', unit: 'USD' }, '대규모 거래'),
      ],
    },
    decision: { kind: 'Deny', reason: 'large market swap', severity: 'WARN' },
    manifestHash: '#fc20a91',
    signalCounts: { base: 6, custom: 5 },
  };
  function leaf(id, sigId, op, value, note) {
    const s = v2SigById(sigId);
    return {
      kind: 'leaf', id, sigId, label: s ? s.label.ko : sigId,
      leafType: s ? s.leafType : 'number',
      operator: op, value,
      custom: !!(s && s.custom),
      absence: s && s.custom ? 'false' : null,
      note,
    };
  }
}

// ───────────────────────────── Structure detection ──────────────────────────
// "Flat AND" = root.op === 'AND' AND none of the children are containers.

function v2IsFlatAND(root) {
  if (!root || root.kind !== 'container') return false;
  if (root.op !== 'AND') return false;
  return root.children.every(c => c.kind === 'leaf');
}

function v2HasOR(node) {
  if (!node || node.kind !== 'container') return false;
  if (node.op === 'OR') return true;
  return node.children.some(c => v2HasOR(c));
}

// ───────────────────────────── Reducer ──────────────────────────────────────
// All mutations go through this. Returns a new root tree (immutable update).
// Actions:
//   ADD_LEAF      { containerId, sigId, position? }
//   ADD_CONTAINER { containerId, op }
//   UPDATE_LEAF   { leafId, patch }     // partial leaf merge
//   UPDATE_CONT   { containerId, patch } // partial container merge (op flip)
//   DELETE_NODE   { id }                 // by id
//   MOVE_NODE     { id, targetContainerId, position }
//   REPLACE_ROOT  { root }

function v2Reduce(state, action) {
  switch (action.type) {
    case 'ADD_LEAF': {
      const sig = v2SigById(action.sigId);
      if (!sig) return state;
      const newLeaf = {
        kind: 'leaf', id: v2Id('g'), sigId: sig.id, label: sig.label.ko,
        leafType: sig.leafType,
        operator: sig.defaultOp || '==',
        value: sig.defaultVal ? JSON.parse(JSON.stringify(sig.defaultVal)) : { kind: 'num', text: '0' },
        custom: !!sig.custom,
        absence: sig.custom ? 'false' : null,
        note: null,
      };
      const root = mapTree(state.root, n => {
        if (n.kind !== 'container' || n.id !== action.containerId) return n;
        const children = n.children.slice();
        if (typeof action.position === 'number') children.splice(action.position, 0, newLeaf);
        else children.push(newLeaf);
        return { ...n, children };
      });
      return { ...state, root, lastAdded: newLeaf.id };
    }
    case 'ADD_CONTAINER': {
      const cont = { kind: 'container', op: action.op, id: v2Id('c'), children: [] };
      const root = mapTree(state.root, n => {
        if (n.kind !== 'container' || n.id !== action.containerId) return n;
        return { ...n, children: [...n.children, cont] };
      });
      return { ...state, root, lastAdded: cont.id };
    }
    case 'UPDATE_LEAF': {
      const root = mapTree(state.root, n => {
        if (n.kind !== 'leaf' || n.id !== action.leafId) return n;
        const merged = { ...n, ...action.patch };
        // Re-derive label if sig changed
        if (action.patch.sigId) {
          const sig = v2SigById(action.patch.sigId);
          if (sig) {
            merged.label = sig.label.ko;
            merged.leafType = sig.leafType;
            merged.custom = !!sig.custom;
          }
        }
        return merged;
      });
      return { ...state, root };
    }
    case 'UPDATE_CONT': {
      const root = mapTree(state.root, n => {
        if (n.kind !== 'container' || n.id !== action.containerId) return n;
        return { ...n, ...action.patch };
      });
      return { ...state, root };
    }
    case 'DELETE_NODE': {
      const root = filterTree(state.root, n => n.id !== action.id);
      return { ...state, root };
    }
    case 'MOVE_NODE': {
      // Detach
      let detached = null;
      const without = filterTree(state.root, n => {
        if (n.id === action.id) { detached = n; return false; }
        return true;
      });
      if (!detached) return state;
      const root = mapTree(without, n => {
        if (n.kind !== 'container' || n.id !== action.targetContainerId) return n;
        const children = n.children.slice();
        if (typeof action.position === 'number') children.splice(action.position, 0, detached);
        else children.push(detached);
        return { ...n, children };
      });
      return { ...state, root };
    }
    case 'REPLACE_ROOT': {
      return { ...state, root: action.root };
    }
    case 'UPDATE_DECISION': {
      return { ...state, decision: { ...state.decision, ...action.patch } };
    }
    default:
      return state;
  }
}

function mapTree(node, fn) {
  if (!node) return node;
  const mapped = fn(node);
  if (mapped !== node) return mapped;
  if (node.kind === 'container') {
    const children = node.children.map(c => mapTree(c, fn));
    if (children.some((c, i) => c !== node.children[i])) {
      return { ...node, children };
    }
  }
  return node;
}

function filterTree(node, pred) {
  if (!node) return node;
  if (!pred(node)) return null;
  if (node.kind === 'container') {
    const children = node.children.map(c => filterTree(c, pred)).filter(Boolean);
    return { ...node, children };
  }
  return node;
}

// ───────────────────────────── Cedar serializer ─────────────────────────────
// Turns the IR into Cedar text + per-line metadata (line → {guardId, custom}).

function v2ToCedar(state) {
  const lines = [];
  let n = 0;
  const push = (text, meta = {}) => { n++; lines.push({ n, text, ...meta }); };

  push('// Swap baseline · manifest ' + state.manifestHash, { kind: 'comment' });
  push(state.decision.kind === 'Deny' ? 'forbid (' : 'permit (', { kind: 'kw' });
  push('  principal,', { kind: 'arg' });
  push('  action == Action::"swap",', { kind: 'arg' });
  push('  resource', { kind: 'arg' });
  push(')', { kind: 'punct' });
  push('when {', { kind: 'kw' });

  const body = serializeNode(state.root, 1);
  // Split on \n, attach metadata
  body.forEach(seg => push('  ' + seg.text, { kind: 'guard', guardId: seg.guardId, custom: seg.custom }));

  push('};', { kind: 'kw' });
  push(`// → ${state.decision.kind} "${state.decision.reason}"  (severity: ${state.decision.severity})`,
       { kind: 'comment' });

  return { lines };
}

function serializeNode(node, depth) {
  if (node.kind === 'leaf') return [{ text: leafToCedar(node), guardId: node.id, custom: node.custom }];

  const joiner = node.op === 'AND' ? '&&' : '||';
  const segs = [];
  node.children.forEach((c, i) => {
    const sub = serializeNode(c, depth + 1);
    // Wrap nested containers in parens
    if (c.kind === 'container' && c.children.length > 1) {
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
  const path = leaf.custom
    ? `context.custom.${leaf.sigId}`
    : `context.${leaf.sigId}`;
  const op = leaf.operator;
  let val;
  if (leaf.value.kind === 'ref')       val = leaf.value.text.replace(/^root\./, 'context.');
  else if (leaf.value.kind === 'enum') val = `"${leaf.value.text}"`;
  else if (leaf.value.kind === 'bool') val = leaf.value.text;
  else if (leaf.value.kind === 'num')  val = leaf.value.text;
  else val = `"${leaf.value.text}"`;

  const note = leaf.note ? `  // ${leaf.note}` : '';
  if (leaf.custom) {
    return `(context.custom has ${leaf.sigId} && ${path} ${op} ${val})${note}`;
  }
  return `${path} ${op} ${val}${note}`;
}

// ───────────────────────────── Evaluator ────────────────────────────────────
// Given a tx fixture, walks the IR and returns { matchedLeafIds, decision }.

function v2Evaluate(state, tx) {
  const matched = [];
  const triggered = evalNode(state.root, tx, matched);
  return {
    matchedLeafIds: matched,
    decision: triggered ? state.decision : { kind: 'Allow', reason: '정책에 일치하는 가드 없음', severity: 'PASS' },
    triggered,
  };
}

function evalNode(node, tx, matched) {
  if (node.kind === 'leaf') {
    const ok = evalLeaf(node, tx);
    if (ok) matched.push(node.id);
    return ok;
  }
  if (node.op === 'AND') return node.children.every(c => evalNode(c, tx, matched));
  return node.children.some(c => evalNode(c, tx, matched));
}

function evalLeaf(leaf, tx) {
  // Look up tx field. Map IR sigId → tx field name (mostly identical).
  const fieldMap = {
    recipient: 'recipient', swapMode: 'swapMode', feeBps: 'feeBps',
    inputAmount: 'inputAmount', outputAmount: 'outputAmount',
    validityExpiresAt: 'validityExpiresAt', validityDeltaSec: 'validityDeltaSec',
    recipientIsContract: 'recipientIsContract',
    effectiveRateVsOracleBps: 'effectiveRateVsOracleBps',
    totalInputUsd: 'totalInputUsd',
    totalInputFractionOfPortfolioBps: 'totalInputFractionOfPortfolioBps',
  };
  const fieldName = fieldMap[leaf.sigId];
  let lhs = tx[fieldName];
  // Custom signal might be missing → absence handling
  if (leaf.custom && (lhs === undefined || lhs === null)) {
    return leaf.absence === 'true';
  }
  // Reference values like root.from
  if (leaf.value.kind === 'ref') {
    const refField = leaf.value.text.replace(/^root\./, '');
    const rhs = tx[refField];
    return cmp(leaf.operator, lhs, rhs);
  }
  if (leaf.value.kind === 'enum') return cmp(leaf.operator, String(lhs), leaf.value.text);
  if (leaf.value.kind === 'bool') return cmp(leaf.operator, !!lhs, leaf.value.text === 'true');
  if (leaf.value.kind === 'num')  return cmp(leaf.operator, Number(lhs), Number(leaf.value.text));
  return false;
}

function cmp(op, a, b) {
  switch (op) {
    case '==': return a === b;
    case '!=': return a !== b;
    case '<':  return a <  b;
    case '<=': return a <= b;
    case '>':  return a >  b;
    case '>=': return a >= b;
    case 'in': return Array.isArray(b) ? b.includes(a) : false;
    default:   return false;
  }
}

// ───────────────────────────── Test fixtures ────────────────────────────────
// Numeric versions so the evaluator can compute real verdicts.

const V2_TEST_FIXTURES = [
  {
    id: 'fx1', label: 'Calm swap · USDC→WETH',
    tx: { from: '0xA1c4...7e29', recipient: '0xA1c4...7e29', swapMode: 'limit',
          inputAmount: 0.5, outputAmount: 1512, feeBps: 30,
          validityDeltaSec: 612, recipientIsContract: false,
          effectiveRateVsOracleBps: 22, totalInputUsd: 900,
          totalInputFractionOfPortfolioBps: 90,
          inputAmountDisplay: '180 USDC', outputAmountDisplay: '0.06 WETH' },
  },
  {
    id: 'fx2', label: 'Market swap · 만료 임박',
    tx: { from: '0xA1c4...7e29', recipient: '0xA1c4...7e29', swapMode: 'market',
          inputAmount: 0.5, outputAmount: 1512, feeBps: 60,
          validityDeltaSec: 18, recipientIsContract: false,
          effectiveRateVsOracleBps: 145, totalInputUsd: 4800,
          totalInputFractionOfPortfolioBps: 420,
          inputAmountDisplay: '0.5 WETH', outputAmountDisplay: '1,512 USDC' },
  },
  {
    id: 'fx3', label: 'Send to contract',
    tx: { from: '0xA1c4...7e29', recipient: '0x8c2f...4910', swapMode: 'limit',
          inputAmount: 1000, outputAmount: 0.34, feeBps: 25,
          validityDeltaSec: 480, recipientIsContract: true,
          effectiveRateVsOracleBps: 18, totalInputUsd: 1000,
          totalInputFractionOfPortfolioBps: 100,
          inputAmountDisplay: '1,000 USDC', outputAmountDisplay: '0.34 WETH' },
  },
];

Object.assign(window, {
  V2_SIGNAL_CATALOG, V2_OPERATORS_BY_TYPE, V2_COLOR_MAP, v2ColorFor, v2SigById,
  v2BuildBaseline, v2BuildFlatAND, v2IsFlatAND, v2HasOR,
  v2Reduce, v2ToCedar, v2Evaluate, v2Id, V2_TEST_FIXTURES,
});
