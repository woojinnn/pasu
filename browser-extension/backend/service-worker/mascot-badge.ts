/**
 * 마스코트 상태 배지 — 노랑목담비가 safe → warn → fail 로 변신.
 *
 * 데모(pasu-demo)에서는 인위적 `PASU_VERDICT` 메시지로 stats 를 누적했지만,
 * 실배포에서는 정책 엔진의 **실제 verdict 기록**(verdict-storage)을 단일 소스로
 * 쓴다. 최근 24시간의 fail/warn 카운트를 집계해서:
 *   - fail > 0 → state-fail (빨강 배지, 차단 건수)
 *   - warn > 0 → state-warn (노랑 배지, 검토 권장 건수)
 *   - 그 외   → state-safe (배지 없음, "보호 중")
 *
 * `refreshBadge()` 는 (1) SW 부팅 시, (2) `appendVerdictsForMessage` 직후
 * (orchestrator) 호출된다. chrome.action 호출은 best-effort — 실패해도(예: 일부
 * 환경에서 path setIcon 미지원) 조용히 넘긴다.
 */

import Browser from "webextension-polyfill";

import { countVerdicts } from "./verdict-storage";

/**
 * 마스코트 상태 아이콘 — 툴바 선명도를 위해 사이즈맵(16/32/48/128)을 쓴다.
 * public/picture/ 에 webpack(CopyPlugin)이 그대로 복사한다(핸드오프 에셋).
 */
const MASCOT_ICON: Record<"safe" | "warn" | "fail", Record<number, string>> = {
  safe: {
    16: "picture/state-safe-16.png",
    32: "picture/state-safe-32.png",
    48: "picture/state-safe-48.png",
    128: "picture/state-safe-128.png",
  },
  warn: {
    16: "picture/state-warn-16.png",
    32: "picture/state-warn-32.png",
    48: "picture/state-warn-48.png",
    128: "picture/state-warn-128.png",
  },
  fail: {
    16: "picture/state-fail-16.png",
    32: "picture/state-fail-32.png",
    48: "picture/state-fail-48.png",
    128: "picture/state-fail-128.png",
  },
};

const COLOR = {
  safe: "#1B8C5E",
  warn: "#DCA02C",
  fail: "#DD4A3C",
} as const;

// webextension-polyfill 의 타입에 action.setIcon 의 `path` 시그니처가 좁게
// 잡혀 있어, 좁은 ambient 타입으로 감싼다(런타임은 chrome.action 와 동일).
interface ActionApi {
  setIcon(details: {
    path: Record<number, string> | Record<string, string> | string;
  }): Promise<void>;
  setBadgeText(details: { text: string }): Promise<void>;
  setBadgeBackgroundColor(details: { color: string }): Promise<void>;
  setTitle(details: { title: string }): Promise<void>;
}

function action(): ActionApi | null {
  const a = (Browser as unknown as { action?: ActionApi }).action;
  return a ?? null;
}

/**
 * 최근 24시간 verdict 집계로 배지를 갱신한다. 멱등 — 아무 때나 호출해도 안전.
 */
export async function refreshBadge(): Promise<void> {
  const a = action();
  if (!a) return;

  let fail = 0;
  let warn = 0;
  try {
    const counts = await countVerdicts({ range: "24h" });
    fail = counts.fail ?? 0;
    warn = counts.warn ?? 0;
  } catch (err) {
    console.warn("[Pasu] mascot badge: countVerdicts failed", err);
  }

  let state: "safe" | "warn" | "fail" = "safe";
  let text = "";
  let title = "Pasu — 보호 중";
  if (fail > 0) {
    state = "fail";
    text = String(fail);
    title = `Pasu — 오늘 차단 ${fail}건`;
  } else if (warn > 0) {
    state = "warn";
    text = String(warn);
    title = `Pasu — 검토 권장 ${warn}건`;
  }

  // 모두 best-effort: 한 호출이 실패해도 나머지는 시도한다.
  try {
    // 사이즈맵(16/32/48/128) — Chrome 이 DPI/툴바 크기에 맞는 해상도를 고른다.
    await a.setIcon({ path: MASCOT_ICON[state] });
  } catch (err) {
    void err;
  }
  try {
    await a.setBadgeText({ text });
  } catch (err) {
    void err;
  }
  try {
    await a.setBadgeBackgroundColor({ color: COLOR[state] });
  } catch (err) {
    void err;
  }
  try {
    await a.setTitle({ title });
  } catch (err) {
    void err;
  }
}
