import Browser from "webextension-polyfill";
import "./styles.css";

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

const params = new URLSearchParams(window.location.search);
const requestId = params.get("requestId") ?? "";
const hostname = params.get("hostname") ?? "";
const verdictRaw = params.get("verdict") ?? '{"kind":"fail"}';

let verdict: VerdictDto;
try {
  verdict = JSON.parse(verdictRaw) as VerdictDto;
} catch {
  verdict = { kind: "fail" };
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
      type: "scopeball:verdict-decision",
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

function render(): void {
  const root = document.getElementById("root");
  if (!root) return;

  const isFail = verdict.kind === "fail";
  const isWarn = verdict.kind === "warn";

  const banner = el("div", {
    class: `banner ${verdict.kind}`,
    text: isFail ? "Fail" : "Warn",
  });
  const heading = el("h1", {
    text: isFail
      ? "Transaction blocked by policy"
      : "Policy warning — review before signing",
  });
  const host = hostname ? el("div", { class: "host", text: hostname }) : null;

  const matchedSection = el("section", { class: "matched" });
  const matched = verdict.matched ?? [];
  if (matched.length === 0) {
    matchedSection.appendChild(
      el("div", { class: "empty", text: "No matched policies reported." }),
    );
  } else {
    for (const m of matched) {
      const card = el("article", { class: `match ${m.severity}` });
      card.appendChild(el("div", { class: "match-id", text: m.policy_id }));
      if (m.reason) {
        card.appendChild(el("div", { class: "match-reason", text: m.reason }));
      }
      card.appendChild(
        el("div", { class: "match-meta", text: `${m.severity} • ${m.origin}` }),
      );
      matchedSection.appendChild(card);
    }
  }

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

  root.appendChild(banner);
  root.appendChild(heading);
  if (host) root.appendChild(host);
  root.appendChild(matchedSection);
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
