/**
 * day1-safety 베이스라인 5종을 V2 리스트에 "지갑 처음 켤 때 5 · 기본 제공"
 * 패키지로 노출하기 위한 합성 데이터.
 *
 * 이 5종은 익스텐션에 baked 된 기본 정책이라 dashboard-managed 저장소
 * (`listManagedPolicies`)에는 없고 catalog(`getPolicyCatalog`)에만 존재한다.
 * V2 리스트는 `ManagedPolicy[]` / `PolicySet[]` 로 동작하므로, day1 정책을
 * 합성 `ManagedPolicy`(읽기전용) + 합성 `PolicySet`(readOnly) 로 만들어
 * 기존 scope/필터/토글 로직을 그대로 재사용한다. 토글은 실제 enabled-ids 를
 * 건드리므로 팝업과 양방향 실시간 동기화된다.
 *
 * 표시 이름/순서/심각도/카테고리는 팝업 store.js(TITLE_KO / DEFAULT_PKG)와 동일.
 */
import type { ManagedPolicy, PolicySet, PolicyCatalog } from "../../../server-api";

/** 합성 day1 패키지의 고정 id. 실제 dashboard-set 과 충돌하지 않도록 별도 접두사. */
export const DAY1_SET_ID = "__day1-baseline__";
export const DAY1_PKG_NAME = "지갑 처음 켤 때 5";

interface Day1Spec {
  id: string;
  name: string;
  severity: "deny" | "warn";
  /** V2 카테고리 키(categories.ts) — 팝업 칩과 동일 맥락. */
  cat: string;
}

/** 팝업과 동일한 5종·순서. */
const DAY1_SPECS: readonly Day1Spec[] = [
  { id: "unlimited-approval-deny", name: "무제한 승인 차단", severity: "deny", cat: "approvals" },
  { id: "send-first-time-or-burn-recipient-warn", name: "소각·분실 주소 전송 차단", severity: "deny", cat: "transfer" },
  { id: "unknown-blind-sign-warning", name: "정체불명 블라인드 서명 경고", severity: "warn", cat: "others" },
  { id: "permit2-sign-allowance-confirm", name: "Permit2 허용량 서명 확인", severity: "warn", cat: "approvals" },
  { id: "swap-recipient-not-self-deny", name: "스왑 수령처 = 내 지갑", severity: "deny", cat: "swap" },
];

const DAY1_IDS = new Set(DAY1_SPECS.map((s) => s.id));

export function isDay1Id(id: string): boolean {
  return DAY1_IDS.has(id);
}

/**
 * catalog 에 실제로 존재하는 day1 정책만 합성 `ManagedPolicy` 로 변환한다.
 * - `text` 에 `@severity(...)` 만 박아 severityFromCedar 가 색을 맞추게 한다.
 * - `source:"market"` + `life` 미설정(=publish) → 토글 가능하되 "내가 만듦"이 아님.
 * - `updatedAtMs:0` → "마지막 수정" 칸은 오래전으로 표시(편집 대상 아님).
 */
export function buildDay1Policies(catalog: PolicyCatalog | undefined): ManagedPolicy[] {
  const present = new Set((catalog?.policies ?? []).map((p) => p.id));
  return DAY1_SPECS.filter((s) => present.has(s.id)).map((s) => ({
    id: s.id,
    kind: "raw" as const,
    text: `@severity("${s.severity}")\n@id("${s.id}")\nforbid(principal, action, resource);`,
    displayName: s.name,
    // baked 베이스라인. PolicySource 에 baked 값이 없어 "mine" 으로 두되(=market
    // 아님 → 업데이트 배지/마켓 조회 등 market 부작용 회피), 표시 provenance 는
    // isDay1Id/DAY1_SET_ID 로 식별해 "기본 제공"으로 따로 렌더한다.
    source: "mine" as const,
    cat: s.cat,
    updatedAtMs: 0,
    schemaVersion: 1 as const,
  }));
}

/** day1 정책이 하나라도 있으면 합성 패키지(읽기전용) 하나를 만든다. */
export function buildDay1Set(day1Policies: ManagedPolicy[]): PolicySet | null {
  if (day1Policies.length === 0) return null;
  return {
    id: DAY1_SET_ID,
    displayName: DAY1_PKG_NAME,
    description: "기본 제공",
    memberIds: day1Policies.map((p) => p.id),
    // source 는 "mine"(=market 아님)으로 두고, DAY1_SET_ID 로 식별해 provenance 를
    // "기본 제공"으로 따로 렌더한다(마켓/내가 만듦 둘 다 부정확하므로).
    source: "mine",
    readOnly: true,
    updatedAtMs: 0,
    schemaVersion: 1,
  };
}
