/* Scopeball Market — Community (2단: 피드 + 위젯 스택, 인라인 아코디언) */

function WriteModal({ open, locale, onClose, onSubmitThread, fireToast }) {
  const [title, setTitle] = useState("");
  const [bodyText, setBodyText] = useState("");
  const [target, setTarget] = useState("");
  useEffect(() => { if (open) { setTitle(""); setBodyText(""); setTarget(""); } }, [open]);
  if (!open) return null;
  function submit() {
    if (!title.trim() && !bodyText.trim()) return;
    let tgt = null;
    if (target) { const [type, id] = target.split(":"); tgt = { type, id }; }
    onSubmitThread({ title: { ko: title || "(제목 없음)", en: title || "(untitled)" }, body: { ko: bodyText, en: bodyText }, target: tgt });
    onClose();
    fireToast && fireToast(locale === "en" ? "Posted (prototype)" : "게시했어요 (프로토타입)");
  }
  return (
    <div className="cmty-modal-scrim" onClick={onClose}>
      <div className="cmty-modal" onClick={(e) => e.stopPropagation()}>
        <div className="cm-head">
          <h3><span data-lang="ko">새 글 쓰기</span><span data-lang="en">New post</span></h3>
          <button className="x" onClick={onClose}><Ico d={ICONS.x} w={18} /></button>
        </div>
        <div className="cm-body">
          <label className="cm-label"><span data-lang="ko">제목</span><span data-lang="en">Title</span></label>
          <input className="cm-input" value={title} onChange={(e) => setTitle(e.target.value)} placeholder={locale === "en" ? "What's your question or note?" : "질문이나 이야깃거리를 적어보세요"} />
          <label className="cm-label"><span data-lang="ko">대상 (선택)</span><span data-lang="en">Target (optional)</span></label>
          <select className="cm-input" value={target} onChange={(e) => setTarget(e.target.value)}>
            <option value="">{locale === "en" ? "None" : "대상 없음"}</option>
            <optgroup label={locale === "en" ? "Packages" : "패키지"}>
              {Market.PACKAGES.map((pk) => <option key={pk.id} value={"package:" + pk.id}>{Market.pick(pk.name, locale)}</option>)}
            </optgroup>
            <optgroup label={locale === "en" ? "Policies" : "정책"}>
              {SEED_REVIEWS.map((r) => r.policySlug).filter((s, i, a) => a.indexOf(s) === i).map((s) => <option key={s} value={"policy:" + s}>{Market.pick(Market.BY_SLUG[s].name, locale)}</option>)}
            </optgroup>
          </select>
          <label className="cm-label"><span data-lang="ko">내용</span><span data-lang="en">Body</span></label>
          <textarea className="cm-input cm-textarea" value={bodyText} onChange={(e) => setBodyText(e.target.value)} placeholder={locale === "en" ? "Write your post…" : "내용을 입력하세요…"}></textarea>
          <div className="cm-note"><span data-lang="ko">프로토타입 — 게시물은 이 세션에만 저장됩니다.</span><span data-lang="en">Prototype — posts live only in this session.</span></div>
          <div className="cm-actions">
            <button className="addbtn ghost" onClick={onClose}><span data-lang="ko">닫기</span><span data-lang="en">Close</span></button>
            <button className="addbtn" onClick={submit}><span data-lang="ko">게시</span><span data-lang="en">Post</span></button>
          </div>
        </div>
      </div>
    </div>
  );
}

function Composer({ locale, onSubmit }) {
  const [txt, setTxt] = useState("");
  return (
    <div className="tw-composer inline">
      <Avatar handle="you" size={38} />
      <div className="tw-cinput">
        <textarea value={txt} rows={1} placeholder={locale === "en" ? "Post your reply" : "답글을 남겨보세요"}
          onChange={(e) => setTxt(e.target.value)} onInput={(e) => { e.target.style.height = "auto"; e.target.style.height = e.target.scrollHeight + "px"; }} />
        <div className="tw-crow">
          <span className="tw-cnote"><span data-lang="ko">프로토타입 · 이 세션에만 저장</span><span data-lang="en">Prototype · this session only</span></span>
          <button className="addbtn sm" disabled={!txt.trim()} onClick={() => { if (txt.trim()) { onSubmit(txt.trim()); setTxt(""); } }}>
            <span data-lang="ko">답글</span><span data-lang="en">Reply</span>
          </button>
        </div>
      </div>
    </div>
  );
}

function CommunityScreen({ locale, ctx, fireToast, initialSlug }) {
  const [cfilter, setCfilter] = useState("all");
  const [csort, setCsort] = useState("recent");
  const [expandedId, setExpandedId] = useState(null);
  const [replyingId, setReplyingId] = useState(null);
  const [modal, setModal] = useState(false);
  const [likes, setLikes] = useState(() => lsGet("mk_likes", {}));
  const [bookmarks, setBookmarks] = useState(() => lsGet("mk_bookmarks", {}));
  const [reposts, setReposts] = useState(() => lsGet("mk_reposts", {}));
  const [replyLikes, setReplyLikes] = useState({});
  const [threads, setThreads] = useState(SEED_THREADS.map((t) => Object.assign({ _seed: true }, t)));
  const [replyMap, setReplyMap] = useState({});
  useEffect(() => { if (initialSlug) { setCfilter("trend:policy:" + initialSlug); setExpandedId(null); } }, [initialSlug]);

  function persistToggle(setter, key, id, msgOn, msgOff) {
    setter((s) => { const n = Object.assign({}, s, { [id]: !s[id] }); lsSet(key, n); if (msgOn != null) fireToast && fireToast(n[id] ? msgOn : msgOff); return n; });
  }
  const toggleLike = (id) => persistToggle(setLikes, "mk_likes", id);
  const toggleBookmark = (id) => persistToggle(setBookmarks, "mk_bookmarks", id, locale === "en" ? "Saved" : "내 저장에 담았어요", locale === "en" ? "Removed" : "저장에서 뺐어요");
  const toggleRepost = (id) => persistToggle(setReposts, "mk_reposts", id, locale === "en" ? "Reposted to your lounge" : "리포스트했어요", locale === "en" ? "Repost removed" : "리포스트 취소");
  const share = () => fireToast && fireToast(locale === "en" ? "Link copied" : "링크를 복사했어요");
  const onLikeReply = (rid) => setReplyLikes((s) => Object.assign({}, s, { [rid]: !s[rid] }));

  function likeCountOf(item) { return (item.helpful || 0) + (likes[item.id] ? 1 : 0); }
  function repostNOf(item) { return repostCount(item.id) + (reposts[item.id] ? 1 : 0); }
  function repliesOf(item) {
    const seed = (item.replies || []).map((r, i) => Object.assign({}, r, { id: item.id + "-s" + i, parentId: null }));
    return seed.concat(replyMap[item.id] || []);
  }
  function replyCountOf(item) { return repliesOf(item).length; }
  function addReply(itemId, text, parentId) {
    setReplyMap((m) => {
      const arr = (m[itemId] || []).concat({ id: "u" + Date.now() + Math.random().toString(36).slice(2, 5), parentId: parentId || null, author: "you", createdAt: "2026-06-03", body: { ko: text, en: text } });
      return Object.assign({}, m, { [itemId]: arr });
    });
    fireToast && fireToast(locale === "en" ? "Reply posted" : "답글을 남겼어요");
  }
  function addThread(t) {
    const nt = Object.assign({ id: "u" + Date.now(), kind: "discussion", author: "you", createdAt: "2026-06-03", replies: [], topics: [], status: null, helpful: 0, _seed: false }, t);
    setThreads((prev) => [nt].concat(prev));
    setCfilter("lounge"); setExpandedId(nt.id);
  }
  function toggleExpand(id) { setExpandedId((x) => x === id ? null : id); setReplyingId(null); }

  // ── 통합 아이템 ──
  let all = [];
  SEED_REVIEWS.forEach((r) => all.push({ id: r.id, kind: "review", item: r }));
  threads.forEach((t) => all.push({ id: t.id, kind: "post", item: t }));
  function activityAt(e) { if (e.kind === "post") { const rs = repliesOf(e.item); return rs.length ? rs.slice(-1)[0].createdAt : e.item.createdAt; } return e.item.createdAt; }
  function targetId(e) { return e.kind === "review" ? e.item.policySlug : (e.item.target ? e.item.target.id : null); }

  // ── 지금 주목받는 정책 (리뷰+댓글 신호) ──
  function trending() {
    const map = {};
    function bump(id, type, rv, cm) { if (!id) return; if (!map[id]) map[id] = { id, type, rv: 0, cm: 0 }; map[id].rv += rv; map[id].cm += cm; }
    SEED_REVIEWS.forEach((r) => bump(r.policySlug, "policy", 1, 0));
    threads.forEach((t) => { if (t.target) bump(t.target.id, t.target.type, 0, replyCountOf(t)); });
    const rows = Object.keys(map).map((id) => {
      const m = map[id]; const isPkg = m.type === "package";
      const obj = isPkg ? Market.PKG_BY_ID[id] : Market.BY_SLUG[id];
      if (!obj) return null;
      return { id, type: m.type, name: obj.name, domain: isPkg ? obj.primaryDomain : obj.domain, rv: m.rv, cm: m.cm, score: m.rv * 2 + m.cm };
    }).filter(Boolean).sort((a, b) => b.score - a.score).slice(0, 5);
    return rows;
  }
  const trend = trending();
  const maxScore = Math.max(1, ...trend.map((t) => t.score));

  // ── 필터 ──
  let items = all.slice();
  const special = cfilter.indexOf("trend:") === 0 ? "trend" : (cfilter === "saved" || cfilter === "reposts" ? cfilter : null);
  if (cfilter === "saved") items = items.filter((e) => bookmarks[e.id]);
  else if (cfilter === "reposts") items = items.filter((e) => reposts[e.id]);
  else if (special === "trend") { const tid = cfilter.split(":")[2]; items = items.filter((e) => targetId(e) === tid); }
  else if (cfilter === "review") items = items.filter((e) => e.kind === "review");
  else if (cfilter === "question") items = items.filter((e) => e.kind === "post" && (e.item.topics || []).indexOf("question") >= 0);
  else if (cfilter === "note") items = items.filter((e) => e.kind === "post" && (e.item.topics || []).indexOf("review") >= 0);
  else if (cfilter === "lounge") items = items.filter((e) => e.kind === "post");
  else if (cfilter === "unresolved") items = items.filter((e) => e.kind === "post" && e.item.status !== "resolved");
  // 리포스트한 글을 상단으로 (전체/라운지 한정)
  items.sort((a, b) => csort === "popular" ? likeCountOf(b.item) - likeCountOf(a.item) : new Date(activityAt(b)) - new Date(activityAt(a)));
  if (!special && (cfilter === "all" || cfilter === "lounge")) {
    items.sort((a, b) => (reposts[b.id] ? 1 : 0) - (reposts[a.id] ? 1 : 0));
  }

  const SEGS = [["all", { ko: "전체", en: "All" }], ["review", { ko: "평가", en: "Reviews" }], ["question", { ko: "질문", en: "Questions" }], ["note", { ko: "후기", en: "Notes" }], ["unresolved", { ko: "미해결", en: "Unresolved" }]];
  const savedN = all.filter((e) => bookmarks[e.id]).length;
  const repostN = all.filter((e) => reposts[e.id]).length;
  const reviews = SEED_REVIEWS;
  const n = reviews.length; const avg = n ? reviews.reduce((a, r) => a + r.rating, 0) / n : 0;
  const recent = threads.slice().sort((a, b) => new Date(b.createdAt) - new Date(a.createdAt)).slice(0, 4);
  const unresolvedN = threads.filter((p) => p.status !== "resolved").length;
  const specialLabel = cfilter === "saved" ? { ko: "내 저장", en: "Saved" } : cfilter === "reposts" ? { ko: "내 리포스트", en: "Reposts" } : (special === "trend" ? { ko: "주목 정책", en: "Trending" } : null);

  function feedCardProps(item) {
    return { liked: !!likes[item.id], likeCount: likeCountOf(item), bookmarked: !!bookmarks[item.id], reposted: !!reposts[item.id], repostN: repostNOf(item),
      onLike: () => toggleLike(item.id), onBookmark: () => toggleBookmark(item.id), onRepost: () => toggleRepost(item.id), onShare: share };
  }

  return (
    <div className="cmty4">
      <div className="c4-feed">
        <div className="c4-toolbar">
          <div className="seg-pills">
            {SEGS.map(([k, lab]) => (
              <button key={k} className={"seg-pill" + (cfilter === k ? " on" : "")} onClick={() => { setCfilter(k); setExpandedId(null); }}>{tt(lab, locale)}</button>
            ))}
            {specialLabel && (
              <button className="seg-pill special on" onClick={() => setCfilter("all")}>{tt(specialLabel, locale)}<Ico d={ICONS.x} w={12} /></button>
            )}
          </div>
          <div className="c4-tr">
            <div className="sortsel">
              <button className={"mini-sort" + (csort === "recent" ? " on" : "")} onClick={() => setCsort("recent")}><span data-lang="ko">최근</span><span data-lang="en">Latest</span></button>
              <button className={"mini-sort" + (csort === "popular" ? " on" : "")} onClick={() => setCsort("popular")}><span data-lang="ko">인기</span><span data-lang="en">Top</span></button>
            </div>
            <button className="addbtn" onClick={() => setModal(true)}><Ico d={ICONS.plus} w={15} /><span data-lang="ko">글쓰기</span><span data-lang="en">Post</span></button>
          </div>
        </div>

        {items.length === 0 ? (
          <div className="cv-empty">
            <h3>{cfilter === "saved" ? <span><span data-lang="ko">저장한 글이 없어요</span><span data-lang="en">Nothing saved yet</span></span> : cfilter === "reposts" ? <span><span data-lang="ko">리포스트한 글이 없어요</span><span data-lang="en">No reposts yet</span></span> : <span><span data-lang="ko">아직 글이 없어요</span><span data-lang="en">Nothing here yet</span></span>}</h3>
            <p><span data-lang="ko">카드의 북마크·리포스트를 누르면 여기 모입니다.</span><span data-lang="en">Bookmark or repost a card to collect it here.</span></p>
          </div>
        ) : (
          <div className="tw-feed dense">
            {items.map((e) => (
              <div className="feed-item" key={e.kind + e.id}>
                {reposts[e.id] && <RepostLabel author="you" locale={locale} />}
                <FeedCard item={e.item} kind={e.kind} locale={locale} ctx={ctx} replyCount={replyCountOf(e.item)} expanded={expandedId === e.id}
                  onOpen={toggleExpand} onReply={toggleExpand} {...feedCardProps(e.item)} />
                {expandedId === e.id && (
                  <div className="accordion">
                    <div className="acc-head"><span data-lang="ko">댓글 {replyCountOf(e.item)}</span><span data-lang="en">{replyCountOf(e.item)} comments</span></div>
                    <Composer locale={locale} onSubmit={(t) => addReply(e.id, t, null)} />
                    <RepliesThread replies={repliesOf(e.item)} locale={locale} replyLikes={replyLikes} onLikeReply={onLikeReply}
                      replyingId={replyingId} setReplyingId={setReplyingId} onAddReply={(txt, pid) => addReply(e.id, txt, pid)} />
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      <aside className="c4-side">
        {/* 지금 주목받는 정책 */}
        <div className="side-card">
          <div className="side-head">
            <h3><span data-lang="ko">지금 주목받는 정책</span><span data-lang="en">Trending now</span></h3>
            <span className="side-ex"><span data-lang="ko">예시</span><span data-lang="en">sample</span></span>
          </div>
          <div className="trend-list">
            {trend.map((t, i) => {
              const c = Market.DOMAIN_COLOR[t.domain] || Market.DOMAIN_COLOR.security;
              return (
                <button className="trend-row" key={t.id} onClick={() => { setCfilter("trend:" + t.type + ":" + t.id); setExpandedId(null); }}>
                  <span className="trend-rank">{i + 1}</span>
                  <span className="trend-dot" style={{ background: c.hex }}></span>
                  <span className="trend-txt">
                    <span className="trend-name">{Market.pick(t.name, locale)}</span>
                    <span className="trend-meta"><span data-lang="ko">리뷰 +{t.rv} · 댓글 +{t.cm}</span><span data-lang="en">+{t.rv} reviews · +{t.cm} replies</span></span>
                  </span>
                  <span className="trend-spark" title={locale === "en" ? "rising" : "상승"}>
                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round"><path d="M6 15l6-6 6 6"/></svg>
                    <span className="spark-bar" style={{ height: (5 + (t.score / maxScore) * 13) + "px" }}></span>
                  </span>
                </button>
              );
            })}
          </div>
        </div>

        {/* 검증된 평가 요약 */}
        <div className="side-card">
          <div className="side-head">
            <h3><span data-lang="ko">검증된 평가</span><span data-lang="en">Verified reviews</span></h3>
            <button className="side-more" onClick={() => { setCfilter("review"); setExpandedId(null); }}><span data-lang="ko">전체</span><span data-lang="en">All</span></button>
          </div>
          <div className="side-rating">
            <span className="sr-avg">{avg.toFixed(1)}</span>
            <div className="sr-meta"><Stars value={avg} size={14} /><div className="sr-n">{n}<span data-lang="ko">개 · 예시</span><span data-lang="en"> · sample</span></div></div>
          </div>
        </div>

        {/* 라운지 미해결 */}
        <div className="side-card">
          <div className="side-head">
            <h3><span data-lang="ko">라운지</span><span data-lang="en">Lounge</span></h3>
            <button className="side-more" onClick={() => { setCfilter("lounge"); setExpandedId(null); }}><span data-lang="ko">전체</span><span data-lang="en">All</span></button>
          </div>
          {unresolvedN > 0 && <button className="side-flag" onClick={() => { setCfilter("unresolved"); setExpandedId(null); }}><span data-lang="ko">미해결 {unresolvedN}</span><span data-lang="en">{unresolvedN} unresolved</span></button>}
          <div className="side-list">
            {recent.map((t) => (
              <button className="side-row col" key={t.id} onClick={() => { setExpandedId(t.id); }}>
                <div className="sp-title">{tt(t.title, locale)}</div>
                <div className="sp-meta">{authorName(t.author)}<span className="th-dot">·</span><span data-lang="ko">답글 {replyCountOf(t)}</span><span data-lang="en">{replyCountOf(t)} replies</span></div>
              </button>
            ))}
          </div>
        </div>

        {/* 내 저장 / 리포스트 */}
        <div className="side-card">
          <div className="side-head"><h3><span data-lang="ko">내 컬렉션</span><span data-lang="en">My collection</span></h3></div>
          <div className="mine-row">
            <button className={"mine-btn" + (cfilter === "saved" ? " on" : "")} onClick={() => { setCfilter("saved"); setExpandedId(null); }}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill={cfilter === "saved" ? "currentColor" : "none"} stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M6 3h12a1 1 0 0 1 1 1v17l-7-4-7 4V4a1 1 0 0 1 1-1z"/></svg>
              <span><span data-lang="ko">내 저장</span><span data-lang="en">Saved</span></span><b>{savedN}</b>
            </button>
            <button className={"mine-btn" + (cfilter === "reposts" ? " on" : "")} onClick={() => { setCfilter("reposts"); setExpandedId(null); }}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"><path d="M17 2l4 4-4 4M21 6H7a4 4 0 0 0-4 4v1M7 22l-4-4 4-4M3 18h14a4 4 0 0 0 4-4v-1"/></svg>
              <span><span data-lang="ko">내 리포스트</span><span data-lang="en">Reposts</span></span><b>{repostN}</b>
            </button>
          </div>
        </div>
      </aside>

      <WriteModal open={modal} locale={locale} onClose={() => setModal(false)} onSubmitThread={addThread} fireToast={fireToast} />
    </div>
  );
}

Object.assign(window, { WriteModal, Composer, CommunityScreen });
