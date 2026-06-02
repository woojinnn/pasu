// Block IR — a thin, generic-faithful re-encoding of the Cedar EST (1:1 with its
// grammar) plus a `raw` escape node and non-authoritative schema annotations.
// EST stays the source of truth; `blocksToEst` reconstructs EST from the
// structural fields only (annotations are dropped). See the design spec.

export type Effect = "permit" | "forbid";
export interface EntityRef {
  type: string;
  id: string;
}
export type Slot = "?principal" | "?resource";

export type Scope =
  | { kind: "scopeAll" }
  | { kind: "scopeEq"; entity: EntityRef }
  | { kind: "scopeIn"; entity: EntityRef }
  | { kind: "scopeIs"; entityType: string; in?: EntityRef }
  | { kind: "slot"; slot: Slot };

export type ActionScope =
  | { kind: "scopeAll" }
  | { kind: "scopeEq"; entity: EntityRef }
  | { kind: "scopeIn"; entities: EntityRef[] };

export type VarName = "principal" | "action" | "resource" | "context";
export type LitType = "long" | "string" | "bool";
export type BinaryOp =
  | "=="
  | "!="
  | "<"
  | "<="
  | ">"
  | ">="
  | "&&"
  | "||"
  | "+"
  | "-"
  | "*"
  | "in"
  | "contains"
  | "containsAll"
  | "containsAny"
  | "getTag"
  | "hasTag";
export type UnaryOp = "!" | "neg" | "isEmpty";
export type SourceKind = "base" | "custom" | "unknown";
// Cedar EST `like` pattern: a token array of literal chars + wildcards.
export type LikePattern = ({ Literal: string } | "Wildcard")[];

export type Expr =
  | { kind: "var"; name: VarName }
  | { kind: "lit"; litType: LitType; value: number | string | boolean }
  | { kind: "litEntity"; entity: EntityRef }
  | { kind: "set"; elements: Expr[] }
  | { kind: "record"; pairs: { key: string; value: Expr }[] }
  | { kind: "attr"; of: Expr; attr: string; type?: string; source?: SourceKind; label?: string }
  // Cedar emits `x has a.b.c` as a nested `.` chain on `of` plus a single final `attr`.
  | { kind: "has"; of: Expr; attr: string }
  | { kind: "binary"; op: BinaryOp; left: Expr; right: Expr }
  | { kind: "unary"; op: UnaryOp; operand: Expr }
  | { kind: "like"; of: Expr; pattern: LikePattern }
  | { kind: "is"; of: Expr; entityType: string; in?: Expr }
  | { kind: "if"; cond: Expr; then: Expr; else: Expr }
  | { kind: "ext"; fn: string; args: Expr[] }
  | { kind: "raw"; est: unknown }
  // RESERVED for the future block UI / parameterization; not produced by estToBlocks.
  | { kind: "hole"; expected: string; name?: string; default?: unknown; label?: string };

export interface Condition {
  kind: "when" | "unless";
  body: Expr;
}

export interface PolicyIR {
  kind: "policy";
  effect: Effect;
  annotations: { name: string; value: string }[];
  scope: { principal: Scope; action: ActionScope; resource: Scope };
  conditions: Condition[];
}
