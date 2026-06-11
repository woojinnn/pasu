/**
 * Publish-time hole spec — 설치자가 채워야 하는 빈칸의 "주소록".
 *
 * redactCedar가 리터럴을 플레이스홀더(제로주소/0)로 치환하고 나면 텍스트에는
 * hole이었다는 정보가 남지 않는다. 설치 측은 같은 폼 파이프라인
 * (textToBlocks → irToForm → parameterizeModel)으로 leaf마다 위치 기반 param
 * 이름(v1..vN)을 붙이므로, 게시 측에서도 redacted cedar를 같은 파이프라인에
 * 통과시켜 블랭킹된 leaf의 param 이름을 계산해 두면 양쪽 번호가 일치한다.
 *
 * 결과는 listing `manifest`의 {@link MANIFEST_HOLES_KEY} 키로 동봉한다.
 * 서버는 manifest를 불투명 JSON으로 패스스루하고(스키마 무변경), 엔진의
 * ManifestV2 역직렬화는 unknown key를 무시하므로 구버전 클라이언트가 이
 * manifest를 그대로 설치해도 평가는 깨지지 않는다 — 단 ManifestV2는
 * `{id, schema_version}`이 필수라, manifest가 없던 정책은 그 둘을 합성한
 * 위에 동봉한다.
 */

import type { PolicyIR } from "../../cedar/blocks";
import { irToForm } from "../../cedar/form";
import {
  canonicalizeModel,
  collectLeaves,
  parameterizeModel,
} from "../../cedar/form/parameterize";
import type { FormValue } from "../../cedar/form";
import type { HoleSpec } from "../../server-api/policy-store";
import { ZERO_ADDR, type PublishHole } from "./publish-redact";

/** manifest 안에서 hole 스펙이 사는 예약 키 (엔진은 unknown key 무시). */
export const MANIFEST_HOLES_KEY = "x_pasu_holes";

/** 설치자가 채워야 하는 한 칸. name은 ps2 위치 기반 param 이름(vN). */
export interface ShippedHoleSpec {
  name: string;
  type: HoleSpec["type"];
  label: string;
  required: true;
}

/** redact 결과 leaf 값이 해당 hole의 플레이스홀더인가. */
function isPlaceholder(v: FormValue, kind: PublishHole["kind"]): boolean {
  if (kind === "address") {
    if (v.kind === "string") return v.value.toLowerCase() === ZERO_ADDR;
    if (v.kind === "set")
      return v.values.length === 1 && String(v.values[0]).toLowerCase() === ZERO_ADDR;
    return false;
  }
  // number — 블랭킹은 0 (long) 또는 "0.0" (decimal lit).
  if (v.kind === "long") return v.value === 0;
  if (v.kind === "decimal") return /^0(\.0+)?$/.test(v.value);
  return false;
}

function holeType(v: FormValue): HoleSpec["type"] {
  switch (v.kind) {
    case "set":
      return "addressSet";
    case "string":
      return v.value.startsWith("0x") ? "address" : "string";
    case "long":
      return "long";
    case "decimal":
      return "decimal";
    case "bool":
      return "bool";
    case "field":
      return "field";
  }
}

/**
 * redacted cedar에서 블랭킹된 hole들의 위치 기반 param 이름을 계산한다.
 *
 * `blanked`는 redactCedar에 실제로 적용된 hole들(주소 전부 + 남기지 않은
 * 숫자). 폼 비호환(irToForm 실패) 정책이면 null — 메타데이터 없이 게시되고
 * 설치 측 게이트도 적용되지 않는다(기존 동작).
 */
export async function computeShippedHoles(
  redactedCedar: string,
  blanked: PublishHole[],
  toBlocks: (t: string) => Promise<PolicyIR[]>,
): Promise<ShippedHoleSpec[] | null> {
  if (blanked.length === 0) return [];
  let ir: PolicyIR | undefined;
  try {
    ir = (await toBlocks(redactedCedar))[0];
  } catch {
    return null;
  }
  if (!ir) return null;
  const model = irToForm(ir);
  if (!model) return null;
  const leaves = collectLeaves(parameterizeModel(canonicalizeModel(model)));

  const out: ShippedHoleSpec[] = [];
  const claimed = new Set<number>();
  for (const h of blanked) {
    const idx = leaves.findIndex(
      (l, i) => !claimed.has(i) && l.fieldPath === h.path && isPlaceholder(l.value, h.kind),
    );
    if (idx < 0) continue; // 텍스트 패턴이 폼 leaf와 안 맞는 경우 — 그 칸은 안내 불가
    claimed.add(idx);
    const leaf = leaves[idx];
    out.push({
      name: leaf.param?.name ?? `v${idx + 1}`,
      type: holeType(leaf.value),
      label: h.label,
      required: true,
    });
  }
  return out;
}

/** 게시 본문 manifest에 hole 스펙을 동봉. manifest가 없으면 유효한 최소
 *  ManifestV2(`{id, schema_version}`)를 합성한 위에 싣는다. */
export function manifestWithHoles(
  manifest: unknown,
  shipped: ShippedHoleSpec[] | null,
  ruleId: string,
): unknown {
  if (!shipped || shipped.length === 0) return manifest;
  const base =
    manifest && typeof manifest === "object" && !Array.isArray(manifest)
      ? (manifest as Record<string, unknown>)
      : { id: ruleId, schema_version: 2 };
  return { ...base, [MANIFEST_HOLES_KEY]: shipped };
}

/** 설치 측: manifest에서 hole 스펙을 분리한다. */
export function splitManifestHoles(manifest: unknown): {
  shipped: ShippedHoleSpec[];
  manifest: unknown;
} {
  if (!manifest || typeof manifest !== "object" || Array.isArray(manifest))
    return { shipped: [], manifest };
  const o = manifest as Record<string, unknown>;
  const raw = o[MANIFEST_HOLES_KEY];
  if (!Array.isArray(raw)) return { shipped: [], manifest };
  const shipped = raw.filter(
    (s): s is ShippedHoleSpec =>
      !!s && typeof s === "object" && typeof (s as ShippedHoleSpec).name === "string",
  );
  const { [MANIFEST_HOLES_KEY]: _drop, ...rest } = o;
  return { shipped, manifest: rest };
}
