/** Hole parameter value — subset of sdk/block-ir ParamFillValue (no entity refs). */
export type HoleValue =
  | string
  | number
  | boolean
  | (string | number)[]
  /** 필드 참조 — 비교 대상 필드 자체를 지갑별로 바꾼다. */
  | { field: string };

export interface HoleSpec {
  name: string;
  type: "addressSet" | "address" | "long" | "decimal" | "string" | "bool" | "field";
  label: string;
  desc?: string;
  /** 마켓 게시 때 비식별로 블랭킹된 칸 — 사용자가 값을 채우기 전에는
   *  바인딩(패키지 적용)할 수 없다. defaults.params/binding.params 가
   *  이 이름을 덮어야 충전된 것으로 본다. */
  required?: boolean | undefined;
}

export interface PolicyDef {
  id: string; // "def::<slug>"
  /** 지갑 전용 정책 — 라이브러리 카탈로그에 노출하지 않는다. homeWallet
   *  지갑의 "지갑 전용 폴더" 트리에서만 보인다(바인딩 유무와 무관). */
  hidden?: boolean | undefined;
  /** hidden def의 앵커 지갑(소문자 주소). hidden이면 항상 있어야 한다 —
   *  normalize가 바인딩에서 추론하거나, 추론 불가면 hidden을 해제한다. */
  homeWallet?: string | undefined;
  /** homeWallet 지갑의 전용 폴더 id ("fold::<uuid>"). undefined = 그 지갑의
   *  미분류(가상 폴더). 라이브러리 폴더(defaults.packageId)와 별개 축. */
  walletFolderId?: string | undefined;
  displayName: string;
  cat?: string;
  memo?: string;
  skeleton: { ir: unknown; manifest?: unknown };
  holes: HoleSpec[];
  defaults: { enabled: boolean; params: Record<string, HoleValue>; packageId?: string };
  source: "builtin" | "mine" | "market";
  sourceListingId?: string | undefined;
  sourceVersion?: string | undefined;
  updatedAtMs: number;
}

export interface PackageDef {
  id: string; // "pkg::<slug>"
  displayName: string;
  desc?: string;
  source: "builtin" | "mine" | "market";
  sourceListingId?: string | undefined;
  sourceVersion?: string | undefined;
  updatedAtMs: number;
}

export interface Binding {
  id: string; // "bind::<uuid>"
  defId: string;
  packageId: string;
  enabled: boolean;
  /** 지갑별 별칭 — 없으면 def displayName으로 표시. */
  alias?: string | undefined;
  params?: Record<string, HoleValue> | undefined;
  updatedAtMs: number;
}

/** 지갑 소속 패키지 — 라이브러리 폴더와 별개의 객체. 지갑 화면의 생성/이름변경/
 *  제거는 이것만 만지고, 라이브러리에는 비치지 않는다. */
export interface WalletPackage {
  id: string;
  displayName: string;
  updatedAtMs: number;
}

/** 지갑 전용 폴더 — 이 지갑에서만 보이는 **템플릿(def)** 묶음. 인스턴스를
 *  묶는 패키지와 별개 축: 폴더=정리, 패키지=적용. */
export interface WalletFolder {
  id: string; // "fold::<uuid>"
  displayName: string;
  updatedAtMs: number;
}

export interface WalletPolicyState {
  bindings: Record<string, Binding>;
  /** 이 지갑의 패키지들. binding.packageId는 여기(또는 미분류)를 가리킨다. */
  packages: Record<string, WalletPackage>;
  /** 패키지 토글. 키가 없으면 true(켜짐) 취급. */
  packageEnabled: Record<string, boolean>;
  /** 이 지갑의 전용 폴더들. hidden def의 walletFolderId가 여기를 가리킨다. */
  folders?: Record<string, WalletFolder>;
}

export interface LibraryDoc {
  schemaVersion: 1;
  defs: Record<string, PolicyDef>;
  packages: Record<string, PackageDef>;
}

export interface WalletsDoc {
  schemaVersion: 1;
  /** 주소는 항상 소문자 키. */
  byAddress: Record<string, WalletPolicyState>;
}

export interface StoreSnapshot {
  library: LibraryDoc;
  wallets: WalletsDoc;
  rev: number;
}

/** 패키지에서 빠진 바인딩이 떨어지는 예약 패키지. 삭제 불가. */
export const UNCATEGORIZED_PKG = "pkg::uncategorized";

/** effective-on = 패키지 토글 ∧ 바인딩 토글 (패키지 미기록 = on). */
export function isEffectiveOn(w: WalletPolicyState, b: Binding): boolean {
  return (w.packageEnabled[b.packageId] ?? true) && b.enabled;
}

/** required hole(마켓 비식별 블랭킹) 중 merged params(def 기본값 ⊕ 바인딩
 *  오버라이드)가 못 덮는 칸의 라벨 목록. 비어 있지 않으면 그 def는 아직
 *  "빈칸" 상태 — 바인딩(패키지 적용)이 거부돼야 한다. */
export function missingRequiredHoles(
  def: Pick<PolicyDef, "holes" | "defaults">,
  params?: Record<string, HoleValue> | undefined,
): string[] {
  const merged = { ...def.defaults.params, ...(params ?? {}) };
  return def.holes
    .filter((h) => h.required && merged[h.name] === undefined)
    .map((h) => h.label || h.name);
}
