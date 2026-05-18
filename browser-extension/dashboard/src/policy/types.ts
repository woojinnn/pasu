// Mirrors the engine's Verdict shape (kept narrow because the dashboard
// only renders these, not constructs them).

export type VerdictKind = "pass" | "warn" | "fail";

export interface MatchedPolicy {
  policy_id: string;
  reason?: string;
  severity: "deny" | "warn";
  origin: "action" | "tx" | "engine_error";
}

export interface VerdictDto {
  kind: VerdictKind;
  matched?: MatchedPolicy[];
}

export interface Envelope<T> {
  ok: boolean;
  data?: T;
  error?: { kind?: string; message?: string };
}

// Minimal PolicyRule shape from policy-builder. Kept loose (string action,
// string predicates) so the WASM JSON glue can return any registered action.
export interface PolicyRule {
  id: string;
  action: string;
  severity: "deny" | "warn";
  reason: string;
  predicates: Predicate[];
}

export interface Predicate {
  field: string;
  op: string;
  value: string | string[] | null;
}

// Mirrors the ActionSchemaDto wire shape from policy-builder-wasm —
// every field already carries the operators valid for its cedar type,
// so the UI never needs a parallel operator table.
export type CedarType =
  | "long"
  | "string"
  | "bool"
  | "decimal"
  | "set_of_string"
  | "set_of_long";

export type OperatorArity = "one" | "many" | "none";

export interface OperatorDto {
  id: string;
  label: string;
  arity: OperatorArity;
}

export interface FieldDto {
  path: string;
  type: CedarType;
  optional: boolean;
  parentPath?: string;
  parentOptional: boolean;
  label?: string;
  /**
   * `true` when the field is a manifest-contributed extension and lives
   * under `context.custom.<path>`. `false` for calldata-derived base
   * fields under `context.<path>`. The WASM compiler emits the
   * `context.custom` prefix and the `has` guard cluster automatically —
   * UIs should use this flag for grouping/labelling only, never to rewrite
   * predicate paths.
   *
   * Optional + defaulted to `false` to stay compatible with the previous
   * wire shape; the field will always be present from v1 builds of
   * policy-builder-wasm.
   */
  isCustom?: boolean;
  operators: OperatorDto[];
}

export interface ActionSchemaDto {
  action: string;
  principalType: string;
  resourceType: string;
  fields: FieldDto[];
}
