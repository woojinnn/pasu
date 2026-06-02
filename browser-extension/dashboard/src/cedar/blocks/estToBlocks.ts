// EST → block IR. Schema-aware annotations are added in a later phase; this
// build is "raw-first" — expression bodies become `raw` nodes and are promoted
// one EST node kind at a time. The policy scaffold (effect / scope / conditions /
// annotations) is mapped structurally from the start.

import type { PolicyIR, Expr, Scope, ActionScope, Condition, EntityRef } from "./ir";
import type { EstPolicy, EstExpr } from "./est";
import type { SchemaDescriptor } from "./schema";

export function estToBlocks(est: EstPolicy, _schema: SchemaDescriptor | null): PolicyIR {
  return {
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
}

// raw-first: every expression becomes a raw node (promoted in Phase 3).
function exprToIr(node: EstExpr): Expr {
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
