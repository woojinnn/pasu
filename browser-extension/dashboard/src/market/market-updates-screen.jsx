/* Scopeball Market — Updates (시간순 타임라인 로그)
   기여자는 모두 동등하게 크레딧. 안전 신호는 사람 등급이 아니라 '검수 상태'로만. */

// 작성자 크레딧 (모두 동등 — 서열 없음)
function AuthorCredit({ author, locale, size }) {
  return (
    <span className="author-credit">
      <Avatar handle={author.handle} size={size || 20} />
      <span className="ac-name">{author.displayName}</span>
      <span className="ac-handle">@{author.handle}</span>
    </span>
  );
}
// 검수 상태 칩 (사람을 가르지 않음 · 공식 검수는 긍정, 그 외는 중립 '검토 중')
function ReviewChip({ audited, locale, compact }) {
  return audited
    ? <span className={"rev-chip ok" + (compact ? " cmp" : "")} title={locale === "en" ? "Reviewed by the official team" : "공식 팀 검수 완료"}>
        <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round"><path d="M20 6 9 17l-5-5"/></svg>
        <span data-lang="ko">공식 검수</span><span data-lang="en">Reviewed</span></span>
    : <span className={"rev-chip pending" + (compact ? " cmp" : "")} title={locale === "en" ? "Community contribution, in review" : "커뮤니티 기여 · 검토 진행 중"}>
        <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M12 7v5l3 2M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0"/></svg>
        <span data-lang="ko">검토 중</span><span data-lang="en">In review</span></span>;
}
function TypeLabel({ type, locale }) {
  return <span className={"row-type " + type}><span className="rt-dot"></span>{Market.tChrome("updates." + type, locale)}</span>;
}
function VersionDelta({ from, to, type }) {
  if (type === "new_release") return <span className="vdelta new"><b>{to}</b></span>;
  return <span className="vdelta"><span className="vd-from">{from}</span><span className="vd-arr">→</span><b>{to}</b></span>;
}

function UpdateRow({ u, locale, ctx, onDiff }) {
  const c = Market.DOMAIN_COLOR[u._domain] || Market.DOMAIN_COLOR.security;
  const isPkg = u.target.type === "package";
  const kinds = (u.changeKind || []).map((k) => tt(CHANGE_KIND[k], locale)).join(", ");
  function openTarget() { isPkg ? ctx.openPackage(u.target.ref) : ctx.openPolicy(u.target.ref); }
  return (
    <div className="utl-row" onClick={openTarget} role="button" title={kinds ? (locale === "en" ? "Change: " + kinds : "변경 종류: " + kinds) : ""}>
      <span className="row-time">{relTime(u.publishedAt, locale)}</span>
      <TypeLabel type={u.type} locale={locale} />
      <span className="row-target">
        <span className="rt-ico" style={{ color: c.hex }}>{isPkg ? <Ico d={ICONS.shield} w={15} /> : <DomainGlyph domain={u._domain} size={15} />}</span>
        <span className="rt-name">{Market.pick(u._name, locale)}</span>
      </span>
      {u.type === "version_bump" && <VersionDelta from={u.fromVersion} to={u.toVersion} type={u.type} />}
      <ReviewChip audited={u.audited} locale={locale} compact />
      <button className="row-by" onClick={(e) => { e.stopPropagation(); ctx.goCommunity && ctx.goCommunity(null); }} title={u.author.displayName}>@{u.author.handle}</button>
      <span className="row-log">{tt(u.changelog, locale)}</span>
      <span className="row-ex" title={locale === "en" ? "Sample data" : "예시 데이터"}><span data-lang="ko">예시</span><span data-lang="en">sample</span></span>
      {u.type === "version_bump" && (
        <button className="row-diff" onClick={(e) => { e.stopPropagation(); onDiff(u); }}><span data-lang="ko">변경점</span><span data-lang="en">diff</span></button>
      )}
    </div>
  );
}

function DiffPanel({ open, u, locale, onClose, ctx }) {
  const lines = u ? makeDiff(u) : [];
  return (
    <React.Fragment>
      <div className={"diff-scrim" + (open ? " open" : "")} onClick={onClose}></div>
      <aside className={"diff-panel" + (open ? " open" : "")}>
        {u && (
          <React.Fragment>
            <div className="diff-head">
              <div>
                <div className="dh-title">{Market.pick(u._name, locale)}</div>
                <div className="dh-sub">{u.target.ref}</div>
              </div>
              <button className="x" onClick={onClose}><Ico d={ICONS.x} w={18} /></button>
            </div>
            <div className="diff-src">
              <AuthorCredit author={u.author} locale={locale} />
              <VersionDelta from={u.fromVersion} to={u.toVersion} type={u.type} />
              <ReviewChip audited={u.audited} locale={locale} />
            </div>
            <div className="diff-meta">
              <span className="diff-sample"><span data-lang="ko">예시 diff · Cedar 원문은 플레이스홀더</span><span data-lang="en">Sample diff · Cedar source is placeholder</span></span>
              <button className="diff-link" onClick={() => { onClose(); u.target.type === "package" ? ctx.openPackage(u.target.ref) : ctx.openPolicy(u.target.ref); }}>
                <span data-lang="ko">이 정책 상세로</span><span data-lang="en">Open detail</span><Ico d={ICONS.arrow} w={13} />
              </button>
            </div>
            <pre className="diff-code"><code>
{lines.map((l, i) => (
  <div key={i} className={"dl " + (l.s === "+" ? "add" : l.s === "-" ? "del" : l.s === "h" ? "hh" : "ctx")}>
    <span className="dl-sign">{l.s === "+" ? "+" : l.s === "-" ? "-" : l.s === "h" ? "" : " "}</span>
    <span className="dl-text">{l.t}</span>
  </div>
))}
            </code></pre>
            <div className="diff-foot">
              <span className="uc-label">{Market.tChrome("updates.changelog", locale)}</span>
              <span>{tt(u.changelog, locale)}</span>
            </div>
          </React.Fragment>
        )}
      </aside>
    </React.Fragment>
  );
}

function UpdatesScreen({ locale, ctx, fireToast }) {
  const [typeFilter, setTypeFilter] = useState("all");
  const [domainFilter, setDomainFilter] = useState("");
  const [reviewFilter, setReviewFilter] = useState("all");
  const [followOnly, setFollowOnly] = useState(false);
  const [followed] = useState(() => lsGet("mk_follows", {}));
  const [diffItem, setDiffItem] = useState(null);
  const [diffOpen, setDiffOpen] = useState(false);
  const [collapsed, setCollapsed] = useState({});

  const domains = SEED_UPDATES.map((u) => u._domain).filter((d, i, a) => a.indexOf(d) === i);
  function openDiff(u) { setDiffItem(u); setDiffOpen(true); }

  let list = SEED_UPDATES.slice().sort((a, b) => new Date(b.publishedAt) - new Date(a.publishedAt));
  if (typeFilter !== "all") list = list.filter((u) => u.type === typeFilter);
  if (domainFilter) list = list.filter((u) => u._domain === domainFilter);
  if (reviewFilter === "audited") list = list.filter((u) => u.audited);
  else if (reviewFilter === "review") list = list.filter((u) => !u.audited);
  if (followOnly) list = list.filter((u) => followed[u.id]);

  const cnt = (fn) => SEED_UPDATES.filter(fn).length;
  const typeCounts = { all: SEED_UPDATES.length, new_release: cnt((u) => u.type === "new_release"), version_bump: cnt((u) => u.type === "version_bump") };
  const reviewCounts = { all: SEED_UPDATES.length, audited: cnt((u) => u.audited), review: cnt((u) => !u.audited) };
  const followN = SEED_UPDATES.filter((u) => followed[u.id]).length;

  const order = locale === "en" ? BUCKET_ORDER_EN : BUCKET_ORDER_KO;
  const groups = {};
  list.forEach((u) => { const b = dateBucket(u.publishedAt, locale); (groups[b] = groups[b] || []).push(u); });
  const lastBucket = order[order.length - 1];
  function isCollapsed(b) { return collapsed[b] != null ? collapsed[b] : (b === lastBucket); }

  const SEGS = [["all", { ko: "전체", en: "All" }], ["new_release", Market.G.chrome.updates.new_release], ["version_bump", Market.G.chrome.updates.version_bump]];
  const REVS = [["all", { ko: "전체", en: "All" }], ["audited", { ko: "공식 검수", en: "Reviewed" }], ["review", { ko: "검토 중", en: "In review" }]];

  return (
    <div className="cmty4">
      <div className="c4-feed">
        <div className="upd-filters one">
          <div className="seg-pills">
            {SEGS.map(([k, lab]) => (
              <button key={k} className={"seg-pill sm" + (typeFilter === k ? " on" : "")} onClick={() => setTypeFilter(k)}>{locale === "en" ? lab.en : lab.ko}<span className="pill-n">{typeCounts[k]}</span></button>
            ))}
            <span className="filt-sep"></span>
            {REVS.map(([k, lab]) => (
              <button key={k} className={"seg-pill sm" + (reviewFilter === k ? " on" : "")} onClick={() => setReviewFilter(k)}>{tt(lab, locale)}<span className="pill-n">{reviewCounts[k]}</span></button>
            ))}
            <select className="cm-input sm" value={domainFilter} onChange={(e) => setDomainFilter(e.target.value)}>
              <option value="">{locale === "en" ? "All domains" : "전체 도메인"}</option>
              {domains.map((d) => <option key={d} value={d}>{Market.domainName(d, locale)}</option>)}
            </select>
            <button className={"seg-pill sm" + (followOnly ? " on" : "")} onClick={() => setFollowOnly(!followOnly)}>
              <span data-lang="ko">팔로우만</span><span data-lang="en">Following</span><span className="pill-n">{followN}</span>
            </button>
          </div>
        </div>

        {list.length === 0 ? (
          <div className="cv-empty"><h3><span data-lang="ko">표시할 업데이트가 없습니다</span><span data-lang="en">No updates to show</span></h3></div>
        ) : (
          <div className="log-timeline">
            {order.filter((b) => groups[b]).map((b) => {
              const col = isCollapsed(b);
              return (
                <div className="log-group" key={b}>
                  <button className="log-divider" onClick={() => setCollapsed((c) => Object.assign({}, c, { [b]: !isCollapsed(b) }))}>
                    <span className="log-node"></span>
                    <span className="log-date">{b}</span>
                    <span className="log-count">{groups[b].length}</span>
                    <svg className={"log-chev" + (col ? "" : " open")} width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M9 6l6 6-6 6"/></svg>
                  </button>
                  {!col && (
                    <div className="log-rows">
                      {groups[b].map((u) => <UpdateRow key={u.id} u={u} locale={locale} ctx={ctx} onDiff={openDiff} />)}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>

      <aside className="c4-side">
        <div className="side-card">
          <div className="side-head"><h3><span data-lang="ko">이번 주 요약</span><span data-lang="en">This week</span></h3><span className="side-ex"><span data-lang="ko">예시</span><span data-lang="en">sample</span></span></div>
          <div className="wk-row"><span className="row-type new_release" style={{ pointerEvents: "none" }}><span className="rt-dot"></span>{Market.tChrome("updates.new_release", locale)}</span><b>{typeCounts.new_release}</b></div>
          <div className="wk-row"><span className="row-type version_bump" style={{ pointerEvents: "none" }}><span className="rt-dot"></span>{Market.tChrome("updates.version_bump", locale)}</span><b>{typeCounts.version_bump}</b></div>
        </div>

        <div className="side-card">
          <div className="side-head"><h3><span data-lang="ko">검수 현황</span><span data-lang="en">Review status</span></h3></div>
          <button className={"pub-mix-row" + (reviewFilter === "audited" ? " on" : "")} onClick={() => setReviewFilter(reviewFilter === "audited" ? "all" : "audited")}>
            <ReviewChip audited={true} locale={locale} /><b>{reviewCounts.audited}</b>
          </button>
          <button className={"pub-mix-row" + (reviewFilter === "review" ? " on" : "")} onClick={() => setReviewFilter(reviewFilter === "review" ? "all" : "review")}>
            <ReviewChip audited={false} locale={locale} /><b>{reviewCounts.review}</b>
          </button>
          <div className="review-note"><span data-lang="ko">검토 중은 커뮤니티 기여를 포함합니다 — 등급이 아니라 검수 단계입니다.</span><span data-lang="en">In-review includes community contributions — it's a review stage, not a rank.</span></div>
        </div>
      </aside>

      <DiffPanel open={diffOpen} u={diffItem} locale={locale} onClose={() => setDiffOpen(false)} ctx={ctx} />
    </div>
  );
}

Object.assign(window, { AuthorCredit, ReviewChip, TypeLabel, VersionDelta, UpdateRow, DiffPanel, UpdatesScreen });
