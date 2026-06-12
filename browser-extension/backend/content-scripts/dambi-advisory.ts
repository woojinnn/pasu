/**
 * Dambi advisory content-script — ③ 인라인 주석 + ⑤ 인페이지 토스트.
 *
 * ⚠️ 보안 원칙(핸드오프 §보안): 이 스크립트는 **표시(advisory) 전용**이다.
 * 서명 allow/block 게이트는 confirm.html(별도 OS 창)이 전담하며, 여기서는
 * 절대 `dambi:verdict-decision` 같은 결정 메시지를 발신하지 않는다. ④ 인페이지
 * 드로어는 confirm 창과 트리거가 겹치고 위조 표면을 늘리므로 제거됐다.
 *
 * 데모(intercept.js)는 페이지 DOM 에 직접 노드를 삽입했지만, 여기서는 보안
 * 조사 결론에 따라 **closed shadow DOM** 으로 격리한다(페이지 CSS 가 advisory
 * 를 못 건드리게). 호스트 엘리먼트 자체는 페이지가 제거할 수 있으나, advisory
 * 는 결정 권한이 없으므로 그 잔존 리스크는 수용 가능하다.
 *
 * 스타일은 webpack css-loader(<style> in document.head)가 shadow 와 맞지 않아,
 * `public/advisory.css` 를 런타임에 fetch 해 shadow root 안에 주입한다.
 */

import Browser from "webextension-polyfill";

interface ToastSpec {
  sev: "fail" | "warn" | "safe";
  time: string;
  title: string;
  bodyHtml: string;
  actions: { label: string; kind?: "danger" | "primary" }[];
}

interface AnnotMessage {
  type: "DAMBI_ANNOT";
  host?: string;
  selector?: string;
}
interface ToastMessage {
  type: "DAMBI_TOAST";
  scenario?: string;
  /** SW 가 채워 보내는 실제 데이터(예: 주간요약 fail/warn 카운트). */
  data?: { fail?: number; warn?: number };
}
type AdvisoryMessage =
  | AnnotMessage
  | ToastMessage
  | { type: "DAMBI_HIDE" };

const ASSET = (p: string): string => Browser.runtime.getURL(`picture/${p}`);
const STATE_WARN = ASSET("state-warn.png");
const STATE_FAIL = ASSET("state-fail.png");
const STATE_SAFE = ASSET("state-safe.png");
const PAW_GOLD = ASSET("paw-gold.png");
const PAW_NAVY = ASSET("paw-navy.png");

const X14 =
  '<svg viewBox="0 0 24 24" fill="none" width="14" height="14"><path d="M18 6 6 18M6 6l12 12" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"/></svg>';

// ─── shadow host (격리) ────────────────────────────────────────────
let host: HTMLElement | null = null;
let shadow: ShadowRoot | null = null;
let stylesInjected = false;

function ensureHost(): ShadowRoot {
  if (host && shadow) return shadow;
  host = document.createElement("div");
  host.id = "dambi-advisory-host";
  // 호스트는 레이아웃에 영향 안 주는 0-size 컨테이너. 자식(annot/toast)은
  // position:fixed 라 호스트 크기와 무관하게 뜬다.
  host.style.cssText = "all:initial; position:fixed; z-index:2147483646;";
  // closed → 페이지 스크립트가 host.shadowRoot 로 내부에 접근 불가.
  shadow = host.attachShadow({ mode: "closed" });
  (document.documentElement || document.body).appendChild(host);
  void injectStyles(shadow);
  return shadow;
}

async function injectStyles(root: ShadowRoot): Promise<void> {
  if (stylesInjected) return;
  stylesInjected = true;
  try {
    const res = await fetch(Browser.runtime.getURL("advisory.css"));
    const css = await res.text();
    const style = document.createElement("style");
    style.textContent = css;
    root.appendChild(style);
  } catch {
    /* 스타일 fetch 실패 시에도 마크업은 뜬다(미스타일) */
  }
}

/* ============================================================
   ③ 인라인 주석 — 위험 요소 옆 콜아웃(말풍선).
   ============================================================ */
function placeAnnot(box: HTMLElement, target: Element | null): void {
  box.classList.add("up");
  if (target && target.getBoundingClientRect) {
    const r = target.getBoundingClientRect();
    const w = 300;
    const left = Math.min(
      window.innerWidth - w - 12,
      Math.max(12, r.left + r.width / 2 - w + 70),
    );
    box.style.left = `${left}px`;
    box.style.top = `${r.bottom + 12}px`;
    const bx = Math.max(16, Math.min(w - 28, r.left + r.width / 2 - left - 7));
    box.style.setProperty("--bx", `${bx}px`);
    box.classList.add("anchored");
  } else {
    box.classList.add("floating");
    box.style.setProperty("--bx", "250px");
  }
}

function showAnnotation(opts: AnnotMessage): void {
  const root = ensureHost();
  removeById(root, "dambi-annot");
  const hostname = opts.host || location.hostname;
  // selector 는 페이지 DOM 기준 — shadow 밖이라 document 에서 찾는다(앵커 위치 계산용).
  const target = opts.selector ? document.querySelector(opts.selector) : null;
  const box = document.createElement("div");
  box.id = "dambi-annot";
  box.className = "warn";
  box.innerHTML =
    '<div class="co-head">' +
    `<span class="co-mk"><img src="${STATE_WARN}" alt="" /></span>` +
    '<span class="co-tag">워치리스트</span>' +
    `<button class="co-x" aria-label="닫기">${X14}</button>` +
    "</div>" +
    '<div class="co-t">연결 전에 잠깐 — 이 도메인이 의심돼요</div>' +
    `<div class="co-s">도메인 <span class="mono">${escapeHtml(hostname)}</span> 의 평판을 확인하지 못했어요. 연결·서명 전 한 번 더 확인하세요.</div>` +
    '<div class="co-foot"><button class="co-btn">자세히</button><button class="co-btn solid">이 사이트 떠나기</button></div>';
  root.appendChild(box);
  placeAnnot(box, target);
  // isTrusted 가드: 페이지가 합성 클릭을 던져도 무시(advisory라 치명적이진
  // 않지만 일관성).
  box.querySelector(".co-x")?.addEventListener("click", (e) => {
    if (!(e as MouseEvent).isTrusted) return;
    removeAnnotation();
  });
  box.querySelector(".co-btn.solid")?.addEventListener("click", (e) => {
    if (!(e as MouseEvent).isTrusted) return;
    removeAnnotation();
  });
}

function removeAnnotation(): void {
  if (shadow) removeById(shadow, "dambi-annot");
}

/* ============================================================
   ⑤ 인페이지 토스트 — 4종 시나리오. summary 는 SW 실데이터 바인딩.
   ============================================================ */
function toastSpec(scenario: string, data?: ToastMessage["data"]): ToastSpec {
  const fail = data?.fail ?? 0;
  const warn = data?.warn ?? 0;
  switch (scenario) {
    case "summary":
      return {
        sev: fail > 0 ? "fail" : "warn",
        time: "지난 7일",
        title: "이번 주 Dambi 요약",
        bodyHtml:
          `<div class="mn-text">이번 주 위험 <b>${fail}건</b>을 차단하고 <b>${warn}건</b>은 검토를 권했어요.</div>` +
          '<div class="mn-ctx">백그라운드 모니터링 · 지난 7일</div>',
        actions: [{ label: "닫기" }, { label: "대시보드 열기", kind: "primary" }],
      };
    case "approval":
      return {
        sev: "fail",
        time: "지금",
        title: "승인 권한이 위험해졌어요",
        bodyHtml:
          '<div class="mn-text">방금 한 토큰 <b>무제한 승인</b>이 위험 컨트랙트로 표시됐어요.</div>' +
          '<div class="mn-ctx">백그라운드 모니터링</div>',
        actions: [{ label: "나중에" }, { label: "권한 취소", kind: "danger" }],
      };
    case "tx":
    default:
      return {
        sev: "warn",
        time: "방금",
        title: "의심 거래가 감지됐어요",
        bodyHtml:
          '<div class="mn-text">상호작용한 주소가 위험 목록과 일치해요.</div>' +
          '<div class="mn-ctx">백그라운드 모니터링</div>',
        actions: [{ label: "무시" }, { label: "확인하기", kind: "primary" }],
      };
  }
}

function showToast(scenario: string, data?: ToastMessage["data"]): void {
  const root = ensureHost();
  removeById(root, "dambi-toast");
  const d = toastSpec(scenario, data);
  const mar = d.sev === "fail" ? STATE_FAIL : d.sev === "warn" ? STATE_WARN : STATE_SAFE;
  const paw = d.sev === "fail" ? PAW_NAVY : PAW_GOLD;
  const box = document.createElement("div");
  box.id = "dambi-toast";
  box.className = d.sev;
  box.innerHTML =
    '<div class="mn-main">' +
    `<div class="mn-icon"><img src="${mar}" alt="" /><span class="mn-paw"><img src="${paw}" alt="" /></span></div>` +
    '<div class="mn-content">' +
    `<div class="mn-top"><span class="mn-app">Dambi</span><span class="mn-time">${d.time}</span></div>` +
    `<div class="mn-title">${d.title}</div>${d.bodyHtml}` +
    "</div>" +
    "</div>" +
    '<div class="mn-actions">' +
    d.actions
      .map((a) => `<button class="${a.kind ?? ""}">${a.label}</button>`)
      .join("") +
    "</div>";
  root.appendChild(box);
  box.querySelectorAll(".mn-actions button").forEach((b) =>
    b.addEventListener("click", (e) => {
      if (!(e as MouseEvent).isTrusted) return;
      removeToast();
    }),
  );
}

function removeToast(): void {
  if (shadow) removeById(shadow, "dambi-toast");
}

// ─── helpers ───────────────────────────────────────────────────────
function removeById(root: ShadowRoot, id: string): void {
  const node = root.querySelector(`#${id}`);
  if (node) node.remove();
}
function escapeHtml(s: string): string {
  return s.replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ] as string,
  );
}

/* ---------- message bridge (advisory 표시 전용 — 결정 채널 없음) ---------- */
Browser.runtime.onMessage.addListener((msg: unknown) => {
  const m = msg as AdvisoryMessage | null;
  if (!m || typeof m !== "object" || !("type" in m)) return;
  if (m.type === "DAMBI_ANNOT") showAnnotation(m);
  else if (m.type === "DAMBI_TOAST") showToast(m.scenario ?? "tx", m.data);
  else if (m.type === "DAMBI_HIDE") {
    removeAnnotation();
    removeToast();
  }
});
