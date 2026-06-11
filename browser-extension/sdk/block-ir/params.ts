/**
 * # Policy parameterization (customizable fields)
 *
 * Lets a policy **author** expose specific value fields as adopter-editable
 * "parameters", and an **adopter** fill them to produce a concrete policy. Pure
 * functions over the block IR ({@link PolicyIR}) — no schema dependency, no UI.
 *
 * A *template* is just a {@link PolicyIR} that contains one or more `hole` nodes
 * ({@link HoleNode}); it is plain JSON, so store/ship it however you like.
 *
 * ## Author — turn a value into a parameter
 * ```ts
 * // The UI knows which value node the user selected. Build a hole + swap it in:
 * const hole = makeHole(selected, { name: "maxUsd", label: "Max swap (USD)", constraints: { min: 0 } });
 * const template = replaceNode(policyIr, (e) => e === selected, hole);
 * ```
 *
 * ## Adopter — fill the parameters
 * ```ts
 * const specs = extractParams(template);            // → render a form (name/label/type/constraints/default)
 * const result = fillParams(template, { maxUsd: 5000 });
 * if (result.ok) {
 *   const est = blocksToEst(result.policy);         // hole-free → serializable
 *   // const { text } = JSON.parse(est_json_to_policy_text(JSON.stringify(est)));  // WASM → Cedar text
 * } else {
 *   showErrors(result.errors);                      // reason: missing | type | range | enum | unknown
 * }
 * ```
 *
 * ## Rules
 * - Only **value nodes** (`lit` / `litEntity` / `set`) are parameterizable; `makeHole`
 *   throws otherwise.
 * - Parameters are **required by default**. Mark `optional: true` to let an unsupplied
 *   value fall back to the captured `default`; otherwise `fillParams` returns a `missing`
 *   error.
 * - Types are **author-set** (`label`/`type`/`constraints`); `expected` is inferred from
 *   the marked value's structure (no schema lookup).
 * - A template is intentionally incomplete — `blocksToEst` throws on a `hole`. Always
 *   `fillParams` to a hole-free policy before serializing.
 */

import type {
  Expr,
  Expected,
  HoleNode,
  LitType,
  ParamConstraints,
  ParamError,
  ParamFillValue,
  ParamSpec,
  PolicyIR,
} from "./ir";

// ── tree helpers ────────────────────────────────────────────────────────

/** The direct child expressions of `e` (for read-only traversal). */
export function childExprs(e: Expr): Expr[] {
  switch (e.kind) {
    case "var":
    case "lit":
    case "litEntity":
    case "raw":
    case "hole":
      return [];
    case "set":
      return e.elements;
    case "record":
      return e.pairs.map((p) => p.value);
    case "attr":
    case "has":
    case "like":
      return [e.of];
    case "binary":
      return [e.left, e.right];
    case "unary":
      return [e.operand];
    case "is":
      return e.in ? [e.of, e.in] : [e.of];
    case "if":
      return [e.cond, e.then, e.else];
    case "ext":
      return e.args;
  }
}

/** Rebuild `e` with each direct child mapped through `f`. */
function mapChildren(e: Expr, f: (x: Expr) => Expr): Expr {
  switch (e.kind) {
    case "var":
    case "lit":
    case "litEntity":
    case "raw":
    case "hole":
      return e;
    case "set":
      return { ...e, elements: e.elements.map(f) };
    case "record":
      return { ...e, pairs: e.pairs.map((p) => ({ ...p, value: f(p.value) })) };
    case "attr":
      return { ...e, of: f(e.of) };
    case "has":
      return { ...e, of: f(e.of) };
    case "like":
      return { ...e, of: f(e.of) };
    case "binary":
      return { ...e, left: f(e.left), right: f(e.right) };
    case "unary":
      return { ...e, operand: f(e.operand) };
    case "is":
      return { ...e, of: f(e.of), ...(e.in ? { in: f(e.in) } : {}) };
    case "if":
      return { ...e, cond: f(e.cond), then: f(e.then), else: f(e.else) };
    case "ext":
      return { ...e, args: e.args.map(f) };
  }
}

// ── author side ─────────────────────────────────────────────────────────

function expectedOf(value: Expr): Expected {
  switch (value.kind) {
    case "lit":
      return `lit:${value.litType}` as Expected;
    case "litEntity":
      return "litEntity";
    case "set":
      return "set";
    case "attr":
    case "var":
      // 필드 참조 RHS (`recipient != principal.address`) — 지갑별로 비교 대상
      // 필드 자체를 바꿀 수 있다.
      return "attr";
    default:
      throw new Error(`makeHole: only lit/litEntity/set/attr can be parameterized, got "${value.kind}"`);
  }
}

/** 점 경로("principal.address")를 attr 체인으로. 루트는 요청 변수여야 한다. */
export function pathToAttrExpr(path: string): Expr | null {
  const segs = path.split(".").filter(Boolean);
  const root = segs.shift();
  if (!root || !["principal", "context", "resource", "action"].includes(root)) return null;
  let e: Expr = { kind: "var", name: root as "principal" | "context" | "resource" | "action" };
  for (const seg of segs) e = { kind: "attr", of: e, attr: seg };
  return e;
}

/** attr 체인을 점 경로로 (pathToAttrExpr의 역). 루트가 변수면 경로, 아니면 null. */
export function attrExprToPath(e: Expr): string | null {
  const segs: string[] = [];
  let cur: Expr = e;
  while (cur.kind === "attr") {
    segs.unshift(cur.attr);
    cur = cur.of;
  }
  if (cur.kind !== "var") return null;
  segs.unshift(cur.name);
  return segs.join(".");
}

/** Turn a value node into a parameter hole, capturing it as the default. Throws
 *  if `value` is not a lit/litEntity/set/attr node. */
export function makeHole(
  value: Expr,
  meta: { name: string; optional?: boolean; label?: string; type?: string; constraints?: ParamConstraints },
): HoleNode {
  return {
    kind: "hole",
    name: meta.name,
    expected: expectedOf(value),
    default: value,
    ...(meta.optional ? { optional: true } : {}),
    ...(meta.label ? { label: meta.label } : {}),
    ...(meta.type ? { type: meta.type } : {}),
    ...(meta.constraints ? { constraints: meta.constraints } : {}),
  };
}

/** Replace the first expression matching `locate` (document order) with
 *  `replacement`. Pure. */
export function replaceNode(ir: PolicyIR, locate: (e: Expr) => boolean, replacement: Expr): PolicyIR {
  let done = false;
  const map = (e: Expr): Expr => {
    if (done) return e;
    if (locate(e)) {
      done = true;
      return replacement;
    }
    return mapChildren(e, map);
  };
  return { ...ir, conditions: ir.conditions.map((c) => ({ ...c, body: map(c.body) })) };
}

// ── adopter side ────────────────────────────────────────────────────────

/** Collect every hole in document order into form specs. Throws on duplicate names. */
export function extractParams(ir: PolicyIR): ParamSpec[] {
  const specs: ParamSpec[] = [];
  const visit = (e: Expr): void => {
    if (e.kind === "hole") {
      specs.push({
        name: e.name,
        expected: e.expected,
        default: e.default,
        ...(e.optional ? { optional: true } : {}),
        ...(e.label ? { label: e.label } : {}),
        ...(e.type ? { type: e.type } : {}),
        ...(e.constraints ? { constraints: e.constraints } : {}),
      });
    }
    childExprs(e).forEach(visit);
  };
  ir.conditions.forEach((c) => visit(c.body));

  const seen = new Set<string>();
  for (const s of specs) {
    if (seen.has(s.name)) throw new Error(`extractParams: duplicate param name "${s.name}"`);
    seen.add(s.name);
  }
  return specs;
}

function checkRange(s: ParamSpec, v: number, errors: ParamError[]): boolean {
  const c = s.constraints;
  if (c?.min !== undefined && v < c.min) {
    errors.push({ name: s.name, reason: "range", message: `"${s.name}" must be >= ${c.min}` });
    return false;
  }
  if (c?.max !== undefined && v > c.max) {
    errors.push({ name: s.name, reason: "range", message: `"${s.name}" must be <= ${c.max}` });
    return false;
  }
  return true;
}

function checkEnum(s: ParamSpec, v: string | number, errors: ParamError[]): boolean {
  if (s.constraints?.enum && !s.constraints.enum.includes(v)) {
    errors.push({
      name: s.name,
      reason: "enum",
      message: `"${s.name}" must be one of ${JSON.stringify(s.constraints.enum)}`,
    });
    return false;
  }
  return true;
}

/** Element literal type of a `set` default (defaults to "string" if undeterminable). */
function setElemLitType(def: Expr): LitType {
  if (def.kind === "set" && def.elements[0]?.kind === "lit") return def.elements[0].litType;
  return "string";
}

/** Validate `v` against the spec and build the replacement Expr, or push errors. */
function buildValue(s: ParamSpec, v: ParamFillValue, errors: ParamError[]): Expr | null {
  const typeErr = (msg: string): null => {
    errors.push({ name: s.name, reason: "type", message: `"${s.name}" ${msg}` });
    return null;
  };
  // 필드 참조 값은 어느 자리에서든 허용 — "값 ↔ 다른 필드" 전환도 지갑별 설정이다.
  if (typeof v === "object" && v !== null && !Array.isArray(v) && "field" in v) {
    const built = pathToAttrExpr(v.field);
    if (!built) return typeErr("expects a field path rooted at principal/context/resource/action");
    return built;
  }
  switch (s.expected) {
    case "lit:long":
      if (typeof v !== "number" || !Number.isInteger(v)) return typeErr("expects an integer");
      if (!checkRange(s, v, errors) || !checkEnum(s, v, errors)) return null;
      return { kind: "lit", litType: "long", value: v };
    case "lit:string":
      if (typeof v !== "string") return typeErr("expects a string");
      if (!checkEnum(s, v, errors)) return null;
      return { kind: "lit", litType: "string", value: v };
    case "lit:bool":
      if (typeof v !== "boolean") return typeErr("expects a boolean");
      return { kind: "lit", litType: "bool", value: v };
    case "litEntity":
      if (typeof v !== "object" || v === null || Array.isArray(v) || typeof v.type !== "string" || typeof v.id !== "string")
        return typeErr("expects { type, id }");
      return { kind: "litEntity", entity: { type: v.type, id: v.id } };
    case "set": {
      if (!Array.isArray(v)) return typeErr("expects an array");
      const elemType = setElemLitType(s.default);
      const elements: Expr[] = [];
      for (const item of v) {
        if (elemType === "long" && (typeof item !== "number" || !Number.isInteger(item))) return typeErr("elements must be integers");
        if (elemType === "bool" && typeof item !== "boolean") return typeErr("elements must be booleans");
        if (elemType === "string" && typeof item !== "string") return typeErr("elements must be strings");
        elements.push({ kind: "lit", litType: elemType, value: item });
      }
      return { kind: "set", elements };
    }
    case "attr":
      // 기본값이 필드 참조였던 자리에 lit 값을 넣는 경우 — 문자열/숫자/불리언을
      // 그대로 lit으로. (역방향 전환)
      if (typeof v === "string") return { kind: "lit", litType: "string", value: v };
      if (typeof v === "number" && Number.isInteger(v)) return { kind: "lit", litType: "long", value: v };
      if (typeof v === "boolean") return { kind: "lit", litType: "bool", value: v };
      return typeErr("expects a field path or a literal");
  }
}

/** Replace every hole by name with its precomputed replacement. */
function replaceHoles(ir: PolicyIR, repl: Map<string, Expr>): PolicyIR {
  const map = (e: Expr): Expr => (e.kind === "hole" ? (repl.get(e.name) ?? e) : mapChildren(e, map));
  return { ...ir, conditions: ir.conditions.map((c) => ({ ...c, body: map(c.body) })) };
}

/** Fill parameters with adopter-supplied values. Required params must be supplied
 *  unless `optional` (then they fall back to `default`). All errors are returned
 *  together; on success the policy is hole-free. */
export function fillParams(
  ir: PolicyIR,
  values: Record<string, ParamFillValue>,
): { ok: true; policy: PolicyIR } | { ok: false; errors: ParamError[] } {
  const specs = extractParams(ir); // also enforces unique names
  const errors: ParamError[] = [];
  const byName = new Map(specs.map((s) => [s.name, s]));

  for (const key of Object.keys(values)) {
    if (!byName.has(key)) errors.push({ name: key, reason: "unknown", message: `unknown parameter "${key}"` });
  }

  const repl = new Map<string, Expr>();
  for (const s of specs) {
    if (!Object.prototype.hasOwnProperty.call(values, s.name)) {
      if (s.optional) repl.set(s.name, s.default);
      else errors.push({ name: s.name, reason: "missing", message: `parameter "${s.name}" is required` });
      continue;
    }
    const built = buildValue(s, values[s.name], errors);
    if (built) repl.set(s.name, built);
  }

  if (errors.length) return { ok: false, errors };
  return { ok: true, policy: replaceHoles(ir, repl) };
}

/** Replace every hole with a concrete value (supplied params, else the hole's
 *  default) so hole-free consumers (`blocksToEst`, diagrams, text rendering)
 *  can run. Returns the input unchanged when it has no holes; falls back to
 *  the input if a non-optional hole can't be defaulted. */
export function concretizeIr(ir: PolicyIR, values: Record<string, ParamFillValue> = {}): PolicyIR {
  let specs: ParamSpec[];
  try {
    specs = extractParams(ir);
  } catch {
    return ir; // duplicate names etc. — leave for the editor to surface
  }
  if (specs.length === 0) return ir;
  const repl = new Map<string, Expr>();
  const errors: ParamError[] = [];
  for (const s of specs) {
    if (Object.prototype.hasOwnProperty.call(values, s.name)) {
      const built = buildValue(s, values[s.name], errors);
      repl.set(s.name, built ?? s.default);
    } else {
      repl.set(s.name, s.default);
    }
  }
  return replaceHoles(ir, repl);
}
