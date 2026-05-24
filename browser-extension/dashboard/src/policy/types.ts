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
  /**
   * Closed-set string enum mirrored from the upstream action-schema JSON
   * (`"enum": [...]`). When present the UI should render a `<select>`
   * (arity=one) or multi-select (arity=many) of these literals instead of
   * a free-form text input — values outside this set are rejected by the
   * WASM validator with kind `"disallowed_value"`. Omitted for free-form
   * fields.
   */
  allowedValues?: string[];
  /**
   * Implicit `10^scale` exponent for Long fields whose context value is
   * pre-rescaled by the manifest (token-native amount fields use scale 9).
   * When present, the user enters a decimal-shaped string (e.g. `0.5`,
   * `100`, `0.00003`) and the WASM compiler emits `value × 10^scale` as
   * the Long literal in Cedar. Used to drive placeholder hints and accept
   * fractional input for fields the JSON wire shape declares as Long.
   */
  scale?: number;
  /**
   * Regex the operand string must match, mirrored from the upstream
   * action-schema JSON's `"pattern"` keyword (e.g. EVM address shape
   * `^0x[0-9a-fA-F]{40}$`). The WASM validator rejects out-of-shape
   * input as `kind: "pattern_mismatch"`. UIs can use this for live form
   * feedback — e.g. a red border on the value input when the user's
   * typed value doesn't match, so they notice before clicking compile.
   */
  pattern?: string;
  operators: OperatorDto[];
}

export interface ActionSchemaDto {
  action: string;
  principalType: string;
  resourceType: string;
  fields: FieldDto[];
}
