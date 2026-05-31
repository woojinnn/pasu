// monitoring-l1.jsx — L1 multi-wallet portfolio (landing). Assets ⇄ Risk lens over the same rows.

const { useMemo: useML1 } = React;

/* big value with 3-state (filled / loading / empty) */
function BigVal({ value, cls }) {
  const s = ds();
  if (s === 'loading') return <span className="sc-v skeleton"></span>;
  if (s === 'p0') return <span className="sc-v none"><L ko="연결 전" en="not connected" /></span>;
  return <span className={`sc-v ${cls || ''}`}>{fmtUsd(value)}</span>;
}

function failCountFor(walletId) {
  return FINDINGS.filter(f => f.verdict === 'fail' && (walletId === 'all' || f.wallet === walletId)).length;
}

/* ── summary header ── */
function SummaryBar({ sel }) {
  const w = sel === 'all' ? null : WALLETS.find(x => x.id === sel);
  const totalUsd = w ? w.totalUsd : SUMMARY.totalUsd;
  const varUsd = w ? w.varUsd : SUMMARY.totalVarUsd;
  const fails = failCountFor(sel);
  const unl = w ? w.unlimited : SUMMARY.unlimitedCount;
  return (
    <div className="summary-bar">
      <div className="sum-cell">
        <span className="sc-k"><L ko="총 자산" en="Total assets" /></span>
        <BigVal value={totalUsd} />
        <span className="sc-sub">{sel === 'all' ? <L ko="4개 지갑 합산 · 5개 체인" en="4 wallets · 5 chains" /> : <L ko="이 지갑 평가액" en="this wallet" />}</span>
      </div>
      <div className="sum-cell var">
        <span className="sc-k"><L ko="총 VaR · 현재 노출액" en="Total VaR · current exposure" /></span>
        <BigVal value={varUsd} cls="" />
        <span className="sc-sub"><L ko="무제한 승인은 금액 합산 제외" en="unlimited approvals excluded from $ sum" /></span>
      </div>
      <div className="sum-cell risk">
        <span className="sc-k"><L ko="위험 신호" en="Risk signals" /></span>
        <div className="risk-chips">
          <span className="risk-chip fail"><span className="rc-dot"></span>FAIL <b>{fails}</b></span>
          <span className="risk-chip unl"><span className="rc-dot"></span><L ko="무제한" en="Unlimited" /> <b>{unl}</b></span>
        </div>
      </div>
    </div>
  );
}

/* ── wallet switcher ── */
function WalletSwitch({ sel, setSel }) {
  return (
    <div className="wallet-switch" role="group" aria-label="wallet switcher">
      <button className={`ws-chip ${sel === 'all' ? 'on' : ''}`} onClick={() => setSel('all')}>
        <L ko="전체 합산" en="All wallets" />
        <span className="ws-amt">{WALLETS.length}</span>
      </button>
      {WALLETS.map(w => (
        <button key={w.id} className={`ws-chip ${sel === w.id ? 'on' : ''}`} onClick={() => setSel(w.id)}>
          <span className={`ws-dot ${w.status}`}></span>
          <L {...w.name} />
          <span className="ws-amt">{w.addr}</span>
        </button>
      ))}
    </div>
  );
}

/* ── lens toggle ── */
function LensToggle({ lens, setLens }) {
  return (
    <div className="lens-toggle" role="tablist" aria-label="lens">
      <button role="tab" aria-selected={lens === 'assets'} className={`lens-btn ${lens === 'assets' ? 'on' : ''}`} onClick={() => setLens('assets')}>
        <MI.wallet /><L ko="자산 보기" en="Assets" />
      </button>
      <button role="tab" aria-selected={lens === 'risk'} className={`lens-btn ${lens === 'risk' ? 'on risk-on' : ''}`} onClick={() => setLens('risk')}>
        <MI.shield /><L ko="위험 보기" en="Risk" />
      </button>
    </div>
  );
}

/* ── chain breakdown ── */
function ChainBreakdown({ sel }) {
  const rows = useML1(() => {
    if (sel === 'all') return CHAIN_BREAKDOWN;
    const hs = HOLDINGS.filter(h => h.wallet === sel);
    const total = hs.reduce((s, h) => s + h.balanceUsd, 0) || 1;
    const map = {};
    hs.forEach(h => { map[h.chain] = (map[h.chain] || 0) + h.balanceUsd; });
    return Object.entries(map).map(([chain, usd]) => ({ chain, usd, pct: +(usd / total * 100).toFixed(1) }))
      .sort((a, b) => b.usd - a.usd);
  }, [sel]);

  if (ds() !== 'full') {
    return (
      <div className="chain-card empty-state">
        <MI.scan />
        {ds() === 'loading' ? <L ko="체인별 분포 산정 중…" en="Computing chain breakdown…" /> : <L ko="잔고 연결 전 · 체인 분포 대기 (P1)" en="Balances not connected · chain breakdown pending (P1)" />}
      </div>
    );
  }
  return (
    <div className="chain-card">
      <div className="cc-head">
        <span className="cc-ttl"><L ko="체인별 분포" en="Chain breakdown" /></span>
        <span className="cc-meta">{rows.length} chains</span>
      </div>
      <div className="chain-bar">
        {rows.map(r => <div key={r.chain} className="chain-seg" style={{ width: r.pct + '%', background: chainOf(r.chain).color }} title={chainOf(r.chain).name}></div>)}
      </div>
      <div className="chain-legend">
        {rows.map(r => (
          <span key={r.chain} className="chain-leg">
            <span className="cl-dot" style={{ background: chainOf(r.chain).color }}></span>
            <span className="cl-name">{chainOf(r.chain).name}</span>
            <span className="cl-pct">{fmtUsd(r.usd)} · {r.pct}%</span>
          </span>
        ))}
      </div>
    </div>
  );
}

/* risk class of a holding */
function riskClassOf(h) {
  if (h.risk.some(r => r.kind === 'fail')) return 'fail';
  if (h.risk.some(r => r.kind === 'unlimited' || r.kind === 'warn')) return 'warn';
  return 'safe';
}
const RANK = { fail: 0, warn: 1, safe: 2 };

/* ── holdings table ── */
function Holdings({ sel, lens, onDrill }) {
  const rows = useML1(() => {
    let hs = HOLDINGS.filter(h => sel === 'all' || h.wallet === sel);
    if (lens === 'risk') {
      hs = [...hs].sort((a, b) => {
        const ra = RANK[riskClassOf(a)], rb = RANK[riskClassOf(b)];
        if (ra !== rb) return ra - rb;
        // unlimited / no-price floats up even at $0 VaR
        return (b.varUsd ?? 1e12 * (riskClassOf(b) !== 'safe')) - (a.varUsd ?? 1e12 * (riskClassOf(a) !== 'safe'));
      });
    } else {
      hs = [...hs].sort((a, b) => b.balanceUsd - a.balanceUsd);
    }
    return hs;
  }, [sel, lens]);

  if (ds() === 'p0') {
    return (
      <div className="holdings">
        <div className="hold-empty">
          <div className="he-ic"><MI.scan /></div>
          <div className="he-t"><L ko="잔고 · 승인 스캔 대기" en="Balance & approval scan pending" /></div>
          <div className="he-d"><L ko="자산 잔고(P1)와 승인 인덱싱(P2)이 아직 연결되지 않았어요. SDK Findings는 살아있으니 지갑을 선택해 상세에서 탐지 이벤트를 확인하세요." en="Balances (P1) and approval indexing (P2) aren't connected yet. SDK Findings are live — open a wallet to inspect detections in detail." /></div>
          <span className="he-phase">P1 · P2 pending</span>
        </div>
      </div>
    );
  }

  return (
    <div className="holdings">
      <div className="hold-head">
        <span className="hh-t"><L ko="자산 · 포지션" en="Holdings & positions" /><span className="hh-n">{rows.length}</span></span>
        <span className="hh-sort">{lens === 'risk' ? <L ko="정렬 · 위험 우선순위" en="sorted · risk priority" /> : <L ko="정렬 · 평가액순" en="sorted · value" />}</span>
      </div>
      <div className="htable">
        <div className="hrow colhead">
          <span><L ko="자산" en="Asset" /></span>
          <span><L ko="체인" en="Chain" /></span>
          <span><L ko="잔고" en="Balance" /></span>
          <span><L ko="위험 오버레이" en="Risk overlay" /></span>
          <span className="col-r"><L ko="VaR" en="VaR" /></span>
          <span></span>
        </div>
        {rows.map(h => {
          const rc = riskClassOf(h);
          return (
            <div key={h.id} className={`hrow ${rc === 'fail' ? 'risk-fail' : rc === 'warn' ? 'risk-warn' : 'safe'}`}
              role="button" tabIndex={0} onClick={() => onDrill(h.wallet)}
              onKeyDown={(e) => { if (e.key === 'Enter') onDrill(h.wallet); }}>
              <div className="h-asset">
                <span className={`ha-ic ${h.kind === 'nft' ? 'nft' : h.kind === 'native' ? 'native' : ''}`}>{h.asset.slice(0, 3).toUpperCase()}</span>
                <div className="ha-txt">
                  <div className="ha-sym">{h.asset}{(h.kind === 'nft' || h.kind === 'position') && <span className="ha-kind">{h.kind}</span>}</div>
                  <div className="ha-nm">{h.name}</div>
                </div>
              </div>
              <ChainPill id={h.chain} />
              <div className="h-bal">
                <div className="hb-amt">{h.balance}</div>
                <div className={`hb-usd ${h.balanceUsd ? '' : 'none'}`}>{ds() === 'loading' ? '· · ·' : (h.balanceUsd ? fmtUsd(h.balanceUsd) : h.priceNote)}</div>
              </div>
              <div className="h-risk">
                {h.risk.length === 0
                  ? <span className="r-safe"><MI.check /><L ko="노출 없음" en="No exposure" /></span>
                  : h.risk.map((r, i) => <RiskBadge key={i} r={r} />)}
              </div>
              <div className="h-var col-r"><VarCell value={h.varUsd} risk={rc !== 'safe'} /></div>
              <MI.caretR className="h-chev" />
            </div>
          );
        })}
      </div>
    </div>
  );
}

/* ── L1 view ── */
function L1View({ sel, setSel, lens, setLens, onDrill, suggestOpen, setSuggestOpen }) {
  const fails = failCountFor(sel);
  return (
    <>
      <SummaryBar sel={sel} />
      <div className="ctrl-row">
        <WalletSwitch sel={sel} setSel={setSel} />
        <span className="spacer"></span>
        <LensToggle lens={lens} setLens={setLens} />
      </div>

      {lens === 'assets' && fails > 0 && suggestOpen && (
        <div className="risk-suggest">
          <span className="rs-ic"><MI.alert /></span>
          <span className="rs-txt"><L ko={<><b>FAIL {fails}건</b>이 감지됐어요. 위험 보기로 전환하면 노출된 자산이 상단으로 떠오릅니다.</>} en={<><b>{fails} FAIL{fails > 1 ? 's' : ''}</b> detected. Switch to the Risk lens to float exposed assets to the top.</>} /></span>
          <button className="rs-act" onClick={() => setLens('risk')}><MI.shield /><L ko="위험 보기로 전환" en="Switch to Risk" /></button>
          <button className="rs-dismiss" onClick={() => setSuggestOpen(false)} aria-label="dismiss"><MI.x /></button>
        </div>
      )}

      <ChainBreakdown sel={sel} />
      <Holdings sel={sel} lens={lens} onDrill={onDrill} />
    </>
  );
}

Object.assign(window, { L1View });
