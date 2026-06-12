/** "값 자리는 전부 파라미터" — 템플릿/인스턴스 분리의 단일 메커니즘.
 *
 *  템플릿(def)의 폼 모델에서 모든 leaf RHS(숫자·문자열·집합·필드 참조)를 위치
 *  기반 이름(v1..vN)의 파라미터로 승격한다. 구조(트리 모양·필드·연산자)가 곧
 *  뼈대이고, 값은 전부 `defaults.params`가 된다. 바인딩은 같은 구조 위에서 값만
 *  오버라이드한다. 위치 기반 이름이 안전한 이유: 바인딩이 존재하는 동안 구조는
 *  잠긴다(구조 변경 = 라이브러리에서 복제). */
import { formToIr, irToForm } from "./convert";
import { normalizeDecimal } from "./decimal";
import { isGroupNode } from "./model";
import type { FormCondition, FormModel, FormNode, FormValue } from "./model";
import type { HoleValue } from "../../server-api/policy-store";

/** 정규화: IR 왕복으로 joiner/그룹 표현을 canonical 형태로. 서로 다른 경로로
 *  만들어진 두 모델(저장된 뼈대 vs 라이브 편집)을 비교하기 전에 반드시 통과. */
export function canonicalizeModel(model: FormModel): FormModel {
  try {
    return irToForm(formToIr(model)) ?? model;
  } catch {
    return model;
  }
}

/** when → unless 순서의 DFS leaf 나열 — 파라미터 순서의 단일 정의. */
export function collectLeaves(model: FormModel): FormCondition[] {
  const out: FormCondition[] = [];
  const walk = (nodes: FormNode[]) => {
    for (const n of nodes) {
      if (isGroupNode(n)) walk(n.conds);
      else out.push(n);
    }
  };
  walk(model.when);
  walk(model.unless);
  return out;
}

/** 모든 leaf에 위치 기반 param 마커를 부여한 사본. 항상 같은 구조 → 같은 이름. */
export function parameterizeModel(model: FormModel): FormModel {
  let i = 0;
  const mapNodes = (nodes: FormNode[]): FormNode[] =>
    nodes.map((n) => {
      if (isGroupNode(n)) return { ...n, conds: mapNodes(n.conds) };
      i += 1;
      const label = n.fieldPath.split(".").pop() ?? `값 ${i}`; // i18n-ok
      return { ...n, param: { name: `v${i}`, label } };
    });
  return { ...model, when: mapNodes(model.when), unless: mapNodes(model.unless) };
}

/** 구조 동일성 키 — 값·param 마커를 뺀 (트리거, 트리 모양, 필드, 연산자, 연결).
 *  심각도/사유/이름은 메타데이터라 구조가 아니다. */
export function structureKey(model: FormModel): string {
  const nodeKey = (n: FormNode): unknown =>
    isGroupNode(n)
      ? { g: n.joiner, c: n.conds.map(nodeKey) }
      : { f: n.fieldPath, op: n.op, j: n.joiner };
  return JSON.stringify({
    trigger: model.trigger,
    when: model.when.map(nodeKey),
    unless: model.unless.map(nodeKey),
  });
}

/** FormValue → 바인딩/기본 파라미터 값. */
export function leafValueToHoleValue(v: FormValue): HoleValue {
  switch (v.kind) {
    case "bool":
      return v.value;
    case "long":
      return v.value;
    case "decimal":
      // decimal 홀은 내부 lit string — Cedar 형식("3"→"3.0")으로 정규화해 저장.
      return normalizeDecimal(v.value) ?? v.value;
    case "string":
      return v.value;
    case "set":
      return [...v.values];
    case "field":
      return { field: v.path };
  }
}

const valueEq = (a: FormValue, b: FormValue): boolean =>
  JSON.stringify(leafValueToHoleValue(a)) === JSON.stringify(leafValueToHoleValue(b));

/** 같은 구조의 두 모델에서 값이 달라진 leaf만 추출 — 인스턴스 저장의 전부.
 *  반환 키는 템플릿 leaf의 param 이름(parameterizeModel과 같은 위치 규칙). */
export function diffParamValues(
  defModel: FormModel,
  editedModel: FormModel,
): Record<string, HoleValue> {
  const defLeaves = collectLeaves(parameterizeModel(canonicalizeModel(defModel)));
  const editedLeaves = collectLeaves(canonicalizeModel(editedModel));
  const out: Record<string, HoleValue> = {};
  for (let i = 0; i < defLeaves.length; i++) {
    const d = defLeaves[i];
    const e = editedLeaves[i];
    if (!e || !d.param) continue;
    if (!valueEq(d.value, e.value)) out[d.param.name] = leafValueToHoleValue(e.value);
  }
  return out;
}
