/** 에디터 저장 → ps2 페이로드 변환(순수). 신규 def는 범위 모달 입력을 defaults에 기록. */
import type { PolicyDef } from "../../../server-api/policy-store";

export type SaveScope =
  | { kind: "wallets"; addresses: string[] }
  /** "모든 지갑" — 호출 시점에 알려진 전체 주소를 명시 전달(이후 새 지갑은 defaults가 처리). */
  | { kind: "all-wallets"; addresses: string[] }
  | { kind: "library-only" };

export interface BindPlan {
  defId: string;
  packageId: string;
  addresses: string[];
}

export function buildDefPayload(opts: {
  existing: PolicyDef | null;
  displayName: string;
  cat: string | undefined;
  ir: unknown;
  manifest: unknown;
  scope: SaveScope | null; // 기존 def 저장이면 null
  packageId: string | null; // 〃
  applyToNewWallets: boolean | null; // 〃
}): { def: PolicyDef; bindPlan: BindPlan | null } {
  const skeleton = { ir: opts.ir, manifest: opts.manifest };
  if (opts.existing) {
    return {
      def: {
        ...opts.existing,
        displayName: opts.displayName,
        cat: opts.cat,
        skeleton,
        updatedAtMs: Date.now(),
      },
      bindPlan: null,
    };
  }
  const def: PolicyDef = {
    id: `def::${crypto.randomUUID()}`,
    displayName: opts.displayName,
    cat: opts.cat,
    skeleton,
    holes: [],
    defaults: {
      enabled: opts.applyToNewWallets ?? false,
      params: {},
      packageId: opts.packageId ?? undefined,
    },
    source: "mine",
    updatedAtMs: Date.now(),
  };
  const bindPlan =
    opts.scope && opts.scope.kind !== "library-only" && opts.packageId
      ? { defId: def.id, packageId: opts.packageId, addresses: opts.scope.addresses }
      : null;
  return { def, bindPlan };
}
