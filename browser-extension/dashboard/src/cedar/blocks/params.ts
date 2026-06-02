// Policy parameterization (customizable fields). Author side: turn a value node
// into a named parameter `hole` (`makeHole`) and swap it into the tree
// (`replaceNode`). Adopter side (added below): `extractParams` / `fillParams`.
// Pure functions over the block IR; no schema dependency. See the design spec.

import type { Expr, Expected, HoleNode, ParamConstraints, ParamSpec, PolicyIR } from "./ir";

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
