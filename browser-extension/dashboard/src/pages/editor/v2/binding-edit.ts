/** 지갑 바인딩 편집의 핵심: 편집된 구체 IR을 def 뼈대와 자리맞춤 비교해서
 *  "값만 바뀜"과 "구조가 바뀜"을 가른다.
 *
 *  값만 바뀌면 그 자리의 def 값 노드를 (숨은) optional 홀로 승격하고 — 기본값은
 *  원래 값이라 다른 지갑은 영향 없음 — 바뀐 값은 이 바인딩의 params가 된다.
 *  구조가 바뀌면 호출측이 "이 지갑 전용 복제" 흐름으로 분기한다. 홀은 사용자에게
 *  보이지 않는 내부 메커니즘이다. */
import { makeHole } from "../../../cedar/blocks";
import type { Expr, PolicyIR } from "../../../cedar/blocks";
import type { HoleValue, PolicyDef } from "../../../server-api/policy-store";
import { holesFromIr } from "./save-def";

export interface ParamUpdate {
  /** def 뼈대의 그 자리 노드(identity) — 홀이거나 승격 대상 값 노드. */
  defNode: Expr;
  /** 이미 홀이면 그 이름(승격 불필요, params만 갱신). */
  existingHole: string | null;
  /** 편집된 구체 값 노드. */
  newValue: Expr;
  /** 승격 시 라벨 — 비교 LHS 필드 경로 꼬리. */
  labelHint: string;
}

export type BindingDiff =
  | { kind: "unchanged" }
  | { kind: "params"; updates: ParamUpdate[] }
  | { kind: "structural" };

const isValueNode = (e: Expr): boolean =>
  e.kind === "lit" || e.kind === "set" || e.kind === "litEntity";

function litEq(a: Expr, b: Expr): boolean {
  return (
    a.kind === "lit" && b.kind === "lit" && a.litType === b.litType && a.value === b.value
  );
}

function setEq(a: Expr, b: Expr): boolean {
  if (a.kind !== "set" || b.kind !== "set") return false;
  if (a.elements.length !== b.elements.length) return false;
  return a.elements.every((el, i) => litEq(el, b.elements[i]));
}

/** 노드의 정렬용 자식 목록 — 두 트리를 같은 순서로 걷는다. */
function children(e: Expr): Expr[] {
  switch (e.kind) {
    case "binary":
      return [e.left, e.right];
    case "unary":
      return [e.operand];
    case "ext":
      return e.args;
    case "has":
    case "attr":
      return [e.of];
    case "set":
      return e.elements;
    case "record":
      return e.pairs.map((p) => p.value);
    default:
      return [];
  }
}

/** 재귀 자리맞춤 비교. 값 차이는 updates에 누적, 그 외 불일치는 structural. */
function walk(
  d: Expr,
  e: Expr,
  labelHint: string,
  updates: ParamUpdate[],
): boolean {
  // def 쪽 홀: 편집값은 항상 구체 — 그 자리 값을 params 갱신으로 기록.
  if (d.kind === "hole") {
    const inner = isValueNode(e) ? e : null;
    if (!inner) return false;
    updates.push({ defNode: d, existingHole: d.name, newValue: inner, labelHint });
    return true;
  }
  if (d.kind !== e.kind) {
    // 값 노드끼리의 종류 차이(lit↔set 등)는 구조 변경으로 취급.
    return false;
  }
  if (d.kind === "lit" && e.kind === "lit") {
    if (litEq(d, e)) return true;
    if (d.litType !== e.litType) return false;
    updates.push({ defNode: d, existingHole: null, newValue: e, labelHint });
    return true;
  }
  if (d.kind === "set" && e.kind === "set") {
    if (setEq(d, e)) return true;
    // 셋은 통째로 하나의 값 — 원소 구성이 다르면 값 변경.
    const allLit = (s: Expr) =>
      s.kind === "set" && s.elements.every((el) => el.kind === "lit" && el.litType === "string");
    if (!allLit(d) || !allLit(e)) return false;
    updates.push({ defNode: d, existingHole: null, newValue: e, labelHint });
    return true;
  }
  // 비-값 노드: 판별자(연산자/속성명 등)가 같아야 한다.
  switch (d.kind) {
    case "binary":
      if (e.kind !== "binary" || d.op !== e.op) return false;
      break;
    case "unary":
      if (e.kind !== "unary" || d.op !== e.op) return false;
      break;
    case "ext":
      if (e.kind !== "ext" || d.fn !== e.fn || d.args.length !== e.args.length) return false;
      break;
    case "attr":
      if (e.kind !== "attr" || d.attr !== e.attr) return false;
      break;
    case "has":
      if (e.kind !== "has" || d.attr !== e.attr) return false;
      break;
    case "var":
      return e.kind === "var" && d.name === e.name;
    case "litEntity":
      return (
        e.kind === "litEntity" && d.entity.type === e.entity.type && d.entity.id === e.entity.id
      );
    case "record":
      if (e.kind !== "record" || d.pairs.length !== e.pairs.length) return false;
      if (!d.pairs.every((p, i) => e.pairs[i].key === p.key)) return false;
      break;
    case "raw":
      // 불투명 노드는 동일성 판단 불가 — 보수적으로 구조 변경 취급.
      return false;
    default:
      return false;
  }
  const dc = children(d);
  const ec = children(e);
  if (dc.length !== ec.length) return false;
  // RHS로 내려갈 때 LHS attr 경로 꼬리를 라벨 힌트로 넘긴다.
  let hint = labelHint;
  if (d.kind === "binary") {
    // 비교의 필드 쪽 attr 이름을 힌트로 — contains(set, attr)처럼 우측일 수도 있다.
    if (d.left.kind === "attr") hint = d.left.attr;
    else if (d.right.kind === "attr") hint = d.right.attr;
  }
  if (d.kind === "ext" && dc[0]?.kind === "attr") hint = dc[0].attr;
  for (let i = 0; i < dc.length; i++) {
    if (!walk(dc[i], ec[i], hint, updates)) return false;
  }
  return true;
}

export function diffBindingEdit(defIr: PolicyIR, editedIr: PolicyIR): BindingDiff {
  // 트리거/effect/annotations(심각도·사유)가 다르면 구조 변경. `@id`는 제외 —
  // 정체성 표식이라 에디터가 def id로 다시 찍는데, baked 원본의 슬러그와 달라서
  // 모든 값 편집이 구조 변경으로 오판된다. 비교는 이름→값 맵(순서 무관).
  const meta = (ir: PolicyIR) => {
    const ann = Object.fromEntries(
      (ir.annotations ?? []).filter((a) => a.name !== "id").map((a) => [a.name, a.value]),
    );
    return JSON.stringify({ e: ir.effect, a: ann, s: ir.scope });
  };
  if (meta(defIr) !== meta(editedIr)) return { kind: "structural" };
  if (defIr.conditions.length !== editedIr.conditions.length) return { kind: "structural" };

  const updates: ParamUpdate[] = [];
  for (let i = 0; i < defIr.conditions.length; i++) {
    const d = defIr.conditions[i];
    const e = editedIr.conditions[i];
    if (d.kind !== e.kind) return { kind: "structural" };
    if (!walk(d.body, e.body, "값", updates)) return { kind: "structural" };
  }
  return updates.length === 0 ? { kind: "unchanged" } : { kind: "params", updates };
}

function exprToHoleValue(e: Expr): HoleValue {
  if (e.kind === "lit") {
    if (e.litType === "long") return Number(e.value);
    if (e.litType === "bool") return Boolean(e.value);
    return String(e.value);
  }
  if (e.kind === "set") {
    return e.elements.flatMap((el) => (el.kind === "lit" ? [String(el.value)] : []));
  }
  return String((e as { value?: unknown }).value ?? "");
}

function mapExpr(e: Expr, repl: Map<Expr, Expr>): Expr {
  const hit = repl.get(e);
  if (hit) return hit;
  switch (e.kind) {
    case "binary":
      return { ...e, left: mapExpr(e.left, repl), right: mapExpr(e.right, repl) };
    case "unary":
      return { ...e, operand: mapExpr(e.operand, repl) };
    case "ext":
      return { ...e, args: e.args.map((a) => mapExpr(a, repl)) };
    case "attr":
    case "has":
      return { ...e, of: mapExpr(e.of, repl) };
    case "set":
      return { ...e, elements: e.elements.map((el) => mapExpr(el, repl)) };
    case "record":
      return { ...e, pairs: e.pairs.map((p) => ({ ...p, value: mapExpr(p.value, repl) })) };
    default:
      return e;
  }
}

/** params diff를 적용: 새 승격은 def 뼈대에 숨은 홀을 심고(기본값 = 원래 값),
 *  반환된 params를 바인딩에 저장하면 이 지갑만 새 값으로 평가된다. */
export function applyBindingEdit(
  def: PolicyDef,
  updates: ParamUpdate[],
): { def: PolicyDef; params: Record<string, HoleValue> } {
  const params: Record<string, HoleValue> = {};
  const repl = new Map<Expr, Expr>();
  for (const u of updates) {
    if (u.existingHole) {
      params[u.existingHole] = exprToHoleValue(u.newValue);
      continue;
    }
    const name = `${u.labelHint.replace(/[^A-Za-z0-9]/g, "") || "value"}_${Math.random().toString(36).slice(2, 6)}`;
    repl.set(u.defNode, makeHole(u.defNode, { name, label: u.labelHint, optional: true }));
    params[name] = exprToHoleValue(u.newValue);
  }

  if (repl.size === 0) return { def, params };

  const ir = def.skeleton.ir as PolicyIR;
  const newIr: PolicyIR = {
    ...ir,
    conditions: ir.conditions.map((c) => ({ ...c, body: mapExpr(c.body, repl) })),
  };
  const { holes, paramDefaults } = holesFromIr(newIr);
  return {
    def: {
      ...def,
      skeleton: { ...def.skeleton, ir: newIr },
      holes,
      defaults: { ...def.defaults, params: paramDefaults },
      updatedAtMs: Date.now(),
    },
    params,
  };
}
