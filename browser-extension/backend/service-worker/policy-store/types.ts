/** Hole parameter value — subset of sdk/block-ir ParamFillValue (no entity refs). */
export type HoleValue = string | number | boolean | (string | number)[];

export interface HoleSpec {
  name: string;
  type: "addressSet" | "address" | "long" | "decimal" | "string" | "bool";
  label: string;
  desc?: string;
}

export interface PolicyDef {
  id: string; // "def::<slug>"
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
  params?: Record<string, HoleValue> | undefined;
  updatedAtMs: number;
}

export interface WalletPolicyState {
  bindings: Record<string, Binding>;
  /** 패키지 토글. 키가 없으면 true(켜짐) 취급. */
  packageEnabled: Record<string, boolean>;
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
