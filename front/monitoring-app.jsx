// monitoring-app.jsx — Scopeball Monitoring console. Orchestrates L1 ⇄ L2 + revoke handoff modal.

const { useState: useSApp, useEffect: useEApp } = React;

function MonitoringApp() {
  const [t, setTweak] = useTweaks(MON_TWEAKS);
  const [view, setView] = useSApp('l1');          // 'l1' | 'l2'
  const [sel, setSel] = useSApp('all');            // L1 wallet switcher
  const [drill, setDrill] = useSApp('main');       // L2 target wallet
  const [suggestOpen, setSuggestOpen] = useSApp(true);
  const [modal, setModal] = useSApp(null);         // approval being revoked

  const lens = t.lens, setLens = (v) => setTweak('lens', v);

  // set body attrs synchronously during render so JS reads (ds()) and CSS see the current value
  document.body.setAttribute('data-locale', t.locale);
  document.body.setAttribute('data-density', t.density);
  document.body.setAttribute('data-lens', lens);
  document.body.setAttribute('data-state', t.state);

  const onDrill = (walletId) => { setDrill(walletId); setView('l2'); };
  const wallet = WALLETS.find(w => w.id === drill) || WALLETS[0];

  return (
    <>
      <NavRail />
      <main className="content">
        {/* topbar */}
        <div className="topbar">
          {view === 'l1' ? (
            <div className="crumb">
              <span className="here">Scopeball Monitoring</span>
              <span className="sep">/</span>
              <span><L ko="Acme · 4 지갑" en="Acme · 4 wallets" /></span>
            </div>
          ) : (
            <div className="crumb">
              <button className="back" onClick={() => setView('l1')}><MI.back /><L ko="포트폴리오" en="Portfolio" /></button>
              <span className="sep">/</span>
              <span className="here"><L {...wallet.name} /></span>
              <span className="mono" style={{ fontSize: 12, color: 'var(--slate-400)' }}>{wallet.addr}</span>
            </div>
          )}
          <span className="spacer"></span>
          <ReadOnly />
          <span className="updated"><L {...SUMMARY.updated} /></span>
        </div>

        {view === 'l1'
          ? <L1View sel={sel} setSel={setSel} lens={lens} setLens={setLens} onDrill={onDrill} suggestOpen={suggestOpen} setSuggestOpen={setSuggestOpen} />
          : <L2View wallet={wallet} onRevoke={(ap) => setModal(ap)} onBack={() => setView('l1')} />}
      </main>

      {modal && <RevokeModal approval={modal} onClose={() => setModal(null)} />}

      <TweaksPanel>
        <TweakSection label="렌즈" />
        <TweakRadio label="자산 / 위험 보기" value={t.lens} options={['assets', 'risk']} onChange={(v) => setTweak('lens', v)} />
        <TweakSection label="데이터 상태 (백엔드 Phase)" />
        <TweakRadio label="data state" value={t.state} options={['full', 'p0', 'loading']} onChange={(v) => setTweak('state', v)} />
        <TweakSection label="밀도 · 언어" />
        <TweakRadio label="density" value={t.density} options={['compact', 'regular', 'comfy']} onChange={(v) => setTweak('density', v)} />
        <TweakRadio label="locale" value={t.locale} options={['ko', 'en']} onChange={(v) => setTweak('locale', v)} />
      </TweaksPanel>
    </>
  );
}

const MON_TWEAKS = /*EDITMODE-BEGIN*/{
  "lens": "assets",
  "state": "full",
  "density": "regular",
  "locale": "ko"
}/*EDITMODE-END*/;

ReactDOM.createRoot(document.getElementById('app')).render(<MonitoringApp />);
