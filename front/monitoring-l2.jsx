// monitoring-l2.jsx — L2 single-wallet drilldown: action queue (triage) → findings feed → approvals table.

const { useState: useSL2, useMemo: useML2 } = React;

/* verdict pill */
function VPill({ v, sm }) {
  const map = { fail: { ko: 'FAIL', en: 'Blocked' }, warn: { ko: 'WARN', en: 'Review' }, pass: { ko: 'PASS', en: 'Passed' } };
  return <span className={`vpill ${v} ${sm ? 'sm' : ''}`}><span className="vp-dot"></span><L {...map[v]} /></span>;
}
/* type tag — detection vs approval (brief §3.2: prevent character confusion) */
function TypeTag({ kind }) {
  if (kind === 'approval') return <span className="type-tag approval"><MI.infinity /><L ko="승인" en="Approval" /></span>;
  return <span className="type-tag detection"><MI.scan /><L ko="탐지" en="Detection" /></span>;
}

/* ── L2 header band ── */
function L2Header({ wallet }) {
  const fails = FINDINGS.filter(f => f.wallet === wallet.id && f.verdict === 'fail').length;
  const warns = FINDINGS.filter(f => f.wallet === wallet.id && f.verdict === 'warn').length;
  return (
    <div className="l2-head">
      <div className="l2h-status">
        {fails > 0 && <span className="l2-chip fail"><span className="lc-dot"></span>FAIL <b>{fails}</b></span>}
        {warns > 0 && <span className="l2-chip warn"><span className="lc-dot"></span>WARN <b>{warns}</b></span>}
        {fails === 0 && warns === 0 && <span className="l2-chip calm"><span className="lc-dot"></span><L ko="정상" en="Calm" /></span>}
        {wallet.pending > 0 && <span className="l2-pending">{wallet.pending} <L ko="처리 대기" en="pending" /></span>}
      </div>
      <div className="l2h-metrics">
        <div className="l2m">
          <span className="l2m-k"><L ko="총 VaR · 현재 노출" en="Total VaR · exposure" /></span>
          <span className="l2m-v">{ds() === 'full' ? fmtUsd(wallet.varUsd) : ds() === 'loading' ? '···' : '—'}</span>
        </div>
        <div className="l2m">
          <span className="l2m-k"><L ko="무제한 승인 · 잠재 노출" en="Unlimited · potential" /></span>
          <span className="l2m-v unl">{wallet.unlimited}<L ko="건" en="" /></span>
        </div>
      </div>
    </div>
  );
}

/* ── Action queue (triage) ── */
function ActionQueue({ wallet, onRevoke, handled, onHandle }) {
  const items = useML2(() => {
    const det = FINDINGS.filter(f => f.wallet === wallet.id && (f.verdict === 'fail' || f.verdict === 'warn'))
      .map(f => ({ kind: 'detection', id: f.id, verdict: f.verdict, data: f, score: f.verdict === 'fail' ? 0 : 1 }));
    const ap = (ds() === 'full' ? APPROVALS.filter(a => a.wallet === wallet.id && (a.risk === 'fail' || a.risk === 'warn'))
      .map(a => ({ kind: 'approval', id: a.id, verdict: a.risk, data: a, score: a.risk === 'fail' ? 0 : 1, var: a.varUsd ?? 0 })) : []);
    return [...det, ...ap].sort((a, b) => a.score - b.score || (b.var || 0) - (a.var || 0));
  }, [wallet.id, ds()]);

  const live = items.filter(it => !handled[it.id]);

  return (
    <section className="l2-section">
      <div className="l2-sec-head">
        <span className="ls-t"><MI.alert /><L ko="긴급 항목 큐" en="Action queue" /><span className="ls-n">{live.length}</span></span>
        <span className="ls-meta"><L ko="우선순위순 · 처리하면 사라지고 감사 로그에 남습니다" en="priority order · resolved items leave an audit trail" /></span>
      </div>
      {live.length === 0 ? (
        <div className="aq-empty"><MI.check /><L ko="처리할 긴급 항목이 없어요" en="No urgent items to triage" /></div>
      ) : (
        <div className="aq-list">
          {live.map(it => <QueueRow key={it.id} it={it} onRevoke={onRevoke} onHandle={onHandle} />)}
        </div>
      )}
    </section>
  );
}

function QueueRow({ it, onRevoke, onHandle }) {
  const isAp = it.kind === 'approval';
  return (
    <article className={`aq-row ${it.verdict}`}>
      <div className="aq-l"><TypeTag kind={it.kind} /><VPill v={it.verdict} sm /></div>
      <div className="aq-main">
        {isAp ? (
          <>
            <div className="aq-title"><b>{it.data.asset}</b> · {it.data.allowance} · <span className="mono">{it.data.type}</span></div>
            <div className="aq-sub"><L ko="승인 대상" en="spender" /> <Spender id={it.data.spender} compact /></div>
          </>
        ) : (
          <>
            <div className="aq-title"><b><L {...it.data.title} /></b> · <span className="mono">{it.data.value}</span></div>
            <div className="aq-sub"><span className="mono">{it.data.rule}</span> · {it.data.source} · {it.data.ts} ago</div>
          </>
        )}
      </div>
      <div className="aq-act">
        {isAp ? (
          <>
            <button className="btn revoke" onClick={() => onRevoke(it.data)}><MI.revoke /><L ko="Revoke 하기" en="Revoke" /><MI.caretR style={{ width: 12, height: 12 }} /></button>
            <button className="btn ghost sm" onClick={() => onHandle(it.id)}><L ko="한도 조정" en="Adjust" /></button>
          </>
        ) : (
          <>
            <button className="btn ghost sm" onClick={() => onHandle(it.id)}><L ko="임시 허용" en="Allow once" /></button>
            <button className="btn ghost sm" onClick={() => onHandle(it.id)}><L ko="무시 · 유지" en="Dismiss" /></button>
            <a className="btn ghost sm" href="History.html"><L ko="검토" en="Review" /><MI.ext style={{ width: 11, height: 11 }} /></a>
          </>
        )}
      </div>
    </article>
  );
}

/* ── Findings feed ── */
function FindingsFeed({ wallet }) {
  const [open, setOpen] = useSL2(null);
  const rows = FINDINGS.filter(f => f.wallet === wallet.id);
  return (
    <section className="l2-section">
      <div className="l2-sec-head">
        <span className="ls-t"><MI.scan /><L ko="Findings 피드" en="Findings feed" /><span className="ls-n">{rows.length}</span></span>
        <span className="ls-meta"><L ko="모든 탐지 이벤트 · 시간순 · P0 실데이터" en="all detections · newest first · P0 live" /></span>
      </div>
      <div className="ff-list">
        {rows.map(f => (
          <article key={f.id} className={`ff-row ${f.verdict} ${open === f.id ? 'open' : ''}`}>
            <header className="ff-head" role="button" tabIndex={0} onClick={() => setOpen(o => o === f.id ? null : f.id)}>
              <VPill v={f.verdict} sm />
              <span className="ff-title"><b><L {...f.title} /></b> · <span className="mono">{f.value}</span></span>
              <span className="ff-age mono">{f.ts} ago</span>
              <MI.caretR className="ff-chev" />
            </header>
            <div className="ff-detail"><div className="ff-detail-in">
              {open === f.id && (
                <dl className="ff-props">
                  <dt>method</dt><dd className="mono">{f.method}</dd>
                  <dt>to</dt><dd className="mono">{f.to}</dd>
                  <dt>value</dt><dd className="mono">{f.value}</dd>
                  <dt>matched</dt><dd><span className={`mono tag-${f.verdict}`}>{f.rule}</span></dd>
                  <dt>source</dt><dd>{f.source}</dd>
                </dl>
              )}
            </div></div>
          </article>
        ))}
      </div>
    </section>
  );
}

/* ── Approvals table ── */
function ApprovalsTable({ wallet, onRevoke }) {
  const rows = useML2(() => APPROVALS.filter(a => a.wallet === wallet.id)
    .sort((a, b) => { const r = { fail: 0, warn: 1, none: 2 }; return r[a.risk] - r[b.risk] || (b.varUsd || 0) - (a.varUsd || 0); }), [wallet.id]);

  if (ds() === 'p0') {
    return (
      <section className="l2-section">
        <div className="l2-sec-head"><span className="ls-t"><MI.infinity /><L ko="Approvals" en="Approvals" /></span></div>
        <div className="ap-empty">
          <div className="he-ic"><MI.scan /></div>
          <div className="he-t"><L ko="승인 스캔 대기" en="Approval scan pending" /></div>
          <div className="he-d"><L ko="allowance 인덱싱(P2)이 연결되면 revoke 대상 승인이 여기에 VaR순으로 나열됩니다." en="Once allowance indexing (P2) is connected, revoke targets list here by VaR." /></div>
          <span className="he-phase">P2 pending</span>
        </div>
      </section>
    );
  }

  return (
    <section className="l2-section">
      <div className="l2-sec-head">
        <span className="ls-t"><MI.infinity /><L ko="Approvals" en="Approvals" /><span className="ls-n">{rows.length}</span></span>
        <span className="ls-meta"><L ko="모든 승인 · VaR순 · revoke 대상" en="all approvals · by VaR · revoke targets" /></span>
      </div>
      <div className="ap-table">
        <div className="ap-row colhead">
          <span><L ko="자산" en="Asset" /></span>
          <span><L ko="유형" en="Type" /></span>
          <span><L ko="승인액" en="Allowance" /></span>
          <span className="col-r"><L ko="VaR" en="VaR" /></span>
          <span><L ko="Spender · 평판" en="Spender · reputation" /></span>
          <span className="col-r"><L ko="액션" en="Actions" /></span>
        </div>
        {rows.map(a => <ApprovalRow key={a.id} a={a} onRevoke={onRevoke} />)}
      </div>
    </section>
  );
}

function ApprovalRow({ a, onRevoke }) {
  const [editing, setEditing] = useSL2(false);
  const [val, setVal] = useSL2(a.allowance);
  return (
    <div className={`ap-row ${a.risk === 'none' ? 'safe' : a.risk}`}>
      <div className="ap-asset">
        <span className={`ha-ic ${a.type === 'NFT' ? 'nft' : ''}`}>{a.asset.slice(0, 3).toUpperCase()}</span>
        <div><div className="ha-sym">{a.asset}</div><ChainPill id={a.chain} /></div>
      </div>
      <div className="ap-type"><span className="type-chip">{a.type}</span>{a.delegation && <span className="deleg-note"><L ko="감지됨 · 분석 v2" en="detected · v2" /></span>}</div>
      <div className="ap-allow">
        {editing ? (
          <span className="allow-edit"><input value={val} onChange={e => setVal(e.target.value)} autoFocus /><button onClick={() => setEditing(false)}><MI.check style={{ width: 13, height: 13 }} /></button></span>
        ) : (
          <span className={`allow-val ${String(val).match(/unlimited|all items/i) ? 'unl' : ''}`}>
            {String(val).match(/unlimited|all items/i) && <MI.infinity style={{ width: 13, height: 13 }} />}{val}
            <button className="allow-edit-btn" onClick={() => setEditing(true)} title="edit"><MI.edit /></button>
          </span>
        )}
      </div>
      <div className="ap-var col-r"><VarCell value={a.varUsd} risk={a.risk !== 'none'} /></div>
      <div className="ap-spender"><Spender id={a.spender} /></div>
      <div className="ap-act col-r">
        <button className={`btn ${a.risk === 'none' ? 'ghost sm' : 'revoke'}`} onClick={() => onRevoke(a)}><MI.revoke style={{ width: 13, height: 13 }} /><L ko="Revoke" en="Revoke" /></button>
        <button className="ibtn" title="정책으로 만들기" onClick={() => onRevoke(a)}><MI.rule /></button>
        <button className="ibtn" title="Spender 차단" onClick={() => onRevoke(a)}><MI.block /></button>
      </div>
    </div>
  );
}

/* ── L2 view ── */
function L2View({ wallet, onRevoke }) {
  const [handled, setHandled] = useSL2({});
  const onHandle = (id) => setHandled(h => ({ ...h, [id]: true }));
  return (
    <>
      <L2Header wallet={wallet} />
      <ActionQueue wallet={wallet} onRevoke={onRevoke} handled={handled} onHandle={onHandle} />
      <FindingsFeed wallet={wallet} />
      <ApprovalsTable wallet={wallet} onRevoke={onRevoke} />
    </>
  );
}

Object.assign(window, { L2View, VPill });
