// monitoring-modal.jsx — revoke handoff modal (H). Read-only / non-custodial:
// Scopeball assembles the UNSIGNED tx and previews it; the EXTERNAL wallet signs via WalletConnect.

const { useState: useSMod, useEffect: useEMod } = React;

const STEPS = [
  { id: 'connect', ko: '연결', en: 'Connect' },
  { id: 'sign', ko: '서명', en: 'Sign' },
  { id: 'confirm', ko: '확인', en: 'Confirm' },
  { id: 'done', ko: '완료', en: 'Done' },
];

function RevokeModal({ approval: a, onClose }) {
  const [phase, setPhase] = useSMod('preview'); // preview | connecting | signing | confirming | done
  const s = spenderOf(a.spender);
  const isNft = a.type === 'NFT';
  const stepIdx = { preview: -1, connecting: 0, signing: 1, confirming: 2, done: 3 }[phase];

  // auto-advance the connecting → signing and confirming → done waits (simulated)
  useEMod(() => {
    if (phase === 'connecting') { const id = setTimeout(() => setPhase('signing'), 1400); return () => clearTimeout(id); }
    if (phase === 'confirming') { const id = setTimeout(() => setPhase('done'), 1600); return () => clearTimeout(id); }
  }, [phase]);

  useEMod(() => {
    const h = (e) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', h); return () => window.removeEventListener('keydown', h);
  }, []);

  return (
    <div className="modal-backdrop" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="modal" role="dialog" aria-modal="true">
        <div className="modal-head">
          <div className="mh-title"><span className="mh-ic"><MI.revoke /></span>
            <div><div className="mh-t"><L ko="승인 취소 (Revoke)" en="Revoke approval" /></div><div className="mh-sub"><L ko="외부 지갑이 서명합니다 · Scopeball은 서명하지 않음" en="Signed by your external wallet · Scopeball never signs" /></div></div>
          </div>
          <button className="modal-close" onClick={onClose} aria-label="close"><MI.x /></button>
        </div>

        {/* step indicator */}
        <div className="mod-steps">
          {STEPS.map((st, i) => (
            <div key={st.id} className={`mod-step ${stepIdx === i ? 'on' : ''} ${stepIdx > i ? 'done' : ''}`}>
              <span className="ms-dot">{stepIdx > i ? <MI.check style={{ width: 11, height: 11 }} /> : i + 1}</span>
              <span className="ms-lab"><L ko={st.ko} en={st.en} /></span>
            </div>
          ))}
        </div>

        <div className="mod-body">
          {/* unsigned tx preview — always shown */}
          <div className="tx-preview">
            <div className="txp-tag"><L ko="미서명 트랜잭션 미리보기" en="Unsigned transaction preview" /></div>
            <div className="txp-headline">
              <L ko={<>이 트랜잭션에 서명하면 <b>{s.short}</b>의 <b>{a.asset}</b> {isNft ? '컬렉션 승인이 해제' : '승인이 0으로 설정'}됩니다.</>}
                 en={<>Signing this sets <b>{s.short}</b>'s <b>{a.asset}</b> {isNft ? 'collection approval to revoked' : 'allowance to 0'}.</>} />
            </div>
            <dl className="txp-props">
              <dt>method</dt><dd className="mono">{isNft ? 'setApprovalForAll(false)' : 'approve(spender, 0)'}</dd>
              <dt><L ko="대상" en="token" /></dt><dd className="mono">{a.asset} · <ChainPill id={a.chain} /></dd>
              <dt>spender</dt><dd><Spender id={a.spender} /></dd>
              <dt><L ko="현재 한도" en="current" /></dt><dd className="mono"><span className="txp-from">{a.allowance}</span> <MI.caretR style={{ width: 12, height: 12, verticalAlign: 'middle', color: 'var(--slate-300)' }} /> <span className="txp-to">0</span></dd>
            </dl>
          </div>

          {/* phase-specific body */}
          {phase === 'preview' && (
            <div className="nc-note"><MI.lock /><L ko="Scopeball은 키를 보관하지 않습니다. 연결된 외부 지갑에서만 서명이 이루어집니다." en="Scopeball holds no keys. Signing happens only in your connected external wallet." /></div>
          )}
          {phase === 'connecting' && (
            <div className="wc-box">
              <div className="qr-ph"><MI.wc /></div>
              <div className="wc-txt"><div className="wc-t"><L ko="WalletConnect 연결 대기…" en="Waiting for WalletConnect…" /></div><div className="wc-d"><L ko="외부 지갑 앱에서 연결을 승인하세요." en="Approve the connection in your wallet app." /></div></div>
            </div>
          )}
          {phase === 'signing' && (
            <div className="wait-box"><span className="spinner"></span><div><div className="wc-t"><L ko="외부 지갑에서 서명 대기 중" en="Awaiting signature in your wallet" /></div><div className="wc-d"><L ko="지갑 앱에 뜬 트랜잭션을 확인하고 서명하세요. Scopeball은 이 단계에 개입하지 않습니다." en="Review and sign the tx shown in your wallet. Scopeball does not intervene here." /></div></div></div>
          )}
          {phase === 'confirming' && (
            <div className="wait-box"><span className="spinner"></span><div><div className="wc-t"><L ko="온체인 처리 확인 중…" en="Confirming on-chain…" /></div><div className="wc-d mono">tx 0x4f9a···c1e2 · 1/2 confirmations</div></div></div>
          )}
          {phase === 'done' && (
            <div className="done-box">
              <span className="done-art"><MI.check /></span>
              <div className="done-t"><L ko="승인이 취소되었습니다" en="Approval revoked" /></div>
              <div className="done-d"><L ko="큐에서 제거되고 감사 로그(History)에 기록됐어요." en="Removed from the queue and written to the audit trail (History)." /></div>
            </div>
          )}
        </div>

        <div className="mod-foot">
          {phase === 'preview' && (
            <>
              <a className="policy-link" href="Editor v6.html"><MI.rule /><L ko="정책으로 만들기 · rule#approve.cap" en="Make a policy · rule#approve.cap" /></a>
              <span style={{ flex: 1 }}></span>
              <button className="btn ghost" onClick={onClose}><L ko="취소" en="Cancel" /></button>
              <button className="btn revoke" onClick={() => setPhase('connecting')}><MI.wc /><L ko="외부 지갑 연결" en="Connect wallet" /></button>
            </>
          )}
          {(phase === 'connecting' || phase === 'signing') && (
            <>
              <span className="foot-hint mono"><L ko="WalletConnect · 외부 지갑 서명" en="WalletConnect · external signer" /></span>
              <span style={{ flex: 1 }}></span>
              <button className="btn ghost" onClick={onClose}><L ko="취소" en="Cancel" /></button>
              {phase === 'signing' && <button className="btn revoke" onClick={() => setPhase('confirming')}><L ko="서명 완료 (데모)" en="Signed (demo)" /></button>}
            </>
          )}
          {phase === 'confirming' && (<><span style={{ flex: 1 }}></span><button className="btn ghost" disabled style={{ opacity: 0.5 }}><L ko="처리 중…" en="Processing…" /></button></>)}
          {phase === 'done' && (<><span style={{ flex: 1 }}></span><button className="btn primary" onClick={onClose}><L ko="완료" en="Done" /></button></>)}
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { RevokeModal });
