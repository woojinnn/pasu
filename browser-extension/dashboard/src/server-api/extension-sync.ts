/**
 * SW(확장 service worker) 동기화의 잔존 표면 — 정책 스토리지가 ps2(policy-store.ts)로
 * 이관된 뒤 남은 것: current-user 네임스페이스 핸드셰이크 + 에디터 id 헬퍼.
 * (구 dashboard:put-raw/sets/enabled-ids 클라이언트는 P3에서 제거됨.)
 */
import { ExtensionBridgeTimeout, sendToExtension } from "./extension-bridge";

const ID_PREFIX = "dashboard::";

/** 에디터의 작성 방식 힌트 — NewPolicyChooser 시드/탭 초기화에 사용. */
export type PolicyMethod = "form" | "block" | "cedar";

export function dashboardId(idOrName: string | number): string {
  const s = String(idOrName);
  return s.startsWith(ID_PREFIX) ? s : ID_PREFIX + s;
}

export function stripDashboardId(id: string): string {
  return id.startsWith(ID_PREFIX) ? id.slice(ID_PREFIX.length) : id;
}

/**
 * Tell the SW which user is now active. The SW uses this id to namespace
 * every per-user storage key (`ps2:<id>:*`) so a different account on the
 * same Chrome profile sees a disjoint policy space. Call this after a
 * successful `fetchMe()`. Idempotent — passing the same id is a no-op.
 */
export async function setCurrentUser(userId: string): Promise<void> {
  await sendToExtension({ type: "dashboard:set-current-user", userId });
}

/**
 * Drop the active-user discriminator. After this the SW behaves as if no
 * user is logged in (`anonymous` 네임스페이스 — builtin 보호만). Call from
 * the dashboard's logout path.
 */
export async function clearCurrentUser(): Promise<void> {
  await sendToExtension({ type: "dashboard:clear-current-user" });
}

/** Read whatever current-user id the SW currently has stored. Useful for
 *  bootstrap parity checks (dashboard fetched `Me`, does the SW agree?). */
export async function getCurrentUser(): Promise<string | null> {
  try {
    const data = await sendToExtension<{ userId: string | null }>({
      type: "dashboard:get-current-user",
    });
    return data?.userId ?? null;
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return null;
    throw err;
  }
}
