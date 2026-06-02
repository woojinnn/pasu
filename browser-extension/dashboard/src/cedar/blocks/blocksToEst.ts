// block IR → EST. The mechanical inverse of estToBlocks. Reconstructs EST from
// the IR's *structural* fields only — schema annotations (type/source/label) are
// ignored, which is what keeps EST→IR→EST byte-exact.

import type { PolicyIR, Expr, Scope, ActionScope } from "./ir";
import type { EstPolicy, EstExpr } from "./est";

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
  if (node.kind === "raw") return node.est as EstExpr;
  throw new Error(`blocksToEst: unhandled IR node kind "${node.kind}"`);
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
