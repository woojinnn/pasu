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
    conditions: est.conditions.map((c): Condition => ({ kind: c.kind, body: exprToIr(c.body) })),
  };
  if (schema) annotate(ir, schema);
  return ir;
}

function exprToIr(node: EstExpr): Expr {
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
  if ("Set" in node) return { kind: "set", elements: (node.Set as any[]).map(exprToIr) };
  if ("Record" in node) {
    return {
      kind: "record",
      pairs: Object.entries(node.Record as Record<string, any>).map(([key, value]) => ({
        key,
        value: exprToIr(value),
      })),
    };
  }
  if ("." in node) return { kind: "attr", of: exprToIr(node["."].left), attr: node["."].attr };
  if ("has" in node) return { kind: "has", of: exprToIr(node.has.left), attr: node.has.attr };
  if ("like" in node) return { kind: "like", of: exprToIr(node.like.left), pattern: node.like.pattern };
  if ("is" in node) {
    return {
      kind: "is",
      of: exprToIr(node.is.left),
      entityType: node.is.entity_type,
      ...(node.is.in ? { in: exprToIr(node.is.in) } : {}),
    };
  }
  if ("if-then-else" in node) {
    const n = node["if-then-else"];
    return { kind: "if", cond: exprToIr(n.if), then: exprToIr(n.then), else: exprToIr(n.else) };
  }
  const k = opKey(node);
  if (k && BINARY_OPS.has(k)) {
    return { kind: "binary", op: k as BinaryOp, left: exprToIr(node[k].left), right: exprToIr(node[k].right) };
  }
  if (k && UNARY_OPS.has(k)) {
    return { kind: "unary", op: k as UnaryOp, operand: exprToIr(node[k].arg) };
  }
  // Extension call: single key whose value is an array of arg exprs (e.g.
  // { "ip": [...] }, { "isInRange": [recv, arg] }). MUST be last — Set/Record
  // and every structural single-key form are already consumed above.
  if (k && Array.isArray(node[k])) {
    return { kind: "ext", fn: k, args: (node[k] as any[]).map(exprToIr) };
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
