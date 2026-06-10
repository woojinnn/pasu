/** 목록 행의 "마지막 수정" 라벨 — v2 라이브러리 탭이 사용. */
export function mtimeLabel(updatedAtMs: number, draft: boolean): string {
  const ms = Date.now() - updatedAtMs;
  if (draft && ms < 60 * 60_000) {
    const m = Math.max(1, Math.floor(ms / 60_000));
    return `${m}분 전`;
  }
  if (ms < 60 * 60_000) {
    const m = Math.max(1, Math.floor(ms / 60_000));
    return `${m}분 전`;
  }
  if (ms < 24 * 60 * 60_000) {
    const h = Math.floor(ms / (60 * 60_000));
    return `${h}시간 전`;
  }
  if (ms < 7 * 24 * 60 * 60_000) {
    const d = Math.floor(ms / (24 * 60 * 60_000));
    return `${d}일 전`;
  }
  const w = Math.floor(ms / (7 * 24 * 60 * 60_000));
  return `${w}주 전`;
}

/** Bucket the package list by "scope" — all / loose / per-package. */
