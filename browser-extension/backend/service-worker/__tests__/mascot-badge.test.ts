import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  counts: { pass: 0, warn: 0, fail: 0 } as Record<string, number>,
  localStore: new Map<string, unknown>(),
  countVerdicts: vi.fn(async (_opts?: { since?: number }) => mocks.counts),
  browser: {
    runtime: {
      getURL: vi.fn((path: string) => `chrome-extension://test-id/${path}`),
    },
    storage: {
      local: {
        get: vi.fn(async (key: string) => ({ [key]: mocks.localStore.get(key) })),
        set: vi.fn(async (entries: Record<string, unknown>) => {
          for (const [key, value] of Object.entries(entries)) {
            mocks.localStore.set(key, value);
          }
        }),
      },
    },
    action: {
      setIcon: vi.fn(
        async (_details: {
          path?: Record<string, string>;
          imageData?: Record<string, ImageData>;
        }) => {},
      ),
      setBadgeText: vi.fn(async (_details: { text: string }) => {}),
      setTitle: vi.fn(async (_details: { title: string }) => {}),
    },
  },
}));

vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));
vi.mock("../verdict-storage", () => ({ countVerdicts: mocks.countVerdicts }));

import { markBadgeSeen, refreshBadge } from "../mascot-badge";

// SW 의 OffscreenCanvas 합성 경로를 흉내내는 가짜 캔버스 — getImageData 가
// 사이즈를 박은 marker 객체를 돌려줘 어느 사이즈가 합성됐는지 검증한다.
class FakeCanvas {
  constructor(
    public width: number,
    public height: number,
  ) {}
  getContext() {
    return {
      fillStyle: "",
      drawImage: vi.fn(),
      beginPath: vi.fn(),
      arc: vi.fn(),
      fill: vi.fn(),
      getImageData: () => ({ __size: this.width }) as unknown as ImageData,
    };
  }
}

const NOW_SEC = 1_750_000_000;

describe("mascot-badge", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.counts = { pass: 0, warn: 0, fail: 0 };
    mocks.localStore.clear();
    vi.useFakeTimers();
    vi.setSystemTime(NOW_SEC * 1000);
    vi.stubGlobal("OffscreenCanvas", FakeCanvas);
    vi.stubGlobal(
      "fetch",
      vi.fn(async (_url: string) => ({ ok: true, blob: async () => ({}) })),
    );
    vi.stubGlobal(
      "createImageBitmap",
      vi.fn(async () => ({ width: 128, height: 128 })),
    );
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("미확인 fail → paw 합성 imageData 사이즈맵, 숫자 배지 제거, 건수는 툴팁에", async () => {
    mocks.counts = { pass: 1, warn: 1, fail: 2 };

    await refreshBadge();

    const { imageData, path } = mocks.browser.action.setIcon.mock.calls[0]![0];
    expect(path).toBeUndefined();
    expect(Object.keys(imageData!)).toEqual(["16", "32", "48", "128"]);
    expect(mocks.browser.action.setBadgeText).toHaveBeenCalledWith({ text: "" });
    expect(mocks.browser.action.setTitle).toHaveBeenCalledWith({
      title: "Pasu — 오늘 차단 2건",
    });
    // base(사이즈별) + paw 에셋을 절대 URL 로 fetch 했는지
    const fetched = (fetch as ReturnType<typeof vi.fn>).mock.calls.map(
      (c) => c[0] as string,
    );
    expect(fetched).toContain("chrome-extension://test-id/picture/paw-white.png");
    expect(fetched).toContain(
      "chrome-extension://test-id/picture/state-fail-16.png",
    );
  });

  it("미확인 없음(safe) → paw 없는 path 사이즈맵(절대 URL) + 배지 없음 + 보호 중", async () => {
    await refreshBadge();

    const { path, imageData } = mocks.browser.action.setIcon.mock.calls[0]![0];
    expect(imageData).toBeUndefined();
    expect(Object.keys(path!)).toEqual(["16", "32", "48", "128"]);
    for (const url of Object.values(path!)) {
      expect(url).toMatch(/^chrome-extension:\/\/test-id\/picture\/state-safe-/);
    }
    expect(mocks.browser.action.setBadgeText).toHaveBeenCalledWith({ text: "" });
    expect(mocks.browser.action.setTitle).toHaveBeenCalledWith({
      title: "Pasu — 보호 중",
    });
  });

  it("markBadgeSeen → seen 시각 기록 + seen 이전은 집계 제외(since=seenAt+1) + safe 복귀", async () => {
    await markBadgeSeen();

    expect(mocks.localStore.get("mascot_badge_seen_at")).toBe(NOW_SEC);
    expect(mocks.countVerdicts).toHaveBeenCalledWith({ since: NOW_SEC + 1 });
    const { path } = mocks.browser.action.setIcon.mock.calls[0]![0];
    expect(path!["128"]).toContain("state-safe-128.png");
  });

  it("seen 이 오래됐으면 24h 롤링 윈도우가 하한", async () => {
    mocks.localStore.set("mascot_badge_seen_at", NOW_SEC - 90_000); // 25h 전

    await refreshBadge();

    expect(mocks.countVerdicts).toHaveBeenCalledWith({ since: NOW_SEC - 86_400 });
  });

  it("OffscreenCanvas 부재 → paw 없는 path 사이즈맵으로 폴백", async () => {
    vi.stubGlobal("OffscreenCanvas", undefined);
    mocks.counts = { pass: 0, warn: 0, fail: 1 };

    await refreshBadge();

    const { path, imageData } = mocks.browser.action.setIcon.mock.calls[0]![0];
    expect(imageData).toBeUndefined();
    expect(path!["16"]).toContain("state-fail-16.png");
    expect(path!["16"]).toMatch(/^chrome-extension:\/\//);
  });

  it("warn 만 있으면 warn 마스코트 + 검토 권장 툴팁", async () => {
    mocks.counts = { pass: 0, warn: 3, fail: 0 };

    await refreshBadge();

    const { imageData } = mocks.browser.action.setIcon.mock.calls[0]![0];
    expect(Object.keys(imageData!)).toHaveLength(4);
    expect(mocks.browser.action.setTitle).toHaveBeenCalledWith({
      title: "Pasu — 검토 권장 3건",
    });
    const fetched = (fetch as ReturnType<typeof vi.fn>).mock.calls.map(
      (c) => c[0] as string,
    );
    expect(fetched).toContain(
      "chrome-extension://test-id/picture/state-warn-128.png",
    );
  });
});
