/* Scopeball Market — 카드 & 배지 컴포넌트 */
const { useState } = React;

// ── 작은 아이콘들 ──
function Ico({ d, w = 16 }) {
  return (
    <svg width={w} height={w} viewBox="0 0 24 24" fill="none" stroke="currentColor"
         strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round">
      {Array.isArray(d) ? d.map((p, i) => <path key={i} d={p} />) : <path d={d} />}
    </svg>
  );
}
const ICONS = {
  plus: "M12 5v14M5 12h14",
  check: "M20 6 9 17l-5-5",
  bell: "M18 8a6 6 0 1 0-12 0c0 7-3 9-3 9h18s-3-2-3-9M13.7 21a2 2 0 0 1-3.4 0",
  gear: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9c.2.61.76 1.05 1.42 1.05H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z",
  hourglass: "M6 2h12M6 22h12M6 2c0 4 3 6 6 10M18 2c0 4-3 6-6 10M6 22c0-4 3-6 6-10M18 22c0-4-3-6-6-10",
  bolt: "M13 2 3 14h7l-1 8 10-12h-7z",
  shield: "M12 3l7 3v5c0 4-3 7-7 9-4-2-7-5-7-9V6z",
  arrow: "M5 12h14M13 6l6 6-6 6",
  x: "M18 6 6 18M6 6l12 12",
  star: "M12 3l2.9 5.9 6.5.9-4.7 4.6 1.1 6.5L12 18l-5.8 3 1.1-6.5L2.6 9.8l6.5-.9z",
};

// ── 배지 ──
function SeverityBadge({ sev, locale }) {
  const m = Market.severityMeta(sev);
  const label = locale === "en" ? m.en : m.ko;
  return <span className={"badge sev-" + sev} title={locale === "en" ? m.desc_en : m.desc_ko}>{label}</span>;
}
function ReadinessBadge({ rd, locale }) {
  const m = Market.readinessMeta(rd);
  const label = locale === "en" ? m.en : m.ko;
  return (
    <span className={"badge rd-" + rd} title={locale === "en" ? m.desc_en : m.desc_ko}>
      <span className="bi" style={{ fontSize: 11, lineHeight: 1 }}>{m.icon}</span>{label}
    </span>
  );
}
function PublisherBadge({ locale }) {
  const m = Market.G.chrome.publisher.official;
  return <span className="badge pub">{m.icon} {locale === "en" ? m.en : m.ko}</span>;
}

// 버전 태그 (단일 출처 Market.versionFor, 중립색, 예시 배지). 값 없으면 null.
function VersionTag({ id, locale, variant }) {
  const v = Market.versionFor(id);
  if (!v) return null;
  return (
    <span className={"vtag" + (variant ? " " + variant : "")} title={locale === "en" ? "Version (sample)" : "버전 (예시)"}>
      <span className="vt-num">{v}</span>
      <span className="ex-badge"><span data-lang="ko">예시</span><span data-lang="en">sample</span></span>
    </span>
  );
}

// ── 도메인 칩 ──
function DomainChip({ domain, locale }) {
  const c = Market.DOMAIN_COLOR[domain];
  return (
    <span className="dchip" style={{ background: c.soft, color: c.ink }}>
      <svg className="dico" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round">
        {Market.DOMAIN_ICON[domain].split("M").filter(Boolean).map((seg, i) => <path key={i} d={"M" + seg} />)}
      </svg>
      {Market.domainName(domain, locale)}
    </span>
  );
}
function DomainGlyph({ domain, size = 22 }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      {Market.DOMAIN_ICON[domain].split("M").filter(Boolean).map((seg, i) => <path key={i} d={"M" + seg} />)}
    </svg>
  );
}

// ── 해시태그 칩 ──
function HashTag({ intent, locale, active, onClick }) {
  return (
    <button className={"htag" + (active ? " on" : "")} onClick={onClick}>
      {Market.intentTag(intent, locale)}
    </button>
  );
}

// ── Add-to-set 버튼 ──
function AddButton({ readiness, inSet, onToggle, locale, size }) {
  const cls = "addbtn" + (size === "lg" ? " lg" : "");
  if (!Market.canAddToSet(readiness)) {
    return (
      <button className={cls + " notify"} onClick={(e) => { e.stopPropagation(); onToggle && onToggle("notify"); }}>
        <Ico d={ICONS.bell} w={15} />{Market.tChrome("action.notify_me", locale)}
      </button>
    );
  }
  if (inSet) {
    return (
      <button className={cls + " in-set"} onClick={(e) => { e.stopPropagation(); onToggle("remove"); }}>
        <Ico d={ICONS.check} w={15} /><span data-lang="ko">담김</span><span data-lang="en">In set</span>
      </button>
    );
  }
  return (
    <button className={cls} onClick={(e) => { e.stopPropagation(); onToggle("add"); }}>
      <Ico d={ICONS.plus} w={15} />{Market.tChrome("action.add_to_set", locale)}
    </button>
  );
}

// ── PolicyCard ──
function PolicyCard({ policy, locale, inSet, onToggle, onOpen }) {
  const c = Market.DOMAIN_COLOR[policy.domain];
  const soon = policy.readiness === "soon";
  const ext = policy.readiness === "external";
  return (
    <div className={"pcard" + (soon ? " is-soon" : "")} style={{ borderLeftColor: c.hex }}
         onClick={() => onOpen(policy.slug)} role="button">
      <div className="top">
        <DomainChip domain={policy.domain} locale={locale} />
        <div className="badges">
          <SeverityBadge sev={policy.severity} locale={locale} />
          <ReadinessBadge rd={policy.readiness} locale={locale} />
        </div>
      </div>
      <div>
        <div className="title">{Market.pick(policy.name, locale)}</div>
        <div className="slug-row">
          <span className="slug">{policy.slug}</span>
          <VersionTag id={policy.slug} locale={locale} variant="mini" />
        </div>
      </div>
      <div className="reason">
        <span className="tgt">🎯</span>
        <span>{window.marketNLG(policy, locale)}</span>
      </div>
      <div className="tags">
        {policy.intents.slice(0, 3).map((i) => (
          <span key={i} className="htag" style={{ pointerEvents: "none" }}>{Market.intentTag(i, locale)}</span>
        ))}
        {ext && (
          <span className="ext-note"><Ico d={ICONS.gear} w={13} />
            <span data-lang="ko">피드 연동 시 작동</span><span data-lang="en">Works with a feed</span>
          </span>
        )}
      </div>
      <div className="pc-rating">
        <RatingInline agg={Market.ratingForPolicy(policy.slug)} locale={locale} />
      </div>
      <div className="foot">
        <span className="trust"><PublisherBadge locale={locale} /></span>
        <div className="addbtn-wrap foot-add">
          <AddButton readiness={policy.readiness} inSet={inSet} onToggle={onToggle} locale={locale} />
        </div>
      </div>
    </div>
  );
}

// ── PackageCard ──
function PackageCard({ pkg, locale, inSet, onToggle, onOpen }) {
  const c = Market.DOMAIN_COLOR[pkg.primaryDomain];
  return (
    <div className="pkgcard" onClick={() => onOpen(pkg.id)} role="button">
      <div className="ptop">
        <span className="official">{Market.G.chrome.publisher.official.icon}
          <span data-lang="ko">공식 패키지</span><span data-lang="en">Official package</span>
        </span>
        <span className="pmeta">
          <span data-lang="ko">정책 {pkg.count}개</span><span data-lang="en">{pkg.count} policies</span>
        </span>
        <VersionTag id={pkg.id} locale={locale} variant="mini" />
      </div>
      <div className="title">{Market.pick(pkg.name, locale)}</div>
      <div className="tagline">{Market.pick(pkg.tagline, locale)}</div>
      <div className="tags">
        {pkg.intents.map((i) => (
          <span key={i} className="htag" style={{ pointerEvents: "none" }}>{Market.intentTag(i, locale)}</span>
        ))}
      </div>
      <div className="pfoot">
        <span className="readymeta">
          <Ico d={ICONS.bolt} w={14} />
          <span data-lang="ko"><b>{pkg.readyCount}</b>개 즉시작동</span>
          <span data-lang="en"><b>{pkg.readyCount}</b> ready now</span>
        </span>
        <span className="pkg-rating"><RatingInline agg={Market.ratingForPackage(pkg)} locale={locale} /></span>
        <div style={{ marginLeft: "auto" }}>
          <AddButton readiness="ready" inSet={inSet} onToggle={onToggle} locale={locale} />
        </div>
      </div>
    </div>
  );
}

// ── Mini policy row (패키지 상세) ──
function MiniRow({ policy, locale, onOpen }) {
  const c = Market.DOMAIN_COLOR[policy.domain];
  return (
    <div className="mrow" style={{ borderLeftColor: c.hex }} onClick={() => onOpen(policy.slug)} role="button">
      <div className="mr-txt">
        <div className="mr-name">{Market.pick(policy.name, locale)}</div>
        <div className="mr-slug">{policy.slug}</div>
      </div>
      <div className="mr-badges">
        <SeverityBadge sev={policy.severity} locale={locale} />
        <ReadinessBadge rd={policy.readiness} locale={locale} />
        <Ico d={ICONS.arrow} w={17} />
      </div>
    </div>
  );
}

Object.assign(window, {
  Ico, ICONS, SeverityBadge, ReadinessBadge, PublisherBadge, VersionTag,
  DomainChip, DomainGlyph, HashTag, AddButton,
  PolicyCard, PackageCard, MiniRow,
});
