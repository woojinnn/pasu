// EST → block IR. Generic-faithful: one IR node per EST grammar production,
// with a `raw` escape for anything not structurally mapped. When a schema
// descriptor is supplied, a post-pass annotates `attr` nodes with type/source
// (non-authoritative — does not affect EST round-trip).

import type {
  PolicyIR,
  Expr,
  Scope,
  ActionScope,
  Condition,
  EntityRef,
  BinaryOp,
  UnaryOp,
} from "./ir";
import type { EstPolicy, EstExpr } from "./est";
import { opKey } from "./est";
import { type SchemaDescriptor, type SchemaField, attrPath, classify } from "./schema";

const BINARY_OPS = new Set<string>([
  "==", "!=", "<", "<=", ">", ">=", "&&", "||", "+", "-", "*",
  "in", "contains", "containsAll", "containsAny", "getTag", "hasTag",
]);
const UNARY_OPS = new Set<string>(["!", "neg", "isEmpty"]);

// Guard against unbounded recursion on adversarial / pathologically deep EST.
// Well below the JS call-stack limit, so we throw a clean error rather than a
// RangeError. Cedar's own parser bounds real inputs far below this.
const MAX_DEPTH = 400;

/**
 * Convert one Cedar policy's EST into a {@link PolicyIR} for a block editor to render.
 *
 * @param est    One policy's EST — from the WASM `policy_text_to_est_json` export
 *               (which returns `{ policies: [{ id, est }] }` for a document).
 * @param schema Optional enriched-schema descriptor (see `descriptorFromCustomTypes`).
 *               When given, `attr` nodes receive display-only `type`/`source`
 *               annotations; pass `null` to skip. The round trip is identical either way.
 * @returns A faithful, renderable IR; any unmapped EST construct appears as a `raw` node.
 */
export function estToBlocks(est: EstPolicy, schema: SchemaDescriptor | null): PolicyIR {
  const ir: PolicyIR = {
    kind: "policy",
    effect: est.effect,
    annotations: Object.entries(est.annotations ?? {}).map(([name, value]) => ({ name, value })),
    scope: {
      principal: scopeToIr(est.principal),
      action: actionScopeToIr(est.action),
      resource: scopeToIr(est.resource),
    },
    conditions: est.conditions.map((c): Condition => ({ kind: c.kind, body: exprToIr(c.body, 0) })),
  };
  if (schema) annotate(ir, schema);
  return ir;
}

function exprToIr(node: EstExpr, depth: number): Expr {
  if (depth > MAX_DEPTH) {
    throw new Error(`estToBlocks: expression nesting exceeds MAX_DEPTH (${MAX_DEPTH})`);
  }
  const d = depth + 1;
  if ("Var" in node) return { kind: "var", name: node.Var };
  if ("Value" in node) {
    const v = node.Value;
    if (v && typeof v === "object" && "__entity" in v) {
      return { kind: "litEntity", entity: { type: v.__entity.type, id: v.__entity.id } };
    }
    if (typeof v === "number") return { kind: "lit", litType: "long", value: v };
    if (typeof v === "string") return { kind: "lit", litType: "string", value: v };
    if (typeof v === "boolean") return { kind: "lit", litType: "bool", value: v };
    return { kind: "raw", est: node };
  }
  if ("Set" in node) return { kind: "set", elements: (node.Set as any[]).map((x) => exprToIr(x, d)) };
  if ("Record" in node) {
    return {
      kind: "record",
      pairs: Object.entries(node.Record as Record<string, any>).map(([key, value]) => ({
        key,
        value: exprToIr(value, d),
      })),
    };
  }
  if ("." in node) return { kind: "attr", of: exprToIr(node["."].left, d), attr: node["."].attr };
  if ("has" in node) return { kind: "has", of: exprToIr(node.has.left, d), attr: node.has.attr };
  if ("like" in node) return { kind: "like", of: exprToIr(node.like.left, d), pattern: node.like.pattern };
  if ("is" in node) {
    return {
      kind: "is",
      of: exprToIr(node.is.left, d),
      entityType: node.is.entity_type,
      ...(node.is.in ? { in: exprToIr(node.is.in, d) } : {}),
    };
  }
  if ("if-then-else" in node) {
    const n = node["if-then-else"];
    return { kind: "if", cond: exprToIr(n.if, d), then: exprToIr(n.then, d), else: exprToIr(n.else, d) };
  }
  const k = opKey(node);
  if (k && BINARY_OPS.has(k)) {
    return { kind: "binary", op: k as BinaryOp, left: exprToIr(node[k].left, d), right: exprToIr(node[k].right, d) };
  }
  if (k && UNARY_OPS.has(k)) {
    return { kind: "unary", op: k as UnaryOp, operand: exprToIr(node[k].arg, d) };
  }
  // Extension call: single key whose value is an array of arg exprs (e.g.
  // { "ip": [...] }, { "isInRange": [recv, arg] }). MUST be last — Set/Record
  // and every structural single-key form are already consumed above.
  if (k && Array.isArray(node[k])) {
    return { kind: "ext", fn: k, args: (node[k] as any[]).map((x) => exprToIr(x, d)) };
  }
  return { kind: "raw", est: node };
}

function entity(e: any): EntityRef {
  return { type: e.type, id: e.id };
}

function scopeToIr(s: Record<string, any>): Scope {
  switch (s.op) {
    case "All":
      return { kind: "scopeAll" };
    case "==":
      return s.slot
        ? { kind: "slot", slot: s.slot }
        : { kind: "scopeEq", entity: entity(s.entity) };
    case "in":
      return { kind: "scopeIn", entity: entity(s.entity) };
    case "is":
      return {
        kind: "scopeIs",
        entityType: s.entity_type,
        ...(s.in ? { in: entity(s.in.entity) } : {}),
      };
    default:
      return { kind: "scopeAll" };
  }
}

function actionScopeToIr(s: Record<string, any>): ActionScope {
  switch (s.op) {
    case "All":
      return { kind: "scopeAll" };
    case "==":
      return { kind: "scopeEq", entity: entity(s.entity) };
    case "in":
      return { kind: "scopeIn", entities: (s.entities ?? []).map(entity) };
    default:
      return { kind: "scopeAll" };
  }
}

// ── schema-aware annotation (non-authoritative post-pass) ───────────────

function annotate(ir: PolicyIR, schema: SchemaDescriptor): void {
  // The enriched context is per-action; only a concrete `action == Action::"X"`
  // scope yields a single action's fields.
  const action = ir.scope.action.kind === "scopeEq" ? ir.scope.action.entity.id : null;
  const fields = action ? (schema[action] ?? null) : null;
  for (const c of ir.conditions) annotateExpr(c.body, fields);
}

function annotateExpr(e: Expr, fields: SchemaField[] | null): void {
  switch (e.kind) {
    case "attr": {
      const ann = classify(attrPath(e.of, e.attr), fields);
      if (ann.type) e.type = ann.type;
      e.source = ann.source;
      annotateExpr(e.of, fields);
      break;
    }
    case "binary":
      annotateExpr(e.left, fields);
      annotateExpr(e.right, fields);
      break;
    case "unary":
      annotateExpr(e.operand, fields);
      break;
    case "has":
    case "like":
      annotateExpr(e.of, fields);
      break;
    case "is":
      annotateExpr(e.of, fields);
      if (e.in) annotateExpr(e.in, fields);
      break;
    case "if":
      annotateExpr(e.cond, fields);
      annotateExpr(e.then, fields);
      annotateExpr(e.else, fields);
      break;
    case "set":
      e.elements.forEach((x) => annotateExpr(x, fields));
      break;
    case "record":
      e.pairs.forEach((p) => annotateExpr(p.value, fields));
      break;
    case "ext":
      e.args.forEach((x) => annotateExpr(x, fields));
      break;
    // var, lit, litEntity, raw, hole: nothing to annotate
  }
}
