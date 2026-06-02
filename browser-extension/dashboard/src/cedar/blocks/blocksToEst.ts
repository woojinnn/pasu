// block IR → EST. The mechanical inverse of estToBlocks. Reconstructs EST from
// the IR's *structural* fields only — schema annotations (type/source/label) are
// ignored, which is what keeps EST→IR→EST byte-exact.

import type { PolicyIR, Expr, Scope, ActionScope } from "./ir";
import type { EstPolicy, EstExpr } from "./est";

/**
 * Convert an edited {@link PolicyIR} back into a Cedar EST. Feed the result to the
 * WASM `est_json_to_policy_text` export to get Cedar text.
 *
 * Rebuilds EST from structural fields only — display annotations
 * (`type`/`source`/`label` on `attr`) are dropped, which keeps the round trip exact.
 *
 * @throws if the IR contains an unfilled `hole` node — gate "save/export" on the IR
 *         being hole-free.
 */
export function blocksToEst(ir: PolicyIR): EstPolicy {
  const out: EstPolicy = {
    effect: ir.effect,
    principal: scopeToEst(ir.scope.principal),
    action: actionScopeToEst(ir.scope.action),
    resource: scopeToEst(ir.scope.resource),
    conditions: ir.conditions.map((c) => ({ kind: c.kind, body: exprToEst(c.body) })),
  };
  if (ir.annotations.length) {
    out.annotations = Object.fromEntries(ir.annotations.map((a) => [a.name, a.value]));
  }
  return out;
}

function exprToEst(node: Expr): EstExpr {
  switch (node.kind) {
    case "raw":
      return node.est as EstExpr;
    case "var":
      return { Var: node.name };
    case "lit":
      return { Value: node.value };
    case "litEntity":
      return { Value: { __entity: { type: node.entity.type, id: node.entity.id } } };
    case "set":
      return { Set: node.elements.map(exprToEst) };
    case "record":
      return { Record: Object.fromEntries(node.pairs.map((p) => [p.key, exprToEst(p.value)])) };
    case "attr":
      return { ".": { left: exprToEst(node.of), attr: node.attr } };
    case "has":
      return { has: { left: exprToEst(node.of), attr: node.attr } };
    case "like":
      return { like: { left: exprToEst(node.of), pattern: node.pattern } };
    case "is":
      return {
        is: {
          left: exprToEst(node.of),
          entity_type: node.entityType,
          ...(node.in ? { in: exprToEst(node.in) } : {}),
        },
      };
    case "if":
      return {
        "if-then-else": {
          if: exprToEst(node.cond),
          then: exprToEst(node.then),
          else: exprToEst(node.else),
        },
      };
    case "binary":
      return { [node.op]: { left: exprToEst(node.left), right: exprToEst(node.right) } };
    case "unary":
      return { [node.op]: { arg: exprToEst(node.operand) } };
    case "ext":
      return { [node.fn]: node.args.map(exprToEst) };
    case "hole":
      throw new Error(`blocksToEst: cannot serialize unfilled hole "${node.name}"`);
  }
}

function scopeToEst(s: Scope): Record<string, any> {
  switch (s.kind) {
    case "scopeAll":
      return { op: "All" };
    case "scopeEq":
      return { op: "==", entity: s.entity };
    case "scopeIn":
      return { op: "in", entity: s.entity };
    case "scopeIs":
      return { op: "is", entity_type: s.entityType, ...(s.in ? { in: { entity: s.in } } : {}) };
    case "slot":
      return { op: "==", slot: s.slot };
  }
}

function actionScopeToEst(s: ActionScope): Record<string, any> {
  switch (s.kind) {
    case "scopeAll":
      return { op: "All" };
    case "scopeEq":
      return { op: "==", entity: s.entity };
    case "scopeIn":
      return { op: "in", entities: s.entities };
  }
}
