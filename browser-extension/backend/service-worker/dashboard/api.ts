/**
 * dashboard:* SW 메시지 — 정책 스토리지가 ps2(policy-store/)로 이관된 뒤 남은
 * 표면: ping + current-user 네임스페이스 핸드셰이크.
 * (구 put-raw/list-managed/sets/enabled-ids/catalog/audit 핸들러와 그 스토리지
 * 모듈 의존은 P3에서 제거 — 유일한 송신자였던 dashboard extension-sync v1
 * 클라이언트와 SDK v1 메서드도 함께 제거됨.)
 */
import {
  clearCurrentUserId,
  getCurrentUserId,
  setCurrentUserId,
} from "./current-user";

export type DashboardRequest =
  | { type: "dashboard:ping" }
  | { type: "dashboard:get-current-user" }
  | { type: "dashboard:set-current-user"; userId: string }
  | { type: "dashboard:clear-current-user" };

const TYPES = new Set<DashboardRequest["type"]>([
  "dashboard:ping",
  "dashboard:get-current-user",
  "dashboard:set-current-user",
  "dashboard:clear-current-user",
]);

export function isDashboardRequest(value: unknown): value is DashboardRequest {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { type?: unknown }).type === "string" &&
    TYPES.has((value as { type: DashboardRequest["type"] }).type)
  );
}

interface Ok {
  ok: true;
  data: unknown;
}
interface Fail {
  ok: false;
  error: { kind: string; message: string };
}

function fail(kind: string, message: string): Fail {
  return { ok: false, error: { kind, message } };
}

export async function handleDashboardRequest(req: DashboardRequest): Promise<Ok | Fail> {
  try {
    switch (req.type) {
      case "dashboard:ping": {
        return { ok: true, data: "pong" };
      }

      case "dashboard:get-current-user": {
        const userId = await getCurrentUserId();
        return { ok: true, data: { userId } };
      }

      case "dashboard:set-current-user": {
        if (typeof req.userId !== "string" || req.userId.length === 0) {
          return fail("invalid_request", "userId must be a non-empty string");
        }
        // ps2 스토어는 uid 네임스페이스를 호출 시점에 읽으므로 별도 재설치가
        // 없다 — 다음 resolve/조회가 새 계정 키(ps2:<uid>:*)를 본다.
        await setCurrentUserId(req.userId);
        return { ok: true, data: { userId: req.userId } };
      }

      case "dashboard:clear-current-user": {
        await clearCurrentUserId();
        return { ok: true, data: null };
      }

      default: {
        const _exhaustive: never = req;
        void _exhaustive;
        return fail("unknown_request", "unrecognized dashboard request");
      }
    }
  } catch (err) {
    return fail("internal", err instanceof Error ? err.message : String(err));
  }
}
