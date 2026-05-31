// editor-policy.js
// Shared policy data + signal catalog for all three Editor modes.
// All three modes (Block / Builder / Code) read from the same IR.

// ─── Signal catalog ──────────────────────────────────────────────────────────
// Base = calldata. Custom = manifest enrichment (rendered dashed).
// Some custom signals are *reparented* into base cascade slots (§9.2-bis).

const SIGNAL_CATALOG = {
  // Top-level (6 base + 9 custom = 15 root signals)
  base: [
    { id: 'inputToken',  label: { ko: 'Input',  en: 'Input'  }, kind: 'group',
      cascade: ['asset', 'amount'] },
    { id: 'outputToken', label: { ko: 'Output', en: 'Output' }, kind: 'group',
      cascade: ['asset', 'amount'] },
    { id: 'recipient',   label: { ko: 'Recipient', en: 'Recipient' }, kind: 'leaf',
      leafType: 'address', shape: 'rect' },
    { id: 'swapMode',    label: { ko: 'Swap direction', en: 'Swap direction' }, kind: 'leaf',
      leafType: 'enum', shape: 'hexagon', options: ['market', 'limit'] },
    { id: 'feeBps',      label: { ko: 'Fee (bps)', en: 'Fee (bps)' }, kind: 'leaf',
      leafType: 'bps', shape: 'pill', optional: true },
    { id: 'validity',    label: { ko: 'Validity', en: 'Validity' }, kind: 'group',
      cascade: ['expiresAt', 'source'] },
  ],
  custom: [
    { id: 'inputAmountNano',  label: { ko: 'Input amount',  en: 'Input amount'  },
      reparent: 'inputToken/amount/value', leafType: 'tokenNative', shape: 'pill' },
    { id: 'outputAmountNano', label: { ko: 'Output amount', en: 'Output amount' },
      reparent: 'outputToken/amount/value', leafType: 'tokenNative', shape: 'pill' },
    { id: 'recipientIsContract', label: { ko: '수신자가 컨트랙트', en: 'Recipient is contract' },
      leafType: 'boolean', shape: 'hexagon' },
    { id: 'effectiveRateVsOracleBps', label: { ko: 'Oracle 대비 슬리피지', en: 'Slippage vs oracle' },
      leafType: 'bps', shape: 'pill' },
    { id: 'validityDeltaSec', label: { ko: '만료까지 (sec)', en: 'Time to deadline' },
      leafType: 'seconds', shape: 'pill' },
    { id: 'totalInputUsd', label: { ko: '입력 가치 (USD)', en: 'Input value (USD)' },
      leafType: 'usd', shape: 'pill' },
    { id: 'totalMinOutputUsd', label: { ko: '최소 출력 (USD)', en: 'Min output (USD)' },
      leafType: 'usd', shape: 'pill' },
    { id: 'totalInputFractionOfPortfolioBps', label: { ko: '입력 ÷ 포트폴리오', en: 'Input ÷ portfolio' },
      leafType: 'bps', shape: 'pill' },
    { id: 'windowStats', label: { ko: '24h 통계', en: '24h stats' }, kind: 'group',
      cascade: ['volume', 'count'] },
  ],
};

// Cascade leaf definitions (Input / Output / Validity / windowStats).
// `path` is the full segment chain. `custom: true` means dashed border in palette.
const CASCADE_LEAVES = {
  // Input → Token (base) / Amount → Value (CUSTOM, reparented inputAmountNano)
  inputToken: [
    { path: 'inputToken.asset.address',  label: 'Token · Address',  leafType: 'address', shape: 'rect', custom: false },
    { path: 'inputToken.asset.symbol',   label: 'Token · Symbol',   leafType: 'symbol',  shape: 'rect', custom: false },
    { path: 'inputToken.asset.decimals', label: 'Token · Decimals', leafType: 'number',  shape: 'pill', custom: false },
    { path: 'inputToken.amount.value',   label: 'Amount · Value',   leafType: 'tokenNative', shape: 'pill', custom: true,
      reparentedFrom: 'inputAmountNano' },
  ],
  outputToken: [
    { path: 'outputToken.asset.address',  label: 'Token · Address',  leafType: 'address', shape: 'rect', custom: false },
    { path: 'outputToken.asset.symbol',   label: 'Token · Symbol',   leafType: 'symbol',  shape: 'rect', custom: false },
    { path: 'outputToken.asset.decimals', label: 'Token · Decimals', leafType: 'number',  shape: 'pill', custom: false },
    { path: 'outputToken.amount.value',   label: 'Amount · Value',   leafType: 'tokenNative', shape: 'pill', custom: true,
      reparentedFrom: 'outputAmountNano' },
  ],
  validity: [
    { path: 'validity.expiresAt', label: 'Deadline',          leafType: 'unix',  shape: 'rect', custom: false },
    { path: 'validity.source',    label: 'Source',            leafType: 'enum',  shape: 'hexagon', custom: false },
  ],
  windowStats: [
    { path: 'windowStats.volume', label: 'Volume', leafType: 'usd',    shape: 'pill', custom: true },
    { path: 'windowStats.count',  label: 'Count',  leafType: 'number', shape: 'pill', custom: true },
  ],
};

// ─── Color domain schemes ────────────────────────────────────────────────────
// Tweakable: cycle three reasonable assignments of 6 domains → 3 base colors.
// All schemes use ONLY Sage / Slate / Cyan (no status colors on blocks).

const COLOR_SCHEMES = {
  'io-balanced': {
    label: 'I/O 균형',
    desc: '입력=Sage · 출력·트랜잭션 파라미터=Slate · 수신자·메타=Cyan',
    map: {
      inputToken: 'sage', inputAmountNano: 'sage',
      outputToken: 'slate', outputAmountNano: 'slate',
      swapMode: 'slate', feeBps: 'slate', validity: 'slate', validityDeltaSec: 'slate', effectiveRateVsOracleBps: 'slate',
      recipient: 'cyan', recipientIsContract: 'cyan',
      totalInputUsd: 'cyan', totalMinOutputUsd: 'cyan', totalInputFractionOfPortfolioBps: 'cyan', windowStats: 'cyan',
    },
  },
  'calldata-vs-meta': {
    label: 'Calldata vs Meta',
    desc: 'calldata(I/O/Recipient)=Sage · 트랜잭션 제어=Slate · 모든 enrichment=Cyan',
    map: {
      inputToken: 'sage', inputAmountNano: 'sage', outputToken: 'sage', outputAmountNano: 'sage', recipient: 'sage',
      swapMode: 'slate', feeBps: 'slate', validity: 'slate',
      validityDeltaSec: 'cyan', effectiveRateVsOracleBps: 'cyan', recipientIsContract: 'cyan',
      totalInputUsd: 'cyan', totalMinOutputUsd: 'cyan', totalInputFractionOfPortfolioBps: 'cyan', windowStats: 'cyan',
    },
  },
  'asset-flow': {
    label: 'Asset → Destination',
    desc: '자산(I/O)=Sage · 수신자=Slate · 정책 파라미터·메타=Cyan',
    map: {
      inputToken: 'sage', inputAmountNano: 'sage', outputToken: 'sage', outputAmountNano: 'sage',
      recipient: 'slate', recipientIsContract: 'slate',
      swapMode: 'cyan', feeBps: 'cyan', validity: 'cyan', validityDeltaSec: 'cyan', effectiveRateVsOracleBps: 'cyan',
      totalInputUsd: 'cyan', totalMinOutputUsd: 'cyan', totalInputFractionOfPortfolioBps: 'cyan', windowStats: 'cyan',
    },
  },
};

// ─── Baseline policy (4 OR guards, §6.6) ─────────────────────────────────────
// This is the *swap baseline* shown in all three modes.
// Tree IR: root = OR group containing 4 leaf conditions.
// Each condition has a `path` (full cascade), operator, value, and optional
// `note` (user-authored 근거 라벨 — neutral slate pill, not status).

const BASELINE_POLICY = {
  id: 'swap-baseline-v3',
  action: 'swap',
  manifestHash: '#fc20a91',
  signalCounts: { base: 6, custom: 9 },
  decision: { kind: 'Deny', reason: 'swap baseline violated', severity: 'FAIL' },
  root: {
    op: 'OR',
    id: 'or-root',
    children: [
      {
        id: 'g1',
        path: 'recipient',
        segments: [{ key: 'recipient', label: 'Recipient' }],
        operator: '!=',
        value: { kind: 'ref', text: 'root.from' },
        note: 'swap-and-send 차단',
        custom: false,
      },
      {
        id: 'g2',
        path: 'swapMode',
        segments: [{ key: 'swapMode', label: 'Swap direction' }],
        operator: '==',
        value: { kind: 'enum', text: 'market' },
        note: '무방어 시장가 차단',
        custom: false,
      },
      {
        id: 'g3',
        path: 'validityDeltaSec',
        segments: [{ key: 'validityDeltaSec', label: '만료까지 (sec)' }],
        operator: '<',
        value: { kind: 'num', text: '30', unit: 'sec' },
        note: '만료 임박',
        custom: true,
        absence: 'false',  // dashed → user-selectable absence handling
      },
      {
        id: 'g4',
        path: 'recipientIsContract',
        segments: [{ key: 'recipientIsContract', label: '수신자가 컨트랙트' }],
        operator: '==',
        value: { kind: 'bool', text: 'true' },
        note: '컨트랙트 수신자',
        custom: true,
        absence: 'false',
      },
    ],
  },
};

// ─── Test-pane fixtures ──────────────────────────────────────────────────────
// Realistic-looking tx fixtures that exercise the OR branches.

const TEST_FIXTURES = [
  {
    id: 'fx1', label: 'Calm swap · USDC→WETH',
    tx: { from: '0xA1c4...7e29', recipient: '0xA1c4...7e29', swapMode: 'limit',
          inputAmount: '180 USDC', outputAmount: '0.06 WETH',
          validityDeltaSec: 612, recipientIsContract: false },
    matches: [],   // → policy NOT triggered → tx allowed
  },
  {
    id: 'fx2', label: 'Market swap · 만료 임박',
    tx: { from: '0xA1c4...7e29', recipient: '0xA1c4...7e29', swapMode: 'market',
          inputAmount: '0.5 WETH', outputAmount: '1,512 USDC',
          validityDeltaSec: 18, recipientIsContract: false },
    matches: ['g2', 'g3'],  // → baseline violated → DENY
  },
  {
    id: 'fx3', label: 'Send to contract',
    tx: { from: '0xA1c4...7e29', recipient: '0x8c2f...4910', swapMode: 'limit',
          inputAmount: '1,000 USDC', outputAmount: '0.34 WETH',
          validityDeltaSec: 480, recipientIsContract: true },
    matches: ['g1', 'g4'],  // → baseline violated → DENY
  },
];

// ─── Cedar text (the canonical Code-mode rendering) ──────────────────────────
// We hand-author this once so Code mode has a real, parseable-looking string.
// Line metadata lets us highlight per-guard rows in sync with other modes.

const CEDAR_TEXT = {
  lines: [
    { n: 1,  text: '// Swap baseline · manifest #fc20a91',                       kind: 'comment' },
    { n: 2,  text: 'forbid (',                                                   kind: 'kw' },
    { n: 3,  text: '  principal,',                                               kind: 'arg' },
    { n: 4,  text: '  action == Action::"swap",',                                kind: 'arg' },
    { n: 5,  text: '  resource',                                                 kind: 'arg' },
    { n: 6,  text: ')',                                                          kind: 'punct' },
    { n: 7,  text: 'when {',                                                     kind: 'kw' },
    { n: 8,  text: '     context.recipient != context.from              // swap-and-send 차단', kind: 'guard', guardId: 'g1' },
    { n: 9,  text: '  || context.swapMode == "market"                   // 무방어 시장가 차단', kind: 'guard', guardId: 'g2' },
    { n: 10, text: '  || (context.custom has validityDeltaSec',                  kind: 'guard', guardId: 'g3', custom: true },
    { n: 11, text: '      && context.custom.validityDeltaSec < 30)      // 만료 임박',         kind: 'guard', guardId: 'g3', custom: true },
    { n: 12, text: '  || (context.custom has recipientIsContract',               kind: 'guard', guardId: 'g4', custom: true },
    { n: 13, text: '      && context.custom.recipientIsContract == true)// 컨트랙트 수신자',   kind: 'guard', guardId: 'g4', custom: true },
    { n: 14, text: '};',                                                         kind: 'kw' },
    { n: 15, text: '// → Deny "swap baseline violated"  (severity: FAIL)',       kind: 'comment' },
  ],
};

// Default operators per leaf type — used by Builder to populate the operator dropdown.
const OPERATORS_BY_TYPE = {
  address:     ['==', '!='],
  symbol:      ['==', '!=', 'in'],
  enum:        ['==', '!=', 'in'],
  boolean:     ['==', '!='],
  bps:         ['<', '<=', '>', '>=', '==', '!='],
  seconds:     ['<', '<=', '>', '>=', '=='],
  usd:         ['<', '<=', '>', '>=', '=='],
  tokenNative: ['<', '<=', '>', '>=', '==', '!='],
  number:      ['<', '<=', '>', '>=', '==', '!='],
  unix:        ['<', '<=', '>', '>='],
};

Object.assign(window, {
  SIGNAL_CATALOG, CASCADE_LEAVES, COLOR_SCHEMES, BASELINE_POLICY,
  TEST_FIXTURES, CEDAR_TEXT, OPERATORS_BY_TYPE,
});
