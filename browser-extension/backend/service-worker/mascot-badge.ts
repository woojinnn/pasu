/**
 * 마스코트 상태 배지 — 노랑목담비가 safe → warn → fail 로 변신.
 *
 * 정책 엔진의 실제 verdict 기록(verdict-storage)을 단일 소스로, **미확인**
 * fail/warn 만 집계한다. "확인" = 팝업 열기(PASU_BADGE_SEEN → markBadgeSeen):
 * 그 시각 이전 verdict 는 집계에서 빠지고 배지는 safe 로 복귀한다. 24시간
 * 롤링 윈도우는 하한 — 확인하지 않아도 하루 지난 알람은 흘려보낸다.
 *
 *   - 미확인 fail > 0 → fail 마스코트 + 발바닥 오버레이 (차단 건수는 툴팁에)
 *   - 미확인 warn > 0 → warn 마스코트 + 발바닥 오버레이 (검토 권장 건수는 툴팁에)
 *   - 그 외          → safe 마스코트 (발바닥 없음, "보호 중")
 *
 * 발바닥은 Chrome 배지 텍스트가 아니라(텍스트만 지원) OffscreenCanvas 로
 * 상태 마스코트 우하단에 상태색 원 + paw-white 를 합성해 setIcon(imageData)
 * 으로 준다. 합성 불가 환경이면 발바닥 없는 path 사이즈맵으로 폴백.
 *
 * `refreshBadge()` 는 (1) SW 부팅 시, (2) `appendVerdictsForMessage` 직후
 * (orchestrator), (3) markBadgeSeen 직후 호출된다. chrome.action 호출은
 * best-effort — 실패해도 조용히 넘긴다.
 */

import Browser from "webextension-polyfill";

import { countVerdicts } from "./verdict-storage";

type MascotState = "safe" | "warn" | "fail";

const SIZES = [16, 32, 48, 128] as const;

const SEEN_KEY = "mascot_badge_seen_at";

const COLOR: Record<MascotState, string> = {
  safe: "#1B8C5E",
  warn: "#DCA02C",
  fail: "#DD4A3C",
};

/** public/picture/ 의 사이즈별 상태 마스코트(핸드오프 에셋). */
function statePath(state: MascotState, size: number): string {
  return `picture/state-${state}-${size}.png`;
}

// webextension-polyfill 의 타입에 action.setIcon 의 시그니처가 좁게
// 잡혀 있어, 좁은 ambient 타입으로 감싼다(런타임은 chrome.action 와 동일).
interface ActionApi {
  setIcon(details: {
    path?: Record<string, string>;
    imageData?: Record<string, ImageData>;
  }): Promise<void>;
  setBadgeText(details: { text: string }): Promise<void>;
  setTitle(details: { title: string }): Promise<void>;
}

function action(): ActionApi | null {
  const a = (Browser as unknown as { action?: ActionApi }).action;
  return a ?? null;
}

async function getSeenAt(): Promise<number> {
  try {
    const got = await Browser.storage.local.get(SEEN_KEY);
    const value = got[SEEN_KEY];
    return typeof value === "number" && Number.isFinite(value) ? value : 0;
  } catch (err) {
    void err;
    return 0;
  }
}

/**
 * 팝업이 열렸다 = 알람 확인. 이 시각까지의 verdict 를 배지 집계에서 제외하고
 * 배지를 즉시 갱신한다(→ safe 복귀). 멱등.
 */
export async function markBadgeSeen(): Promise<void> {
  try {
    await Browser.storage.local.set({
      [SEEN_KEY]: Math.floor(Date.now() / 1000),
    });
  } catch (err) {
    console.warn("[Pasu] mascot badge: seen 기록 실패", err);
  }
  await refreshBadge();
}

/**
 * 미확인(확인 이후 + 최근 24시간) verdict 집계로 배지를 갱신한다.
 * 멱등 — 아무 때나 호출해도 안전.
 */
export async function refreshBadge(): Promise<void> {
  const a = action();
  if (!a) return;

  let fail = 0;
  let warn = 0;
  try {
    const now = Math.floor(Date.now() / 1000);
    const seenAt = await getSeenAt();
    // seenAt 그 초의 verdict 는 팝업에서 이미 본 것 — +1 로 제외한다.
    const since = Math.max(now - 86_400, seenAt > 0 ? seenAt + 1 : 0);
    const counts = await countVerdicts({ since });
    fail = counts.fail ?? 0;
    warn = counts.warn ?? 0;
  } catch (err) {
    console.warn("[Pasu] mascot badge: countVerdicts failed", err);
  }

  let state: MascotState = "safe";
  let title = "Pasu — 보호 중";
  if (fail > 0) {
    state = "fail";
    title = `Pasu — 오늘 차단 ${fail}건`;
  } else if (warn > 0) {
    state = "warn";
    title = `Pasu — 검토 권장 ${warn}건`;
  }

  // 모두 best-effort: 한 호출이 실패해도 나머지는 시도한다.
  try {
    await setStateIcon(a, state, state !== "safe");
  } catch (err) {
    void err;
  }
  try {
    // 숫자 배지는 쓰지 않는다 — 미확인 표시는 발바닥 오버레이가 담당.
    await a.setBadgeText({ text: "" });
  } catch (err) {
    void err;
  }
  try {
    await a.setTitle({ title });
  } catch (err) {
    void err;
  }
}

/**
 * 상태 마스코트 아이콘을 적용한다. withPaw 면 발바닥 오버레이를 합성해
 * imageData 로, 합성 실패/불가면 발바닥 없는 path 사이즈맵으로 폴백.
 *
 * setIcon 의 path 는 SW 컨텍스트에서 fetch(path)로 로드돼 워커 base URL
 * (js/background.js → js/) 기준으로 풀리므로, runtime.getURL 절대 URL로 준다.
 */
async function setStateIcon(
  a: ActionApi,
  state: MascotState,
  withPaw: boolean,
): Promise<void> {
  if (withPaw) {
    try {
      const imageData = await composePawIcon(state);
      await a.setIcon({ imageData });
      return;
    } catch (err) {
      void err;
    }
  }
  const path = Object.fromEntries(
    SIZES.map((size) => [
      String(size),
      Browser.runtime.getURL(statePath(state, size)),
    ]),
  );
  await a.setIcon({ path });
}

async function loadBitmap(rel: string): Promise<ImageBitmap> {
  const res = await fetch(Browser.runtime.getURL(rel));
  if (!res.ok) throw new Error(`asset fetch failed: ${rel}`);
  return createImageBitmap(await res.blob());
}

/**
 * 사이즈별(16/32/48/128) 상태 마스코트 우하단에 상태색 원(지름 62.5%) +
 * paw-white(원의 68%)를 합성한 ImageData 사이즈맵을 만든다.
 */
async function composePawIcon(
  state: MascotState,
): Promise<Record<string, ImageData>> {
  if (typeof OffscreenCanvas === "undefined") {
    throw new Error("OffscreenCanvas unavailable");
  }
  const [paw, ...bases] = await Promise.all([
    loadBitmap("picture/paw-white.png"),
    ...SIZES.map((size) => loadBitmap(statePath(state, size))),
  ]);

  const out: Record<string, ImageData> = {};
  SIZES.forEach((size, i) => {
    const canvas = new OffscreenCanvas(size, size);
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("2d context unavailable");
    ctx.drawImage(bases[i]!, 0, 0, size, size);

    const diameter = size * 0.625;
    const radius = diameter / 2;
    const cx = size - radius;
    const cy = size - radius;
    ctx.fillStyle = COLOR[state];
    ctx.beginPath();
    ctx.arc(cx, cy, radius, 0, Math.PI * 2);
    ctx.fill();

    const pawSize = diameter * 0.68;
    ctx.drawImage(paw, cx - pawSize / 2, cy - pawSize / 2, pawSize, pawSize);

    out[String(size)] = ctx.getImageData(0, 0, size, size);
  });
  return out;
}
