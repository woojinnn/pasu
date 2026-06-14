import Browser from "webextension-polyfill";
import "./styles.css";

// Sentinel guard mark (B 드로어 GuardFace). 이 빌드의 webpack 은 png import
// (asset/resource) 룰이 .wasm 에만 걸려 있어 `import x from "*.png"` 를 못 쓴다.
// 핸드오프 권장 대안대로 에셋을 public/picture/ 에 두고(CopyPlugin 이 dist 로
// 복사) 런타임 확장 URL 로 참조한다.
const markWhite = Browser.runtime.getURL("picture/dambi-mark-white.png");
// 가드 = 상태별 마스코트(캐논 GuardFace = STATE_MARTEN[kind]) — 인터셉트·②배지·⑤토스트와 동일한 담비 변신.
const STATE_MARTEN: Record<string, string> = {
  pass: Browser.runtime.getURL("picture/state-safe.png"),
  warn: Browser.runtime.getURL("picture/state-warn.png"),
  fail: Browser.runtime.getURL("picture/state-fail.png"),
};

interface MatchedPolicy {
  policy_id: string;
  reason?: string;
  severity: string;
  origin: string;
}
interface VerdictDto {
  kind: "pass" | "warn" | "fail";
  matched?: MatchedPolicy[];
}
interface ConfirmDetails {
  kind: "untyped_signature";
  title?: string;
  messagePreview?: string;
  messageTruncated?: boolean;
}

const params = new URLSearchParams(window.location.search);
const requestId = params.get("requestId") ?? "";
const hostname = params.get("hostname") ?? "";
const verdictRaw = params.get("verdict") ?? '{"kind":"fail"}';
const detailsRaw = params.get("details") ?? "";

let verdict: VerdictDto;
try {
  verdict = JSON.parse(verdictRaw) as VerdictDto;
} catch {
  verdict = { kind: "fail" };
}

let details: ConfirmDetails | null = null;
try {
  details = detailsRaw ? (JSON.parse(detailsRaw) as ConfirmDetails) : null;
} catch {
  details = null;
}

const PENDING_DECISION_KEY = "requests:pending-decisions";

async function reply(ok: boolean): Promise<void> {
  // Two-channel reply for SW-restart durability:
  //  1. Direct runtime.sendMessage (wakes the SW if needed; immediately
  //     resolves the in-flight openVerdictWindow promise).
  //  2. Persisted decision in chrome.storage.session — the SW poll loop
  //     reads this if the message arrives during a SW death window.
  try {
    const all =
      ((await Browser.storage.session.get(PENDING_DECISION_KEY))[
        PENDING_DECISION_KEY
      ] as Record<string, { status: string; ok?: boolean }> | undefined) ?? {};
    if (all[requestId]) {
      all[requestId] = { ...all[requestId], status: "decided", ok };
      await Browser.storage.session.set({ [PENDING_DECISION_KEY]: all });
    }
  } catch {
    /* best-effort */
  }
  try {
    await Browser.runtime.sendMessage({
      type: "dambi:verdict-decision",
      requestId,
      ok,
    });
  } catch {
    /* SW may have closed already */
  }
  window.close();
}

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Partial<{ class: string; text: string }> = {},
  children: (HTMLElement | string)[] = [],
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (attrs.class) node.className = attrs.class;
  if (attrs.text) node.textContent = attrs.text;
  for (const c of children) {
    if (typeof c === "string") node.appendChild(document.createTextNode(c));
    else node.appendChild(c);
  }
  return node;
}

// Inline SVG (no extra asset) for status glyphs.
function svgSpan(cls: string, svg: string): HTMLSpanElement {
  const s = document.createElement("span");
  s.className = cls;
  s.innerHTML = svg;
  return s;
}

const SVG = {
  check:
    '<svg viewBox="0 0 24 24" fill="none"><path d="M20 6 9 17l-5-5" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/></svg>',
  warn:
    '<svg viewBox="0 0 24 24" fill="none"><path d="M12 3 2 20h20L12 3Z" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round"/><path d="M12 10v4M12 17.4v.2" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"/></svg>',
  x: '<svg viewBox="0 0 24 24" fill="none"><path d="M18 6 6 18M6 6l12 12" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"/></svg>',
  info:
    '<svg viewBox="0 0 24 24" fill="none"><circle cx="12" cy="12" r="9" stroke="currentColor" stroke-width="2"/><path d="M12 11v5M12 7.6v.2" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"/></svg>',
  globe:
    '<svg viewBox="0 0 24 24" fill="none"><circle cx="12" cy="12" r="8.5" stroke="currentColor" stroke-width="1.6"/><path d="M3.5 12h17M12 3.5c2.5 2.4 2.5 14.6 0 17M12 3.5c-2.5 2.4-2.5 14.6 0 17" stroke="currentColor" stroke-width="1.6"/></svg>',
};

const COPY: Record<string, { head: string; sub: string }> = {
  fail: {
    head: "Transaction blocked",
    sub: "Dambi policy stopped this signature. Review the matched policies below.",
  },
  warn: {
    head: "Manual review recommended",
    sub: "This request triggered a policy warning. Check it before you proceed.",
  },
  pass: {
    head: "No policy risks found",
    sub: "This request passed Dambi's baseline checks.",
  },
};

const BADGE: Record<string, string> = { fail: SVG.x, warn: SVG.warn, pass: SVG.check };
const SEV_ICON: Record<string, string> = { deny: SVG.x, warn: SVG.warn, info: SVG.info };
// severity(데이터) → 행 비주얼 클래스
const SEV_CLASS: Record<string, string> = { deny: "fail", warn: "warn", info: "info" };

function renderDetails(details: ConfirmDetails | null): HTMLElement | null {
  if (!details || details.kind !== "untyped_signature") return null;
  const messagePreview = details.messagePreview ?? "";
  if (!messagePreview) return null;

  const children: HTMLElement[] = [
    el("div", {
      class: "sig-title",
      text: details.title || "Plain-text signature",
    }),
    el("pre", { class: "sig-message", text: messagePreview }),
  ];

  if (details.messageTruncated) {
    children.push(
      el("div", { class: "sig-note", text: "Message preview truncated." }),
    );
  }

  return el("section", { class: "sig-preview" }, children);
}

function render(): void {
  const root = document.getElementById("root");
  if (!root) return;

  const kind = verdict.kind;
  const isFail = kind === "fail";
  const isWarn = kind === "warn";
  const c = COPY[kind] ?? COPY.fail;

  // ── top bar (navy) ──
  const mk = document.createElement("img");
  mk.className = "mk";
  mk.src = markWhite;
  mk.alt = "";
  const net = el("span", { class: "net" }, [
    (() => {
      const d = document.createElement("span");
      d.className = "nd";
      return d;
    })(),
    "watch-only",
  ]);
  const top = el("div", { class: "top" }, [
    mk,
    el("span", { class: "wd", text: "DAMBI" }),
    net,
  ]);

  // ── guard hero ──
  const face = document.createElement("img");
  face.src = STATE_MARTEN[kind] ?? STATE_MARTEN.pass;
  face.alt = "";
  const guard = el("div", { class: `guard ${kind}` }, [
    el("div", { class: "ring" }),
    el("div", { class: "face" }, [face]),
  ]);
  guard.appendChild(svgSpan("badge", BADGE[kind] ?? BADGE.fail));
  const hero = el("div", { class: `hero ${kind}` }, [
    guard,
    el("div", { class: `headline ${kind}`, text: c.head }),
    el("div", { class: "sub", text: c.sub }),
  ]);

  // ── body: origin + matched ──
  const body = el("div", { class: "body" });
  const requestLabel =
    details?.kind === "untyped_signature"
      ? "Plain-text signature requested"
      : "Signature requested";
  if (hostname) {
    body.appendChild(
      el("div", { class: "origin" }, [
        svgSpan("fav", SVG.globe),
        el("div", { class: "otx" }, [
          el("div", { class: "oh", text: hostname }),
          el("div", { class: "os", text: requestLabel }),
        ]),
      ]),
    );
  }
  const detailsCard = renderDetails(details);
  if (detailsCard) body.appendChild(detailsCard);

  const matched = verdict.matched ?? [];
  if (matched.length === 0) {
    body.appendChild(
      el("div", { class: "empty" }, [
        svgSpan("empty-ic", isFail ? SVG.warn : SVG.check),
        el("div", { class: "empty-t", text: "No matched policies reported." }),
      ]),
    );
  } else {
    body.appendChild(
      el("div", {
        class: "matched-label",
        text: `${matched.length} ${matched.length === 1 ? "policy" : "policies"} matched`,
      }),
    );
    const risks = el("div", { class: "risks" });
    for (const m of matched) {
      const sev = SEV_ICON[m.severity] ? m.severity : "info";
      const rcls = SEV_CLASS[sev] ?? "info";
      const tx = el("div", { class: "rtx" }, [
        el("div", { class: "rt", text: m.reason ?? m.policy_id }),
      ]);
      if (m.reason) tx.appendChild(el("div", { class: "rid", text: m.policy_id }));
      tx.appendChild(
        el("div", {
          class: "rmeta",
          text: `${m.severity}${m.origin ? " • " + m.origin : ""}`,
        }),
      );
      risks.appendChild(
        el("article", { class: `risk ${rcls}` }, [svgSpan("ic", SEV_ICON[sev]), tx]),
      );
    }
    body.appendChild(risks);
  }

  // ── footer (wiring unchanged) ──
  const footer = el("footer");
  const cancelBtn = el("button", {
    class: "btn-cancel",
    text: isFail ? "Close" : "Cancel",
  });
  cancelBtn.addEventListener("click", () => void reply(false));
  footer.appendChild(cancelBtn);
  if (isWarn) {
    const approveBtn = el("button", {
      class: "btn-approve",
      text: "Trust and proceed",
    });
    approveBtn.addEventListener("click", () => void reply(true));
    footer.appendChild(approveBtn);
  }

  root.appendChild(top);
  root.appendChild(hero);
  root.appendChild(body);
  root.appendChild(footer);
}

// On window close (X button), treat as cancel — already-failed verdicts
// just dismiss; warn verdicts treat close as "user did not approve".
window.addEventListener("beforeunload", () => {
  // Persist a final "decided: ok=false" so the SW poll loop sees the
  // close intent if the runtime message racing with unload doesn't land.
  void Browser.storage.session
    .get(PENDING_DECISION_KEY)
    .then((stored) => {
      const all = (stored as Record<string, unknown>)[PENDING_DECISION_KEY] as
        | Record<string, { status: string; ok?: boolean }>
        | undefined;
      if (all && all[requestId] && all[requestId].status === "awaiting") {
        all[requestId] = { ...all[requestId], status: "decided", ok: false };
        return Browser.storage.session.set({ [PENDING_DECISION_KEY]: all });
      }
      return undefined;
    })
    .catch(() => {});
});

render();
