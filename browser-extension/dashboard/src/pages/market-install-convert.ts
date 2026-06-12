/** 마켓 리스팅 → ps2 def[] 변환(순수, 변환기 주입. 서버/브리지 의존 없음).
 *
 *  폼 호환 정책은 "내 정책" 저장과 같은 구조로 승격한다: 모든 leaf 값을 위치
 *  기반 param(v1..vN)으로 가진 holed IR + holesFromIr 파생 — 그래야 렌더
 *  (fillParams)와 바인딩별 값 오버라이드가 그대로 동작한다. 게시 측이
 *  manifest에 동봉한 x_dambi_holes 스펙(블랭킹된 칸)은 같은 위치 규칙으로
 *  계산된 것이라 이름이 일치하며, 해당 hole은 `required`로 표시되고
 *  defaults.params에서 빠진다 — SW가 충전 전 바인딩을 거부하는 판정 기준. */
import type { PolicyDef } from "../../../sdk/policy-store-types";
import type { PolicyIR } from "../cedar/blocks";
import { formToIr, irToForm, normalizeDecimal } from "../cedar/form";
import { canonicalizeModel, parameterizeModel } from "../cedar/form/parameterize";
import type { HoleSpec, HoleValue } from "../server-api/policy-store";
import { splitManifestHoles } from "./editor/publish-holes";
import { holesFromIr } from "./editor/v2/save-def";

export interface ListingMeta {
  id: string;
  kind: "policy" | "set";
  displayName: string;
  version: string;
  cat: string | undefined;
}

export interface VersionBody {
  cedar_text?: string;
  manifest?: unknown;
  members?: { slug: string; cedar_text: string; manifest?: unknown; display_name?: string }[];
}

/** 폼 호환이면 모든 leaf를 param 홀로 승격한 IR을, 아니면 null. */
function holedIrOf(ir: PolicyIR): PolicyIR | null {
  try {
    const model = irToForm(ir);
    if (!model) return null;
    return formToIr(parameterizeModel(canonicalizeModel(model)));
  } catch {
    return null;
  }
}

/** 변환 실패 항목이 있으면 전체 설치 중단(부분 설치 없음). */
export async function listingToDefs(
  meta: ListingMeta,
  body: VersionBody,
  toBlocks: (t: string) => Promise<PolicyIR[]>,
): Promise<PolicyDef[]> {
  const items =
    meta.kind === "set"
      ? (body.members ?? []).map((m) => ({
          id: `def::market.${meta.id}.${m.slug}`,
          name: m.display_name || m.slug,
          cedar: m.cedar_text,
          manifest: m.manifest,
        }))
      : [
          {
            id: `def::market.${meta.id}`,
            name: meta.displayName,
            cedar: body.cedar_text ?? "",
            manifest: body.manifest,
          },
        ];
  if (items.length === 0) throw new Error("리스팅에 설치할 정책이 없어요");

  const defs: PolicyDef[] = [];
  for (const it of items) {
    let ir: PolicyIR | undefined;
    try {
      ir = (await toBlocks(it.cedar))[0];
    } catch {
      ir = undefined;
    }
    if (!ir) throw new Error(`정책 "${it.name}"을(를) 설치 형식으로 변환할 수 없어요`);

    const { shipped, manifest } = splitManifestHoles(it.manifest);
    let skeletonIr: PolicyIR = ir;
    let holes: HoleSpec[] = [];
    let params: Record<string, HoleValue> = {};
    const holed = holedIrOf(ir);
    if (holed) {
      const derived = holesFromIr(holed);
      const byName = new Map(shipped.map((s) => [s.name, s]));
      skeletonIr = holed;
      holes = derived.holes.map((h) => {
        const s = byName.get(h.name);
        // 게시 측 스펙이 타입/라벨의 출처 — 블랭킹된 칸은 required.
        return s ? { ...h, type: s.type, label: s.label, required: true } : h;
      });
      // required hole은 기본값(플레이스홀더)을 params에 넣지 않는다 —
      // "아직 안 채워짐"의 표현이다.
      params = Object.fromEntries(
        Object.entries(derived.paramDefaults).filter(([k]) => !byName.has(k)),
      );
    } else if (shipped.length > 0) {
      // 게시자는 빈칸을 안내했지만 이쪽에서 폼으로 못 여는 정책 — 게이트를
      // 적용할 방법이 없으므로 기존 동작(있는 그대로 설치)으로 둔다.
      console.warn(`[Dambi] 리스팅 "${it.name}": hole 안내를 적용할 수 없어 무시함`);
    }

    defs.push({
      id: it.id,
      displayName: it.name,
      cat: meta.cat,
      skeleton: { ir: skeletonIr, manifest },
      holes,
      defaults: { enabled: false, params, packageId: undefined }, // 설치 선택이 채움
      source: "market",
      sourceListingId: meta.id,
      sourceVersion: meta.version,
      updatedAtMs: Date.now(),
    });
  }
  return defs;
}

/** def에서 사용자가 채워야 하는 hole 목록(설치 UI 렌더용). */
export function requiredHolesOf(def: PolicyDef): HoleSpec[] {
  return def.holes.filter((h) => h.required);
}

/** 입력 문자열 → hole 값. 형식이 안 맞으면 null (설치 버튼 비활성 근거).
 *  주소는 엔진 표기(소문자)로 정규화한다. */
export function holeInputToValue(type: HoleSpec["type"], raw: string): HoleValue | null {
  const t = raw.trim();
  const ADDR = /^0x[0-9a-fA-F]{40}$/;
  switch (type) {
    case "address":
      return ADDR.test(t) ? t.toLowerCase() : null;
    case "addressSet": {
      const items = t.split(/[\s,]+/).filter(Boolean);
      if (items.length === 0 || !items.every((a) => ADDR.test(a))) return null;
      return items.map((a) => a.toLowerCase());
    }
    case "long":
      return /^-?\d+$/.test(t) ? Number(t) : null;
    case "decimal":
      return t ? normalizeDecimal(t) : null;
    case "bool":
      return t === "true" ? true : t === "false" ? false : null;
    case "string":
      return t || null;
    case "field":
      return t ? { field: t } : null;
  }
}
