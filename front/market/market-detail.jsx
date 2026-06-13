/* Scopeball Market — 정책/패키지 상세 + 세트 패널 */

// 가짜 Cedar 원문 (플레이스홀더 — 실제 정책 원문은 핸드오프 후 주입)
function SourceBlock({ policy, locale }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="dt-block">
      <button className={"src-toggle" + (open ? " open" : "")} onClick={() => setOpen(!open)}>
        <Ico d={"M16 18 22 12 16 6 M8 6 2 12 8 18"} w={17} />
        <span style={{ flex: 1, textAlign: "left" }}>{Market.tChrome("section.source", locale)}</span>
        <span className="tag">cedar · manifest</span>
        <span className="chev"><Ico d={"M9 6l6 6-6 6"} w={16} /></span>
      </button>
      {open && (
        <pre className="src-code"><code>
<span className="cm">{"// " + policy.slug + " — placeholder (실제 Cedar 원문은 핸드오프 시 주입)"}</span>{"\n"}
<span className="kw">permit</span>{" ("}{"\n"}
{"  principal, action == Action::"}<span className="st">"signTransaction"</span>{", resource"}{"\n"}
{")"} <span className="kw">when</span> {"{"}{"\n"}
{"  resource.domain == "}<span className="st">"{policy.domain}"</span>{"\n"}
{"} "}<span className="kw">unless</span> {"{"}{"\n"}
{"  context.risk."}<span className="st">"{policy.intents[0] || policy.domain}"</span>{" >= "}<span className="nu">threshold</span>{"\n"}
{"};"}{"\n"}
<span className="cm">{"// severity: " + policy.severity + "  ·  evalClass: " + policy.evalClass}</span>
        </code></pre>
      )}
    </div>
  );
}

/* ════════════ 정책 상세 ════════════ */
function PolicyDetail({ slug, locale, ctx }) {
  const policy = Market.BY_SLUG[slug];
  if (!policy) return <div className="mk-canvas"><div className="empty"><h3>Not found</h3></div></div>;
  const c = Market.DOMAIN_COLOR[policy.domain];
  const inPkgs = Market.packagesContaining(slug);
  const inSet = ctx.isInSet("policy", slug);
  const rating = Market.ratingForPolicy(slug);
  const soon = policy.readiness === "soon";
  const ext = policy.readiness === "external";

  return (
    <div className="mk-canvas">
      <div className="detail">
        <div className="crumb">
          <a onClick={() => ctx.goPopular()}><span data-lang="ko">마켓</span><span data-lang="en">Market</span></a>
          <Ico d={ICONS.arrow} w={13} />
          <a onClick={() => ctx.goBrowse({ domain: policy.domain })}>{Market.domainName(policy.domain, locale)}</a>
          <Ico d={ICONS.arrow} w={13} />
          <span>{Market.pick(policy.name, locale)}</span>
        </div>

        <div className="dt-header" style={{ borderTopColor: c.hex }}>
          <div className="dt-badges">
            <DomainChip domain={policy.domain} locale={locale} />
            <SeverityBadge sev={policy.severity} locale={locale} />
            <ReadinessBadge rd={policy.readiness} locale={locale} />
          </div>
          <h1>{Market.pick(policy.name, locale)}</h1>
          <div className="dt-slug">{policy.slug}</div>
        </div>

        {/* 🎯 가치 카피 */}
        <div className="dt-value"><span className="tgt">🎯</span><span>{window.marketNLG(policy, locale)}</span></div>

        {/* 신뢰 바 (소셜 지표 없음 — 데이터 있는 것만) */}
        <div className="dt-trust">
          <span className="ti"><PublisherBadge locale={locale} /></span>
          <span className="ti"><Ico d={ICONS.shield} w={15} /><b><span data-lang="ko">검증된 정책</span><span data-lang="en">Verified policy</span></b></span>
          <span className="ti"><span data-lang="ko">의도 태그</span><span data-lang="en">Intents</span>
            <b>{policy.intents.length ? policy.intents.map((i) => Market.intentTag(i, locale)).join(" ") : (locale === "en" ? "—" : "없음")}</b>
          </span>
          {rating && <span className="ti"><RatingInline agg={rating} locale={locale} variant="bar" onClick={() => ctx.goCommunity(slug)} /></span>}
          {Market.versionFor(slug) && <span className="ti"><VersionTag id={slug} locale={locale} /></span>}
        </div>
        {!rating && (
          <div className="dt-norating">
            <span className="nr-txt"><span data-lang="ko">아직 평가 없음</span><span data-lang="en">No ratings yet</span></span>
            <button className="nr-cta" onClick={() => ctx.goCommunity(slug)}>
              <Ico d={ICONS.plus} w={14} /><span data-lang="ko">첫 평가 남기기</span><span data-lang="en">Write the first review</span>
            </button>
          </div>
        )}

        {/* 작동 시점 */}
        <div className="dt-block">
          <h3>{Market.tChrome("section.trigger_when", locale)}</h3>
          <div className="when-box">
            <span className="wb-ico"><Ico d={ICONS.bolt} w={20} /></span>
            <div>
              <div className="wb-txt">{window.marketTrigger(policy, locale)}</div>
              {ext && <div className="wb-sub"><span data-lang="ko">필요 연동: 오라클·블록리스트 피드</span><span data-lang="en">Requires an oracle / blocklist feed</span></div>}
              {soon && <div className="wb-sub"><span data-lang="ko">아직 평가되지 않습니다 — 출시 알림을 받아보세요</span><span data-lang="en">Not evaluated yet — follow for release</span></div>}
            </div>
          </div>
        </div>

        {/* 포함 패키지 */}
        {inPkgs.length > 0 && (
          <div className="dt-block">
            <h3>{Market.tChrome("section.included_in", locale)}</h3>
            <div className="incl-pkgs">
              {inPkgs.map((pk) => (
                <button key={pk.id} className="incl-pkg" onClick={() => ctx.openPackage(pk.id)}>
                  {Market.G.chrome.publisher.official.icon} {Market.pick(pk.name, locale)}
                  <span className="ip-ct">{pk.count}</span>
                </button>
              ))}
            </div>
          </div>
        )}

        {/* 원문 (접힘) */}
        <SourceBlock policy={policy} locale={locale} />

        {/* 하단 고정 CTA */}
        <div className="dt-cta">
          <AddButton readiness={policy.readiness} inSet={inSet} size="lg"
            onToggle={(a) => ctx.toggleItem("policy", slug, a)} locale={locale} />
          <span className="price-note">
            <span data-lang="ko">결제 없음 · 내 지갑 Draft에 추가됩니다</span>
            <span data-lang="en">No payment · added to your wallet draft</span>
          </span>
        </div>
      </div>
    </div>
  );
}

/* ════════════ 패키지 상세 ════════════ */
function PackageDetail({ pkgId, locale, ctx }) {
  const pkg = Market.PKG_BY_ID[pkgId];
  if (!pkg) return <div className="mk-canvas"><div className="empty"><h3>Not found</h3></div></div>;
  const inSet = ctx.isInSet("package", pkgId);
  const members = Market.sortForDisplay(pkg.members);
  const pkgRating = Market.ratingForPackage(pkg);

  return (
    <div className="mk-canvas">
      <div className="detail">
        <div className="crumb">
          <a onClick={() => ctx.goPopular()}><span data-lang="ko">마켓</span><span data-lang="en">Market</span></a>
          <Ico d={ICONS.arrow} w={13} />
          <span>{Market.pick(pkg.name, locale)}</span>
        </div>

        <div className="dt-header">
          <div className="dt-badges">
            <span className="official" style={{ background: "var(--sage-100)", color: "var(--sage-800)", display: "inline-flex", alignItems: "center", gap: 5, height: 24, padding: "0 10px", borderRadius: 999, fontSize: 12, fontWeight: 700 }}>
              {Market.G.chrome.publisher.official.icon} <span data-lang="ko">공식 패키지</span><span data-lang="en">Official package</span>
            </span>
            <DomainChip domain={pkg.primaryDomain} locale={locale} />
          </div>
          <h1>{Market.pick(pkg.name, locale)}</h1>
        </div>

        <div className="dt-value"><span className="tgt">🎯</span><span>{Market.pick(pkg.tagline, locale)}</span></div>

        <div className="dt-trust">
          <span className="ti"><PublisherBadge locale={locale} /></span>
          <span className="ti"><Ico d={ICONS.bolt} w={15} /><b>{pkg.readyCount}</b> / {pkg.count} <span data-lang="ko">즉시작동</span><span data-lang="en">ready</span></span>
          <span className="ti">{pkg.intents.map((i) => Market.intentTag(i, locale)).join(" ")}</span>
          {pkgRating
            ? <span className="ti"><RatingInline agg={pkgRating} locale={locale} variant="bar" /><span className="rollup-note"><span data-lang="ko">(포함 정책 기준)</span><span data-lang="en">(across policies)</span></span></span>
            : <span className="ti" style={{ color: "var(--slate-300)" }}><span data-lang="ko">아직 평가 없음</span><span data-lang="en">No ratings yet</span></span>}
          {Market.versionFor(pkgId) && <span className="ti"><VersionTag id={pkgId} locale={locale} /></span>}
        </div>

        <div className="dt-block">
          <h3>{Market.tChrome("section.policies_in", locale)} · {pkg.count}</h3>
          <div className="mlist">
            {members.map((m) => <MiniRow key={m.slug} policy={m} locale={locale} onOpen={ctx.openPolicy} />)}
          </div>
        </div>

        <div className="dt-cta">
          <button className={"addbtn lg" + (inSet ? " in-set" : "")} onClick={() => ctx.toggleItem("package", pkgId, inSet ? "remove" : "add")}>
            {inSet
              ? <span><Ico d={ICONS.check} w={16} /> <span data-lang="ko">담김</span><span data-lang="en">In set</span></span>
              : <span>{Market.tChrome("action.add_package", locale)}</span>}
          </button>
          <span className="price-note">
            <span data-lang="ko">개별 정책만 골라 담으려면 위 목록에서 선택하세요</span>
            <span data-lang="en">Prefer individual policies? Pick them from the list above</span>
          </span>
        </div>
      </div>
    </div>
  );
}

/* ════════════ 세트 패널 ════════════ */
function SetPanel({ open, onClose, locale, items, ctx }) {
  // items: [{type:'policy'|'package', id}]
  const policies = items.filter((i) => i.type === "policy").map((i) => Market.BY_SLUG[i.id]).filter(Boolean);
  const packages = items.filter((i) => i.type === "package").map((i) => Market.PKG_BY_ID[i.id]).filter(Boolean);

  // 충돌/중복 감지: 같은 slug가 패키지+단일로 중복되거나, 동일 도메인에 deny/warn 혼재
  const allSlugs = {};
  policies.forEach((p) => { allSlugs[p.slug] = (allSlugs[p.slug] || 0) + 1; });
  packages.forEach((pk) => pk.members.forEach((m) => { allSlugs[m.slug] = (allSlugs[m.slug] || 0) + 1; }));
  const dup = Object.keys(allSlugs).filter((s) => allSlugs[s] > 1).length;
  const totalCount = policies.length + packages.reduce((a, p) => a + p.count, 0);

  return (
    <React.Fragment>
      <div className={"set-scrim" + (open ? " open" : "")} onClick={onClose}></div>
      <aside className={"set-panel" + (open ? " open" : "")}>
        <div className="set-head">
          <h2>{Market.tChrome("set_panel.title", locale)}</h2>
          <span className="sc">{totalCount > 0 ? totalCount + (locale === "en" ? " policies" : "개 정책") : ""}</span>
          <button className="x" onClick={onClose}><Ico d={ICONS.x} w={18} /></button>
        </div>

        <div className="set-body">
          {items.length === 0 ? (
            <div className="set-empty">
              <Ico d={"M3 7h18l-2 13H5zM8 7V5a4 4 0 0 1 8 0v2"} w={44} />
              <div>{Market.tChrome("set_panel.empty", locale)}</div>
            </div>
          ) : (
            <React.Fragment>
              {dup > 0 && (
                <div className="set-warn">
                  <Ico d={"M12 9v4M12 17h.01M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"} w={16} />
                  <span>{Market.G.chrome.set_panel.dup_warn[locale === "en" ? "en" : "ko"]}</span>
                </div>
              )}
              {packages.map((pk) => (
                <div key={"pk" + pk.id}>
                  <div className="set-item pkg" style={{ borderLeftColor: Market.DOMAIN_COLOR[pk.primaryDomain].hex }}>
                    <span className="official" style={{ fontSize: 11 }}>{Market.G.chrome.publisher.official.icon}</span>
                    <div className="si-txt">
                      <div className="si-name">{Market.pick(pk.name, locale)}</div>
                      <div className="si-meta"><span data-lang="ko">패키지 · {pk.count}개</span><span data-lang="en">Package · {pk.count}</span></div>
                    </div>
                    <button className="si-x" onClick={() => ctx.toggleItem("package", pk.id, "remove")}><Ico d={ICONS.x} w={15} /></button>
                  </div>
                  <div className="set-sub">
                    {pk.members.slice(0, 4).map((m) => (
                      <div className="ss-row" key={m.slug}><DomainGlyph domain={m.domain} size={13} /><span>{Market.pick(m.name, locale)}</span></div>
                    ))}
                    {pk.count > 4 && <div className="ss-row" style={{ color: "var(--slate-300)" }}>+{pk.count - 4} <span data-lang="ko">더</span><span data-lang="en">more</span></div>}
                  </div>
                </div>
              ))}
              {policies.map((p) => (
                <div className="set-item" key={p.slug} style={{ borderLeftColor: Market.DOMAIN_COLOR[p.domain].hex }}>
                  <DomainGlyph domain={p.domain} size={16} />
                  <div className="si-txt">
                    <div className="si-name">{Market.pick(p.name, locale)}</div>
                    <div className="si-meta">
                      <SeverityBadge sev={p.severity} locale={locale} />
                      <ReadinessBadge rd={p.readiness} locale={locale} />
                    </div>
                  </div>
                  <button className="si-x" onClick={() => ctx.toggleItem("policy", p.slug, "remove")}><Ico d={ICONS.x} w={15} /></button>
                </div>
              ))}
            </React.Fragment>
          )}
        </div>

        {items.length > 0 && (
          <div className="set-foot">
            <button className="addbtn lg" onClick={ctx.commitDraft}>
              <Ico d={"M3 7v13h18V7M3 7l3-4h12l3 4M3 7h18M12 11v5M9.5 13.5 12 16l2.5-2.5"} w={16} />
              {Market.tChrome("action.add_to_draft", locale)}
            </button>
            <div className="secondary">
              <button className="addbtn ghost" onClick={ctx.saveSet}>{Market.tChrome("action.save_as_set", locale)}</button>
              <button className="addbtn ghost" onClick={ctx.shareSet}>{Market.tChrome("action.share_link", locale)}</button>
            </div>
            <div className="price-free"><span data-lang="ko">결제 요소 없음 · 설치 세트입니다</span><span data-lang="en">No checkout · this is an install set</span></div>
          </div>
        )}
      </aside>
    </React.Fragment>
  );
}

Object.assign(window, { SourceBlock, PolicyDetail, PackageDetail, SetPanel });
