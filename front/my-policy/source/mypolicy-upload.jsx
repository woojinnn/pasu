// mypolicy-upload.jsx — 큐레이션 → 마켓 업로드 (순환 구조)
// 흐름: ① 비식별 확인(주소류=강제 hole / 숫자=추천값 남길지 선택) → ② 이름·설명 → ③ 공개
// 강제·우회 불가: 주소류 식별자(지갑·recipient·delegatee·allowlist)는 무조건 파라미터 구멍으로 비움.

const { useState: useSU, useMemo: useMU } = React;

// 패키지 멤버 → de-id 파라미터 수집
function collectParams(pkg) {
  const out = [];
  pkg.members.forEach(m => {
    const d = MP.detailFor(m);
    const params = (d && d.params) || [];
    if (!params.length) return;
    out.push({ member: m, params });
  });
  return out;
}

function UploadModal({ pkg, loc, onClose, toast }) {
  const groups = useMU(() => collectParams(pkg), [pkg]);
  const [step, setStep] = useSU(0);
  // 숫자 파라미터: keep 추천값 여부 (기본 false = 비움)
  const [keep, setKeep] = useSU({});
  const [name, setName] = useSU(MP.t(pkg.name, loc));
  const [desc, setDesc] = useSU(MP.t(pkg.tagline, loc));

  const addrCount = useMU(() => groups.reduce((a, g) => a + g.params.filter(p => p.role === "address").length, 0), [groups]);
  const numCount = useMU(() => groups.reduce((a, g) => a + g.params.filter(p => p.role === "numeric").length, 0), [groups]);

  const keyOf = (mid, pk) => mid + ":" + pk;
  const setKeepFor = (mid, pk, v) => setKeep(s => ({ ...s, [keyOf(mid, pk)]: v }));

  const T = loc === "en" ? UEN : UKO;

  const publish = () => {
    onClose();
    toast(loc === "en" ? `Published “${name}” to the market` : `“${name}” 을 마켓에 공개했어요`, "dambi.co/m/···");
  };

  return (
    <div className="mp-modal-bd" onClick={onClose}>
      <div className="mpu-modal" onClick={e => e.stopPropagation()}>
        {/* header + step rail */}
        <div className="mpu-h">
          <div className="mpu-h-main">
            <span className="mpu-h-ic">{MPI.shield()}</span>
            <div>
              <div className="t">{T.title}</div>
              <div className="s">{T.sub}</div>
            </div>
          </div>
          <button className="mpc-x" onClick={onClose}>{MPI.x()}</button>
        </div>
        <div className="mpu-steps">
          {[T.s1, T.s2, T.s3].map((s, i) => (
            <div key={i} className={`mpu-step ${i === step ? "on" : ""} ${i < step ? "done" : ""}`}>
              <span className="no">{i < step ? MPI.check() : i + 1}</span><span className="lb">{s}</span>
              {i < 2 && <span className="bar" />}
            </div>
          ))}
        </div>

        <div className="mpu-body">
          {step === 0 && (
            <div className="mpu-deid">
              <div className="mpu-deid-banner">
                {MPI.lock()}
                <div>
                  <b>{T.forced}</b>
                  <span>{loc === "en"
                    ? <>Address-type identifiers (wallet, recipient, delegatee, allowlist) are <b>always blanked into parameter holes</b> — the person who installs fills their own. This can't be turned off.</>
                    : <>주소류 식별자(지갑·수취인·위임 대상·allowlist)는 <b>무조건 파라미터 구멍으로 비워서</b> 올라갑니다. 담는 사람이 자기 값을 채웁니다. 끌 수 없습니다.</>}</span>
                </div>
              </div>
              <div className="mpu-deid-legend">
                <span className="lg addr">{MPI.key()}{T.addr} · {addrCount}</span>
                <span className="lg num">{MPI.hash()}{T.num} · {numCount}</span>
              </div>

              <div className="mpu-deid-list">
                {groups.map(g => (
                  <div key={g.member.id} className="mpu-deid-pol">
                    <div className="mpu-deid-pol-h">
                      <span className={`mp-sev ${g.member.sev}`} />
                      <span className="nm">{MP.t(g.member.name, loc)}</span>
                      <span className="slug">{g.member.slug}</span>
                    </div>
                    {g.params.map(p => p.role === "address" ? (
                      <div key={p.key} className="mpu-param addr">
                        <span className="mpu-param-cap">{MPI.key()}</span>
                        <div className="mpu-param-main">
                          <div className="mpu-param-top"><span className="lab">{MP.t(p.label, loc)}</span><span className="canon">{p.canon}</span></div>
                          <div className="mpu-param-was">{p.current && <><span className="strike">{p.current}</span><span className="arrow">→</span></>}<span className="hole">{p.hole}</span></div>
                        </div>
                        <span className="mpu-param-lock">{MPI.lock()}{T.blanked}</span>
                      </div>
                    ) : (
                      <div key={p.key} className="mpu-param num">
                        <span className="mpu-param-cap">{MPI.hash()}</span>
                        <div className="mpu-param-main">
                          <div className="mpu-param-top"><span className="lab">{MP.t(p.label, loc)}</span><span className="canon">{p.canon}</span></div>
                          <div className="mpu-param-hint">{loc === "en" ? "Author used" : "원작자가 쓴 값"} <b>{p.recommended}{p.unit}</b></div>
                        </div>
                        <div className="mpu-keep">
                          <button className={!keep[keyOf(g.member.id, p.key)] ? "on" : ""} onClick={() => setKeepFor(g.member.id, p.key, false)}>{T.blank}<span className="sub">{p.hole}</span></button>
                          <button className={keep[keyOf(g.member.id, p.key)] ? "on" : ""} onClick={() => setKeepFor(g.member.id, p.key, true)}>{T.keep}<span className="sub">{p.recommended}{p.unit}</span></button>
                        </div>
                      </div>
                    ))}
                  </div>
                ))}
                {groups.length === 0 && <div className="mpu-empty">{T.noParams}</div>}
              </div>
            </div>
          )}

          {step === 1 && (
            <div className="mpu-meta">
              <label className="mpu-fl">
                <span className="lb">{T.nameL}</span>
                <input value={name} onChange={e => setName(e.target.value)} placeholder={T.namePh} />
              </label>
              <label className="mpu-fl">
                <span className="lb">{T.descL}</span>
                <textarea value={desc} onChange={e => setDesc(e.target.value)} rows={3} placeholder={T.descPh} />
              </label>
              <div className="mpu-summary-card">
                <div className="h">{T.willPublish}</div>
                <div className="r"><span>{T.rules}</span><b>{pkg.members.length}{loc === "en" ? "" : "개"}</b></div>
                <div className="r"><span>{T.addrHoles}</span><b>{addrCount}</b></div>
                <div className="r"><span>{T.numKept}</span><b>{Object.values(keep).filter(Boolean).length} / {numCount}</b></div>
                <div className="mpu-vis">{MPI.shield()}{T.publicNote}</div>
              </div>
            </div>
          )}
        </div>

        <div className="mpu-foot">
          {step === 0 && <><span className="mpu-foot-note">{T.foot0}</span><button className="mp-btn-ghost" onClick={onClose}>{T.cancel}</button><button className="mpu-next" onClick={() => setStep(1)}>{T.next}{MPI.caret()}</button></>}
          {step === 1 && <><button className="mp-btn-ghost" onClick={() => setStep(0)}>{MPI.back()}{T.back}</button><span className="mpu-foot-spc" /><button className="mpu-publish" onClick={publish}>{MPI.shield()}{T.publish}</button></>}
        </div>
      </div>
    </div>
  );
}

const UKO = {
  title: "마켓에 올리기", sub: "내가 큐레이션한 패키지를 공개해 다른 사용자가 담을 수 있게 합니다.",
  s1: "비식별 확인", s2: "이름·설명", s3: "공개",
  forced: "개인정보 자동 비식별 (강제)",
  addr: "주소류 (비움 고정)", num: "숫자 임계값 (선택)",
  blanked: "비워짐", blank: "비우기", keep: "추천값 남기기",
  nameL: "패키지 이름", namePh: "마켓에 보일 이름",
  descL: "설명", descPh: "이 패키지가 무엇을 막는지 한두 문장으로",
  willPublish: "공개될 내용", rules: "정책 수", addrHoles: "주소 구멍(비식별)", numKept: "추천값 남김",
  publicNote: "공개 = 누구나 마켓에서 담을 수 있음. 비공개로 되돌릴 수 있어요.",
  foot0: "주소류는 항상 비워집니다 · 우회 불가",
  cancel: "취소", next: "다음", back: "뒤로", publish: "마켓에 공개",
  noParams: "비식별할 파라미터가 없습니다."
};
const UEN = {
  title: "Publish to market", sub: "Share your curated package so other users can install it.",
  s1: "De-identify", s2: "Name & describe", s3: "Publish",
  forced: "Automatic de-identification (forced)",
  addr: "Address-type (always blanked)", num: "Numeric thresholds (optional)",
  blanked: "blanked", blank: "Blank", keep: "Keep suggested",
  nameL: "Package name", namePh: "Name shown in the market",
  descL: "Description", descPh: "A sentence or two on what this blocks",
  willPublish: "What gets published", rules: "Policies", addrHoles: "Address holes (de-id)", numKept: "Suggested kept",
  publicNote: "Public = anyone can install from the market. You can make it private again.",
  foot0: "Address-type values are always blanked · can't be bypassed",
  cancel: "Cancel", next: "Next", back: "Back", publish: "Publish to market",
  noParams: "No parameters to de-identify."
};

Object.assign(window, { UploadModal });
