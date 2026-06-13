/* Scopeball Market — Community (검증된 평가 + 자유 토론)
   기존 자산 재사용: Cloudy Pond 토큰, Market 데이터레이어, 카드/배지/칩, KO-EN.
   정직성: 모든 시드는 "예시" 배지. 이 별점은 카탈로그 카드 rating으로 역주입하지 않음. */

// 프로토타입 기준 시각
const CMTY_NOW = new Date("2026-06-03T00:00:00Z");

// ── Review 시드 (API 계약 모양) ──
// { id, kind, verified, rating(1~5, verified만), body{ko,en}, author, policySlug, createdAt, helpful }
const SEED_REVIEWS = [
  { id: "r1", kind: "verified", verified: true, rating: 5, author: "vault.eth", policySlug: "aave-hf-floor-warn", createdAt: "2026-05-28", helpful: 24,
    body: { ko: "HF 바닥 경고 덕분에 변동성 장에서 청산 직전 포지션을 정리했다. 임계값이 보수적이라 안심된다.", en: "The HF-floor warning let me trim a position right before liquidation in a volatile session. Conservative threshold — reassuring." } },
  { id: "r2", kind: "verified", verified: true, rating: 4, author: "0xharin", policySlug: "aave-hf-floor-warn", createdAt: "2026-05-19", helpful: 11,
    body: { ko: "유용하지만 가끔 너무 일찍 울린다. 임계값을 직접 조절할 수 있으면 좋겠다.", en: "Useful, though it sometimes fires too early. Wish the threshold were tunable." } },
  { id: "r3", kind: "verified", verified: true, rating: 5, author: "saltykimchi", policySlug: "air-permit-on-held-token-deny", createdAt: "2026-05-30", helpful: 31,
    body: { ko: "permit 드레인을 실제로 막아줬다. 서명 직전에 차단돼서 식은땀 흘렸다.", en: "Actually blocked a permit drain for me — stopped right at signing. Cold sweat." } },
  { id: "r4", kind: "verified", verified: true, rating: 5, author: "node_runner", policySlug: "air-permit-on-held-token-deny", createdAt: "2026-05-12", helpful: 9,
    body: { ko: "에어드랍 클레임 사칭 사이트에서 바로 작동했다. 필수.", en: "Triggered instantly on a fake claim site. Essential." } },
  { id: "r5", kind: "verified", verified: true, rating: 4, author: "frog.eth", policySlug: "nft-untrusted-blur-root-deny", createdAt: "2026-05-22", helpful: 7,
    body: { ko: "위조 마켓 서명을 잘 잡는다. 정상 Blur 거래엔 영향이 없었다.", en: "Catches spoofed market signatures well. No false positives on legit Blur trades." } },
  { id: "r6", kind: "verified", verified: true, rating: 5, author: "minteddao", policySlug: "unknown-blind-sign-warning", createdAt: "2026-05-26", helpful: 18,
    body: { ko: "블라인드 서명 경고는 모두가 켜야 한다. 하드웨어 지갑 쓸 때 특히.", en: "Everyone should enable the blind-sign warning, especially on a hardware wallet." } },
  { id: "r7", kind: "verified", verified: true, rating: 3, author: "lurking_anon", policySlug: "unknown-blind-sign-warning", createdAt: "2026-05-08", helpful: 4,
    body: { ko: "취지는 좋지만 dApp을 많이 쓰면 경고가 잦아 피로하다.", en: "Good intent, but heavy dApp users will see it a lot — alert fatigue." } },
  { id: "r8", kind: "verified", verified: true, rating: 4, author: "cowswapper", policySlug: "swap-price-impact-warn", createdAt: "2026-05-24", helpful: 14,
    body: { ko: "프라이스 임팩트를 서명 전에 숫자로 보여줘서 좋다. 얇은 풀에서 특히 유용.", en: "Shows price impact as a number before signing — great on thin pools." } },
  { id: "r9", kind: "verified", verified: true, rating: 5, author: "gasfeehater", policySlug: "gas-cost-usd-cap-deny", createdAt: "2026-05-29", helpful: 22,
    body: { ko: "가스비 상한 덕에 혼잡한 블록에서 말도 안 되는 수수료 트랜잭션을 막았다.", en: "The gas cap saved me from an absurd-fee transaction during a congested block." } },
  { id: "r10", kind: "verified", verified: true, rating: 4, author: "0xharin", policySlug: "nft-bid-weth-unlimited-warn", createdAt: "2026-05-15", helpful: 6,
    body: { ko: "무제한 WETH 입찰 승인을 경고해줘서 한도를 다시 설정했다.", en: "Warned me about an unlimited WETH bid approval — re-set it to a cap." } },
  { id: "r11", kind: "verified", verified: true, rating: 4, author: "merkletree", policySlug: "air-merkle-without-proof-warn", createdAt: "2026-05-10", helpful: 5,
    body: { ko: "증명 없는 클레임을 잡아낸다. 가끔 정상 클레임도 경고하지만 합리적.", en: "Catches proofless claims. Occasionally flags legit ones, but reasonable." } },
  { id: "r12", kind: "verified", verified: true, rating: 5, author: "chainhopper", policySlug: "bridge-target-not-allowlisted-deny", createdAt: "2026-05-27", helpful: 12,
    body: { ko: "허용목록 외 브릿지 타깃을 차단한다. 피싱 브릿지 UI에서 작동 확인.", en: "Blocks non-allowlisted bridge targets. Confirmed it works against a phishing bridge UI." } },
];

// 평가 작성 모달에서 고를 수 있는 정책(리뷰 없는 것 포함 → 빈 상태 시연)
const REVIEWABLE_EXTRA = ["aave-emode-leverage-warn", "ammlp-remove-exit-asymmetry-warn"];

// ── Discussion 시드 (스레드) ──
// { id, kind:'discussion', title{ko,en}, target{type,id}, body{ko,en}, author, createdAt, replies[] }
const SEED_THREADS = [
  { id: "t1", kind: "discussion", author: "cowswapper", createdAt: "2026-06-01", target: { type: "policy", id: "swap-price-impact-warn" }, status: "resolved", topics: ["question", "threshold"], helpful: 8,
    title: { ko: "슬리피지 가드, 어느 정도가 적정선일까?", en: "Slippage guard — what's a sane threshold?" },
    body: { ko: "프라이스 임팩트 경고 임계값을 다들 몇 %로 두는지 궁금합니다. 풀 깊이마다 다를 텐데 기준이 있을까요?", en: "Curious what % everyone sets the price-impact warning to. It must vary by pool depth — is there a rule of thumb?" },
    replies: [
      { author: "node_runner", createdAt: "2026-06-01", best: true, helpful: 6, body: { ko: "풀 깊이에 따라 다르지만 보통 0.5~1%로 시작합니다.", en: "Depends on pool depth, but I usually start at 0.5–1%." } },
      { author: "vault.eth", createdAt: "2026-06-02", body: { ko: "메인넷 대형 풀이면 0.3%도 충분하더라고요.", en: "On a deep mainnet pool 0.3% has been plenty for me." } },
      { author: "cowswapper", createdAt: "2026-06-02", body: { ko: "참고됐어요, 감사합니다.", en: "Super helpful, thanks both." } },
    ] },
  { id: "t2", kind: "discussion", author: "saltykimchi", createdAt: "2026-05-31", target: { type: "package", id: "drainer-shield" }, status: "pinned", topics: ["review"], helpful: 14,
    title: { ko: "드레이너·피싱 차단팩 실사용 후기 모음", en: "Drainer & Phishing Shield — field reports" },
    body: { ko: "이 팩 설치하고 한 달 썼습니다. 실제로 막힌 사례 있으면 공유해요.", en: "Ran this pack for a month. Share any real blocks you've seen." },
    replies: [
      { author: "minteddao", createdAt: "2026-05-31", best: true, helpful: 11, body: { ko: "가짜 에어드랍 사이트에서 permit 서명 차단됐습니다.", en: "Blocked a permit signature on a fake airdrop site." } },
      { author: "0xharin", createdAt: "2026-06-01", body: { ko: "Blur 위조 서명도 잡혔어요. 체감 효과 큽니다.", en: "Caught a spoofed Blur signature too. Noticeable difference." } },
    ] },
  { id: "t3", kind: "discussion", author: "frog.eth", createdAt: "2026-05-25", target: { type: "policy", id: "nft-bid-weth-unlimited-warn" }, status: null, topics: ["question", "threshold"], helpful: 5,
    title: { ko: "무제한 승인, 0으로 막는 건 너무 공격적일까?", en: "Is blocking unlimited approvals to zero too aggressive?" },
    body: { ko: "한도 승인으로 바꾸면 거래마다 서명이 늘어 불편한데, 다들 어떻게 타협하나요?", en: "Switching to capped approvals adds a signature per trade. How do you balance UX vs safety?" },
    replies: [
      { author: "gasfeehater", createdAt: "2026-05-26", best: true, helpful: 4, body: { ko: "자주 쓰는 마켓만 한도를 넉넉히 주고 나머진 0으로 둡니다.", en: "Generous cap on the markets I use often, zero everywhere else." } },
    ] },
  { id: "t4", kind: "discussion", author: "chainhopper", createdAt: "2026-05-20", target: { type: "policy", id: "bridge-target-not-allowlisted-deny" }, status: null, topics: ["question"], helpful: 2,
    title: { ko: "브릿지 허용목록은 어디서 관리되나요?", en: "Where is the bridge allowlist maintained?" },
    body: { ko: "허용목록 기준과 갱신 주기가 궁금합니다. 새 브릿지는 어떻게 등록되나요?", en: "Curious about the allowlist criteria and update cadence. How does a new bridge get added?" },
    replies: [] },
  { id: "t5", kind: "discussion", author: "lurking_anon", createdAt: "2026-05-18", target: { type: "package", id: "liq-pack" }, status: "resolved", topics: ["review", "question"], helpful: 9,
    title: { ko: "청산 방어팩 vs 개별 정책, 뭐가 나을까", en: "Liquidation pack vs picking policies individually" },
    body: { ko: "팩으로 통째로 담는 것과 필요한 것만 고르는 것, 운영상 차이가 큰가요?", en: "Is there a real operational difference between the whole pack and hand-picking?" },
    replies: [
      { author: "vault.eth", createdAt: "2026-05-19", best: true, helpful: 7, body: { ko: "팩이 업데이트 추적이 편합니다. 개별은 빠뜨리기 쉬워요.", en: "The pack is easier to keep updated. Hand-picking, you miss things." } },
      { author: "node_runner", createdAt: "2026-05-19", body: { ko: "저는 팩 담고 안 맞는 것만 빼는 식으로 씁니다.", en: "I add the pack and remove the few that don't fit." } },
    ] },
];

// ── 헬퍼 ──
function tt(obj, locale) { return obj ? (locale === "en" ? obj.en : obj.ko) : ""; }
function relTime(iso, locale) {
  const d = new Date(iso + (iso.length <= 10 ? "T00:00:00Z" : ""));
  const days = Math.max(0, Math.round((CMTY_NOW - d) / 86400000));
  if (locale === "en") {
    if (days <= 0) return "today";
    if (days === 1) return "1d ago";
    if (days < 7) return days + "d ago";
    if (days < 30) return Math.floor(days / 7) + "w ago";
    return Math.floor(days / 30) + "mo ago";
  }
  if (days <= 0) return "오늘";
  if (days < 7) return days + "일 전";
  if (days < 30) return Math.floor(days / 7) + "주 전";
  return Math.floor(days / 30) + "개월 전";
}
function authorInitial(a) { return (a.replace(/[^a-zA-Z0-9]/g, "")[0] || "?").toUpperCase(); }

// 표시이름 + @handle (너무 트위터스럽지 않게 절제)
const AUTHORS = {
  cowswapper: { n: "Cow Swapper" }, "vault.eth": { n: "Vault" }, "0xharin": { n: "Harin" },
  saltykimchi: { n: "Salty Kimchi" }, node_runner: { n: "Node Runner" }, minteddao: { n: "Minted" },
  "frog.eth": { n: "Frog" }, gasfeehater: { n: "Gas Fee Hater" }, merkletree: { n: "Merkle" },
  chainhopper: { n: "Chain Hopper" }, lurking_anon: { n: "Lurker" }, you: { n: "You" },
};
function authorName(h) { return (AUTHORS[h] && AUTHORS[h].n) || h; }
function authorHandle(h) { return "@" + h; }

// 아바타 톤 (Cloudy Pond 저채도 — 단색 원, 사진/이모지 없음)
const AV_TONES = [["#EBF3E8", "#44583D"], ["#DCEAED", "#2B3639"], ["#D7DBDF", "#2A3441"], ["#EDF4F6", "#485A5E"], ["#E4EFE1", "#354E2C"], ["#EFF0F2", "#475569"]];
function avatarTone(h) { let s = 0; for (let i = 0; i < h.length; i++) s += h.charCodeAt(i); return AV_TONES[s % AV_TONES.length]; }
function Avatar({ handle, size = 44 }) {
  const [bg, fg] = avatarTone(handle);
  return <span className="cav" style={{ width: size, height: size, fontSize: Math.round(size * 0.4), background: bg, color: fg }}>{authorInitial(handle)}</span>;
}
function VerifiedTick() {
  return <svg className="vtick" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg>;
}

// 리포스트 예시 카운트 (결정적 — 표기는 항상 "예시", 동작 비활성)
function repostCount(id) { let s = 0; for (let i = 0; i < id.length; i++) s = (s * 31 + id.charCodeAt(i)) % 211; return s % 19; }

// 로컬 저장 (북마크/도움 토글 — 새로고침 후에도 유지)
function lsGet(k, d) { try { const v = localStorage.getItem(k); return v ? JSON.parse(v) : d; } catch (e) { return d; } }
function lsSet(k, v) { try { localStorage.setItem(k, JSON.stringify(v)); } catch (e) {} }

// 액션 행 (트위터식 라인 아이콘 — 이모지 금지)
const ACT_ICONS = {
  reply: "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z",
  repost: "M17 2l4 4-4 4M21 6H7a4 4 0 0 0-4 4v1M7 22l-4-4 4-4M3 18h14a4 4 0 0 0 4-4v-1",
  like: "M20.8 5.6a5 5 0 0 0-7.1 0L12 7.3l-1.7-1.7a5 5 0 1 0-7.1 7.1L12 21.5l8.8-8.8a5 5 0 0 0 0-7.1z",
  bookmark: "M6 3h12a1 1 0 0 1 1 1v17l-7-4-7 4V4a1 1 0 0 1 1-1z",
  share: "M4 12v7a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-7M16 6l-4-4-4 4M12 2v13",
};
function ActBtn({ icon, label, count, active, disabled, tone, title, onClick }) {
  return (
    <button className={"act act-" + icon + (active ? " on" : "") + (disabled ? " off" : "")} title={title} onClick={(e) => { e.stopPropagation(); if (!disabled && onClick) onClick(); }}>
      <span className="act-ico">
        <svg width="18" height="18" viewBox="0 0 24 24" fill={active && (icon === "like" || icon === "bookmark" || icon === "repost") ? "currentColor" : "none"} stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d={ACT_ICONS[icon]}/></svg>
      </span>
      {count != null && <span className="act-n">{count}</span>}
      {label && <span className="act-label">{label}</span>}
    </button>
  );
}
function ActionRow({ id, replyCount, likeCount, liked, bookmarked, reposted, repostN, locale, onReply, onLike, onBookmark, onRepost, onShare, compact }) {
  return (
    <div className={"action-row" + (compact ? " compact" : "")}>
      <ActBtn icon="reply" count={replyCount} title={locale === "en" ? "Reply" : "답글"} onClick={onReply} />
      <ActBtn icon="repost" count={repostN != null ? repostN : repostCount(id)} active={reposted} title={locale === "en" ? "Repost" : "리포스트"} onClick={onRepost} />
      <ActBtn icon="like" count={likeCount} active={liked} title={locale === "en" ? "Helpful" : "도움"} onClick={onLike} />
      <ActBtn icon="bookmark" active={bookmarked} title={locale === "en" ? "Bookmark" : "북마크"} onClick={onBookmark} />
      <ActBtn icon="share" title={locale === "en" ? "Share link" : "공유 링크"} onClick={onShare} />
    </div>
  );
}

// 칩 행: 대상 + 주제태그, 최대 max개 + "+N"
function ChipsRow({ target, topics, locale, ctx, max }) {
  const chips = [];
  if (target) chips.push({ k: "t" });
  (topics || []).forEach((tp) => chips.push({ k: "tp", tp: tp }));
  if (chips.length === 0) return null;
  const lim = max || 2;
  const shown = chips.slice(0, lim);
  const extra = chips.length - shown.length;
  return (
    <div className="chips-row">
      {shown.map((ch, i) => ch.k === "t"
        ? <TargetChip key={i} target={target} locale={locale} ctx={ctx} neutral />
        : <TopicTag key={i} topic={ch.tp} locale={locale} />)}
      {extra > 0 && <span className="chip-more">+{extra}</span>}
    </div>
  );
}

// 별점 (중립 강조색 — 상태색 사용 금지)
function Stars({ value, size = 15 }) {
  const star = "M12 3l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 17.8 6.8 19.2l1-5.8L3.5 9.2l5.9-.9z";
  return (
    <span className="stars" style={{ display: "inline-flex", gap: 1 }}>
      {[0, 1, 2, 3, 4].map((i) => {
        const fill = Math.max(0, Math.min(1, value - i));
        const cls = fill >= 0.75 ? "full" : fill >= 0.25 ? "half" : "empty";
        return (
          <svg key={i} width={size} height={size} viewBox="0 0 24 24" className={"star " + cls}>
            <defs><linearGradient id={"sg" + i}><stop offset="50%" stopColor="currentColor" /><stop offset="50%" stopColor="transparent" /></linearGradient></defs>
            <path d={star} fill={cls === "full" ? "currentColor" : cls === "half" ? ("url(#sg" + i + ")") : "none"}
              stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" />
          </svg>
        );
      })}
    </span>
  );
}

function ExampleBadge({ locale }) {
  return <span className="ex-badge"><span data-lang="ko">예시</span><span data-lang="en">sample</span></span>;
}

// 자유 주제 태그 (무채색 칩)
const TOPIC = {
  question: { ko: "#질문", en: "#question" },
  review: { ko: "#후기", en: "#review" },
  threshold: { ko: "#임계값", en: "#threshold" },
};
function TopicTag({ topic, locale }) {
  const m = TOPIC[topic]; if (!m) return null;
  return <span className="topic-tag">{tt(m, locale)}</span>;
}
// 상태 칩 (이모지 금지 · 베이스 팔레트 저채도 · 상태색 금지)
function StatusChip({ status, locale }) {
  if (!status) return null;
  const lab = status === "pinned" ? { ko: "고정", en: "Pinned" } : { ko: "해결됨", en: "Resolved" };
  return <span className={"status-chip " + status}>{tt(lab, locale)}</span>;
}

// 공유 별점 표시: agg = {avg,count} 또는 null.
// 별 색은 중립 강조색(상태색 금지). "★4.x (N)" 동일 표기, 툴팁만 locale.
// variant: 'card'(컴팩트) | 'bar'(신뢰바). onClick 있으면 버튼.
function RatingInline({ agg, locale, onClick, variant }) {
  if (!agg) return null;
  const tip = locale === "en" ? (agg.count + " reviews") : (agg.count + "개 평가");
  const inner = (
    <React.Fragment>
      <svg className="ri-star" width={variant === "bar" ? 15 : 13} height={variant === "bar" ? 15 : 13} viewBox="0 0 24 24">
        <path d="M12 3l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 17.8 6.8 19.2l1-5.8L3.5 9.2l5.9-.9z" fill="currentColor" />
      </svg>
      <b className="ri-num">{agg.avg.toFixed(1)}</b>
      <span className="ri-n">({agg.count})</span>
      <ExampleBadge locale={locale} />
    </React.Fragment>
  );
  if (onClick) return <button className={"rating-inline " + (variant || "card")} title={tip} onClick={(e) => { e.stopPropagation(); onClick(); }}>{inner}</button>;
  return <span className={"rating-inline " + (variant || "card")} title={tip}>{inner}</span>;
}

// 대상(정책/패키지) 칩 → 클릭 시 상세로 이동. neutral=무채색(토론용)
function TargetChip({ target, locale, ctx, neutral }) {
  if (!target) return null;
  if (target.type === "package") {
    const pk = Market.PKG_BY_ID[target.id];
    if (!pk) return null;
    return <button className={"tgt-chip pkg" + (neutral ? " neutral" : "")} onClick={(e) => { e.stopPropagation(); ctx.openPackage(target.id); }}>
      <svg className="tc-ico" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M12 3l7 3v5c0 4-3 7-7 9-4-2-7-5-7-9V6z"/></svg>
      {Market.pick(pk.name, locale)}</button>;
  }
  const p = Market.BY_SLUG[target.id];
  if (!p) return null;
  const c = Market.DOMAIN_COLOR[p.domain];
  if (neutral) {
    return <button className="tgt-chip neutral" onClick={(e) => { e.stopPropagation(); ctx.openPolicy(target.id); }}>
      <span className="tc-dot neutral"></span>{Market.pick(p.name, locale)}</button>;
  }
  return <button className="tgt-chip" style={{ borderColor: c.hex, color: c.ink }} onClick={(e) => { e.stopPropagation(); ctx.openPolicy(target.id); }}>
    <span className="tc-dot" style={{ background: c.hex }}></span>{Market.pick(p.name, locale)}</button>;
}

// ── 별점 분포 요약 ──
function RatingSummary({ reviews, locale, subtitle }) {
  const n = reviews.length;
  const avg = n ? reviews.reduce((a, r) => a + r.rating, 0) / n : 0;
  const dist = [5, 4, 3, 2, 1].map((s) => reviews.filter((r) => r.rating === s).length);
  const max = Math.max(1, ...dist);
  return (
    <div className="rating-sum">
      <div className="rs-top">
        <div className="rs-avg">{avg.toFixed(1)}</div>
        <div className="rs-stars"><Stars value={avg} size={16} /><div className="rs-n">{n}<span data-lang="ko">개 평가</span><span data-lang="en"> reviews</span></div></div>
      </div>
      {subtitle && <div className="rs-sub">{subtitle}</div>}
      <div className="rs-bars">
        {[5, 4, 3, 2, 1].map((s, i) => (
          <div className="rs-row" key={s}>
            <span className="rs-label">{s}<svg width="11" height="11" viewBox="0 0 24 24"><path d="M12 3l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 17.8 6.8 19.2l1-5.8L3.5 9.2l5.9-.9z" fill="currentColor"/></svg></span>
            <span className="rs-track"><span className="rs-fill" style={{ width: (dist[i] / max * 100) + "%" }}></span></span>
            <span className="rs-c">{dist[i]}</span>
          </div>
        ))}
      </div>
      <div className="rs-note"><span data-lang="ko">예시 데이터 — 실제 리뷰 누적 시 detail에 자동 반영</span><span data-lang="en">Sample data — real reviews roll up into detail automatically</span></div>
    </div>
  );
}

// ── 통합 피드 카드 (검증 평가 / 라운지 글 공용, 트위터형) ──
function FeedCard({ item, kind, locale, ctx, onOpen, liked, likeCount, bookmarked, replyCount, reposted, repostN, onReply, onLike, onBookmark, onRepost, onShare, expanded }) {
  const isReview = kind === "review";
  const rc = replyCount != null ? replyCount : (item.replies || []).length;
  return (
    <article className={"tw-card" + (expanded ? " expanded" : "")} onClick={() => onOpen(item.id)}>
      <div className="tw-avcol"><Avatar handle={item.author} /></div>
      <div className="tw-main">
        <div className="tw-head">
          <span className="tw-name">{authorName(item.author)}</span>
          {isReview && <span className="tw-tick" title={locale === "en" ? "Verified reviewer" : "검증된 작성자"}><VerifiedTick /></span>}
          <span className="tw-handle">{authorHandle(item.author)}</span>
          <span className="tw-dot">·</span>
          <span className="tw-time">{relTime(item.createdAt, locale)}</span>
          {!isReview && item.status && <StatusChip status={item.status} locale={locale} />}
          <span className="tw-ex" title={locale === "en" ? "Sample data — not real metrics" : "예시 데이터 — 실제 지표 아님"}><span data-lang="ko">예시</span><span data-lang="en">sample</span></span>
        </div>
        {isReview && <div className="tw-stars"><Stars value={item.rating} size={15} /></div>}
        {!isReview && item.title && <div className="tw-title">{tt(item.title, locale)}</div>}
        <div className="tw-body">{tt(item.body, locale)}</div>
        <ChipsRow target={isReview ? { type: "policy", id: item.policySlug } : item.target} topics={item.topics} locale={locale} ctx={ctx} max={2} />
        <ActionRow id={item.id} replyCount={rc} likeCount={likeCount} liked={liked} bookmarked={bookmarked} reposted={reposted} repostN={repostN} locale={locale}
          onReply={() => onReply ? onReply(item.id) : onOpen(item.id)} onLike={onLike} onBookmark={onBookmark} onRepost={onRepost} onShare={onShare} />
      </div>
    </article>
  );
}

// 리포스트 라벨 (피드 상단)
function RepostLabel({ author, locale }) {
  return (
    <div className="repost-label">
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round"><path d="M17 2l4 4-4 4M21 6H7a4 4 0 0 0-4 4v1M7 22l-4-4 4-4M3 18h14a4 4 0 0 0 4-4v-1"/></svg>
      <span>{authorName(author)}<span data-lang="ko">님이 리포스트함</span><span data-lang="en"> reposted</span></span>
    </div>
  );
}

// 인라인 미니 답글 입력
function MiniComposer({ locale, onSubmit, onCancel }) {
  const [t, setT] = useState("");
  return (
    <div className="mini-composer">
      <textarea value={t} rows={1} placeholder={locale === "en" ? "Write a reply" : "답글을 입력하세요"}
        onChange={(e) => setT(e.target.value)} onInput={(e) => { e.target.style.height = "auto"; e.target.style.height = e.target.scrollHeight + "px"; }} autoFocus />
      <div className="mc-row">
        <button className="addbtn ghost sm" onClick={onCancel}><span data-lang="ko">취소</span><span data-lang="en">Cancel</span></button>
        <button className="addbtn sm" disabled={!t.trim()} onClick={() => { if (t.trim()) { onSubmit(t.trim()); setT(""); } }}><span data-lang="ko">답글</span><span data-lang="en">Reply</span></button>
      </div>
    </div>
  );
}

// 답글 노드 (대댓글 2단계까지)
function ReplyNode({ reply, depth, childrenOf, locale, replyLikes, onLikeReply, replyingId, setReplyingId, onAddReply }) {
  const kids = depth === 0 ? childrenOf(reply.id) : [];
  const liked = !!replyLikes[reply.id];
  const baseHelp = typeof reply.helpful === "number" ? reply.helpful : 0;
  const targetParent = depth === 0 ? reply.id : (reply.parentId || reply.id);
  return (
    <div className={"tw-reply d" + depth}>
      <div className="re-avcol"><Avatar handle={reply.author} size={34} /></div>
      <div className="re-main">
        {reply.best && <span className="best-label"><span data-lang="ko">베스트 답글</span><span data-lang="en">Best answer</span></span>}
        <div className="re-head">
          <span className="tw-name">{authorName(reply.author)}</span>
          <span className="tw-handle">{authorHandle(reply.author)}</span>
          <span className="tw-dot">·</span><span className="tw-time">{relTime(reply.createdAt, locale)}</span>
        </div>
        <div className="tw-body">{tt(reply.body, locale)}</div>
        <div className="re-actions">
          <button className="re-act" onClick={() => setReplyingId(replyingId === reply.id ? null : reply.id)}>
            <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>
            <span data-lang="ko">답글 달기</span><span data-lang="en">Reply</span>
          </button>
          <button className={"re-act" + (liked ? " on" : "")} onClick={() => onLikeReply(reply.id)}>
            <svg width="15" height="15" viewBox="0 0 24 24" fill={liked ? "currentColor" : "none"} stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M20.8 5.6a5 5 0 0 0-7.1 0L12 7.3l-1.7-1.7a5 5 0 1 0-7.1 7.1L12 21.5l8.8-8.8a5 5 0 0 0 0-7.1z"/></svg>
            <span data-lang="ko">도움 {baseHelp + (liked ? 1 : 0)}</span><span data-lang="en">Helpful {baseHelp + (liked ? 1 : 0)}</span>
          </button>
        </div>
        {replyingId === reply.id && <MiniComposer locale={locale} onSubmit={(txt) => { onAddReply(txt, targetParent); setReplyingId(null); }} onCancel={() => setReplyingId(null)} />}
        {kids.length > 0 && <div className="re-children">{kids.map((k) => <ReplyNode key={k.id} reply={k} depth={1} childrenOf={childrenOf} locale={locale} replyLikes={replyLikes} onLikeReply={onLikeReply} replyingId={replyingId} setReplyingId={setReplyingId} onAddReply={onAddReply} />)}</div>}
      </div>
    </div>
  );
}

// 답글 스레드 (베스트 상단 고정)
function RepliesThread({ replies, locale, replyLikes, onLikeReply, replyingId, setReplyingId, onAddReply }) {
  const childrenOf = (pid) => replies.filter((r) => (r.parentId || null) === (pid || null));
  const top = childrenOf(null).slice().sort((a, b) => (b.best ? 1 : 0) - (a.best ? 1 : 0));
  if (top.length === 0) return <div className="tw-noreply"><span data-lang="ko">아직 답글이 없습니다. 첫 답글을 남겨보세요.</span><span data-lang="en">No replies yet. Be the first to reply.</span></div>;
  return (
    <div className="tw-replies inline">
      {top.map((r) => <ReplyNode key={r.id} reply={r} depth={0} childrenOf={childrenOf} locale={locale} replyLikes={replyLikes} onLikeReply={onLikeReply} replyingId={replyingId} setReplyingId={setReplyingId} onAddReply={onAddReply} />)}
    </div>
  );
}

Object.assign(window, {
  SEED_REVIEWS, SEED_THREADS, REVIEWABLE_EXTRA, CMTY_NOW,
  tt, relTime, authorInitial, authorName, authorHandle, avatarTone, Avatar, VerifiedTick,
  repostCount, lsGet, lsSet, ActionRow, ActBtn, ChipsRow,
  Stars, ExampleBadge, RatingInline, TargetChip,
  TopicTag, StatusChip, RatingSummary, FeedCard,
  RepostLabel, MiniComposer, ReplyNode, RepliesThread,
});
