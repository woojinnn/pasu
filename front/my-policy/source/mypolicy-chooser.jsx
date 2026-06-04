// mypolicy-chooser.jsx — 새 정책 만들기 chooser (폼 / 블록 / Cedar 비교 카드)
// 각 카드: 한 줄 요약 + 추천 대상 + 미니 미리보기 + 장단점. 한 정책 = 한 방식 고정.

const CH_DATA = (loc) => [
  {
    key: "form", accent: "cyan", icon: MPI.form(),
    title: loc === "en" ? "Build with a form" : "폼으로 만들기",
    summary: loc === "en"
      ? "Easiest · common shapes (forbid + AND) · .cedar and manifest auto-generated · just change the threshold."
      : "가장 쉬움 · 흔한 정책(forbid + AND) · .cedar와 manifest 자동 생성 · 임계값만 바꾸면 끝.",
    rec: loc === "en" ? "First-timers · standard policies" : "처음·표준 정책",
    pros: loc === "en" ? ["Round-trip safety net", "Auto cedar + manifest", "Inline value editing"] : ["round-trip 안전망", "cedar·manifest 자동", "인라인 값 편집"],
    cons: loc === "en" ? ["Complex policies (OR · nesting) may not open as a form"] : ["복잡한 정책(OR·중첩 등)은 폼으로 안 열릴 수 있어요"],
    preview: "form"
  },
  {
    key: "block", accent: "sage", icon: MPI.blocks(),
    title: loc === "en" ? "Build with blocks" : "블록으로 만들기",
    summary: loc === "en"
      ? "Visually assemble complex conditions — OR · has · nesting — covering the full Cedar expression."
      : "OR·has·중첩 등 복잡한 조건까지 시각적으로 조립 · 전체 Cedar 표현.",
    rec: loc === "en" ? "Complex logic" : "복잡한 로직",
    pros: loc === "en" ? ["OR · has · set", "Visual assembly", "Full AST — superset"] : ["OR · has · set", "시각적 조립", "전체 AST(만능)"],
    cons: loc === "en" ? ["Slower than a form"] : ["폼보다 손이 감"],
    preview: "block"
  },
  {
    key: "cedar", accent: "slate", icon: MPI.shield(),
    title: loc === "en" ? "Write in Cedar" : "Cedar로 만들기",
    summary: loc === "en"
      ? "Write code directly · maximum freedom, minimal guards · outside the form safety net · for experienced users."
      : "코드 직접 작성 · 최대 자유, 가드 최소 · 폼 안전망 밖 · 숙련자용.",
    rec: loc === "en" ? "People who know Cedar" : "Cedar를 아는 사람",
    pros: loc === "en" ? ["Maximum freedom", "Manifest hand-managed"] : ["최대 자유", "manifest 직접 관리"],
    cons: loc === "en" ? ["Minimal guards", "Outside form round-trip"] : ["가드 최소", "폼 안전망 밖"],
    preview: "cedar"
  }
];

function ChooserPreview({ kind }) {
  if (kind === "form") return (
    <div className="mpc-prev form">
      <div className="mpc-prev-row"><span className="cap" /><span className="fld" /><span className="op">&gt;</span><span className="val">150</span></div>
      <div className="mpc-prev-and">AND</div>
      <div className="mpc-prev-row"><span className="cap" /><span className="fld w2" /><span className="op">≠</span><span className="val ref">self</span></div>
    </div>
  );
  if (kind === "block") return (
    <div className="mpc-prev block">
      <div className="mpc-prev-hat" />
      <div className="mpc-prev-or">
        <span className="spine" />
        <div className="mpc-prev-chip" /><div className="mpc-prev-chip w2" />
      </div>
    </div>
  );
  return (
    <div className="mpc-prev cedar">
      <div className="ln"><span className="g" /><span className="t kw" /></div>
      <div className="ln"><span className="g" /><span className="t" /></div>
      <div className="ln"><span className="g" /><span className="t guard" /></div>
      <div className="ln"><span className="g" /><span className="t s" /></div>
    </div>
  );
}

function NewPolicyChooser({ loc, onPick, onClose }) {
  const cards = CH_DATA(loc);
  return (
    <div className="mp-modal-bd" onClick={onClose}>
      <div className="mpc-modal" onClick={e => e.stopPropagation()}>
        <div className="mpc-h">
          <div>
            <div className="t">{loc === "en" ? "Create a new policy" : "새 정책 만들기"}</div>
            <div className="s">{loc === "en"
              ? "Pick how to start. All three save to the same Cedar, and you can view a policy in other ways later (form only for simple policies)."
              : "어떤 방식으로 시작할지 고르세요. 셋 다 같은 Cedar로 저장되고, 나중에 다른 방식으로도 볼 수 있어요 (폼은 단순한 정책만)."}</div>
          </div>
          <button className="mpc-x" onClick={onClose}>{MPI.x()}</button>
        </div>
        <div className="mpc-grid">
          {cards.map(c => (
            <button key={c.key} className={`mpc-card ${c.accent}`} onClick={() => onPick(c.key)}>
              <div className="mpc-card-top">
                <span className="mpc-ic">{c.icon}</span>
                <span className="mpc-title">{c.title}</span>
              </div>
              <ChooserPreview kind={c.preview} />
              <div className="mpc-summary">{c.summary}</div>
              <div className="mpc-rec"><span className="lbl">{loc === "en" ? "Best for" : "추천"}</span>{c.rec}</div>
              <div className="mpc-pc">
                <ul className="pros">{c.pros.map((p, i) => <li key={i}>{MPI.check()}{p}</li>)}</ul>
                <ul className="cons">{c.cons.map((p, i) => <li key={i}>{MPI.x()}{p}</li>)}</ul>
              </div>
              <span className="mpc-go">{loc === "en" ? "Start here" : "이 방식으로 시작"}{MPI.caret()}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { NewPolicyChooser });
