/** 에디터 저장 → ps2 페이로드 변환(순수). 신규 def는 범위 모달 입력을 defaults에 기록. */
import { attrExprToPath, extractParams } from "../../../cedar/blocks";
import type { PolicyIR } from "../../../cedar/blocks";
import type { HoleSpec, HoleValue, PolicyDef } from "../../../server-api/policy-store";

/** holed IR에서 def.holes + 기본 파라미터 값을 파생한다. expected → HoleSpec.type
 *  매핑은 입력 위젯 선택용(평가에는 영향 없음). */
export function holesFromIr(ir: PolicyIR): {
  holes: HoleSpec[];
  paramDefaults: Record<string, HoleValue>;
} {
  const holes: HoleSpec[] = [];
  const paramDefaults: Record<string, HoleValue> = {};
  let specs: ReturnType<typeof extractParams>;
  try {
    specs = extractParams(ir);
  } catch {
    return { holes, paramDefaults }; // 비정형/홀 없는 IR — 파라미터 없음으로 처리
  }
  for (const spec of specs) {
    const d = spec.default;
    let type: HoleSpec["type"] = "string";
    let value: HoleValue = "";
    if (d.kind === "lit" && d.litType === "long") {
      type = "long";
      value = Number(d.value);
    } else if (d.kind === "lit" && d.litType === "bool") {
      type = "bool";
      value = Boolean(d.value);
    } else if (d.kind === "lit" && d.litType === "string") {
      // decimal 홀은 ext("decimal", [lit string]) 안의 lit — 표기상 string과 같다.
      type = String(d.value).startsWith("0x") ? "address" : "string";
      value = String(d.value);
    } else if (d.kind === "set") {
      type = "addressSet";
      value = d.elements.flatMap((e) => (e.kind === "lit" ? [String(e.value)] : []));
    } else if (d.kind === "attr" || d.kind === "var") {
      type = "field";
      value = { field: attrExprToPath(d) ?? "" };
    }
    holes.push({ name: spec.name, type, label: spec.label ?? spec.name });
    paramDefaults[spec.name] = value;
  }
  return { holes, paramDefaults };
}

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
  /** 지갑 전용 정책(라이브러리 비노출) — homeWallet 지갑의 전용 폴더에 앵커. */
  walletOnly?: { homeWallet: string; walletFolderId?: string };
}): { def: PolicyDef; bindPlan: BindPlan | null } {
  const skeleton = { ir: opts.ir, manifest: opts.manifest };
  const { holes, paramDefaults } = holesFromIr(opts.ir as PolicyIR);
  if (opts.existing) {
    return {
      def: {
        ...opts.existing,
        displayName: opts.displayName,
        cat: opts.cat,
        skeleton,
        holes,
        defaults: { ...opts.existing.defaults, params: paramDefaults },
        updatedAtMs: Date.now(),
      },
      bindPlan: null,
    };
  }
  const def: PolicyDef = {
    id: `def::${crypto.randomUUID()}`,
    ...(opts.walletOnly
      ? {
          hidden: true,
          homeWallet: opts.walletOnly.homeWallet.toLowerCase(),
          walletFolderId: opts.walletOnly.walletFolderId,
        }
      : {}),
    displayName: opts.displayName,
    cat: opts.cat,
    skeleton,
    holes,
    defaults: {
      // 지갑 전용 정책은 신규 지갑 자동 적용/라이브러리 폴더와 무관하다.
      enabled: opts.walletOnly ? false : (opts.applyToNewWallets ?? false),
      params: paramDefaults,
      packageId: opts.walletOnly ? undefined : (opts.packageId ?? undefined),
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
