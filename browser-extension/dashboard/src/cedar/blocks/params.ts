// Policy parameterization (customizable fields). Author side: turn a value node
// into a named parameter `hole` (`makeHole`) and swap it into the tree
// (`replaceNode`). Adopter side (added below): `extractParams` / `fillParams`.
// Pure functions over the block IR; no schema dependency. See the design spec.

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
    default:
      throw new Error(`makeHole: only lit/litEntity/set can be parameterized, got "${value.kind}"`);
  }
}

/** Turn a value node into a parameter hole, capturing it as the default. Throws
 *  if `value` is not a lit/litEntity/set node. */
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
