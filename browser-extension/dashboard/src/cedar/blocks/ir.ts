/**
 * # Cedar policy block IR
 *
 * A thin, **generic-faithful** re-encoding of a Cedar policy's EST (Cedar's
 * official JSON policy format) — one IR node per EST grammar production. This is
 * the data model a block editor renders and edits; you should not need to touch
 * raw Cedar text or EST to build a UI on top of it.
 *
 * ## Data flow
 * ```text
 *  read  (text → blocks):  Cedar text ──WASM policy_text_to_est_json──▶ EST ──estToBlocks──▶ PolicyIR
 *  write (blocks → text):  PolicyIR ──blocksToEst──▶ EST ──WASM est_json_to_policy_text──▶ Cedar text
 * ```
 * The **EST is the source of truth**. `estToBlocks(est, schema)` derives a
 * `PolicyIR` for you to render; user edits mutate the IR, and `blocksToEst(ir)`
 * turns it back into EST (then the WASM bridge renders Cedar text). The round
 * trip is loss-free for the policy's *meaning* (verified by the test suite,
 * including all shipped policies and a 100k-case fuzzer).
 *
 * ## Rendering
 * Every node is a discriminated union on `kind` — `switch` on it:
 * ```ts
 * function render(e: Expr): ReactNode {
 *   switch (e.kind) {
 *     case "binary": return <Op op={e.op} l={render(e.left)} r={render(e.right)} />;
 *     case "attr":   return <Field name={e.attr} type={e.type} source={e.source} of={render(e.of)} />;
 *     case "lit":    return <Literal value={e.value} />;
 *     case "var":    return <Var name={e.name} />;
 *     // ...one arm per kind; `raw` and `hole` are the two escape hatches.
 *   }
 * }
 * ```
 *
 * ## Three things to know
 * 1. **Annotations are non-authoritative.** `attr.type` / `attr.source` /
 *    `attr.label` are for display & styling only (e.g. dashed border for a custom
 *    manifest field, a USD widget for a `UsdValuation`). `blocksToEst` ignores
 *    them — don't treat them as truth and don't worry about preserving them on save.
 * 2. **`raw` is the escape hatch.** Any EST node the engine doesn't structurally
 *    map surfaces as `{ kind: "raw", est }`. Render it read-only (e.g. as a code
 *    chip). For every shipped policy this never occurs (see real-policies test),
 *    but render defensively.
 * 3. **`hole` is reserved** for parameterization (the customizable-fields feature)
 *    — a named parameter slot ({@link HoleNode}). `estToBlocks` never emits one;
 *    `blocksToEst` THROWS on an unfilled hole, so gate "export/save" on a hole-free IR.
 *
 * Obtain/return values via {@link estToBlocks} and {@link blocksToEst}.
 */

/** Policy effect: `permit` allows, `forbid` denies. */
export type Effect = "permit" | "forbid";

/** A Cedar entity reference, e.g. `Action::"Swap"` → `{ type: "Action", id: "Swap" }`.
 *  Namespaces are folded into `type` (e.g. `My::Ns::User` → `type: "My::Ns::User"`). */
export interface EntityRef {
  type: string;
  id: string;
}

/** A template slot placeholder in a policy scope (`?principal` / `?resource`). */
export type Slot = "?principal" | "?resource";

/**
 * A principal/resource scope head:
 * - `scopeAll`  → unconstrained (`principal` / `resource`)
 * - `scopeEq`   → `== Entity::"id"`
 * - `scopeIn`   → `in Entity::"id"`
 * - `scopeIs`   → `is Type` (optionally `is Type in Entity::"id"`)
 * - `slot`      → a template slot (`?principal` / `?resource`)
 */
export type Scope =
  | { kind: "scopeAll" }
  | { kind: "scopeEq"; entity: EntityRef }
  | { kind: "scopeIn"; entity: EntityRef }
  | { kind: "scopeIs"; entityType: string; in?: EntityRef }
  | { kind: "slot"; slot: Slot };

/** The action scope head. Like {@link Scope} but `in` takes a *set* of actions
 *  (`action in [Action::"a", Action::"b"]`) and there is no `is` / slot form. */
export type ActionScope =
  | { kind: "scopeAll" }
  | { kind: "scopeEq"; entity: EntityRef }
  | { kind: "scopeIn"; entities: EntityRef[] };

/** The four Cedar request variables usable in expressions. */
export type VarName = "principal" | "action" | "resource" | "context";

/** Primitive literal flavor carried on a `lit` node (for picking an input widget). */
export type LitType = "long" | "string" | "bool";

/** Binary operators. All render as `<left> <op> <right>`. Note `getTag`/`hasTag`
 *  are method-like in source (`x.getTag(y)`) but binary in shape here. */
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

/** Unary operators: `!` (not), `neg` (numeric negation), `isEmpty` (set is empty). */
export type UnaryOp = "!" | "neg" | "isEmpty";

/** Where an `attr` field comes from, for styling:
 *  - `base`    → a calldata-derived field defined in the base schema
 *  - `custom`  → a manifest-enriched field (lives under `context.custom.*`)
 *  - `unknown` → not found in the (enriched) schema, or no schema was supplied */
export type SourceKind = "base" | "custom" | "unknown";

/** A Cedar `like` pattern as a token array of literal chars and wildcards,
 *  e.g. `"a*b"` → `[{ Literal: "a" }, "Wildcard", { Literal: "b" }]`. Render by
 *  concatenating literals and showing `*` for each `"Wildcard"`. */
export type LikePattern = ({ Literal: string } | "Wildcard")[];

/**
 * A Cedar expression node. Discriminate on `kind`. Mirrors the Cedar EST 1:1, so
 * any valid policy is representable; unmapped nodes fall back to `raw`.
 */
export type Expr =
  /** A request variable: `principal` / `action` / `resource` / `context`. */
  | { kind: "var"; name: VarName }
  /** A primitive literal. `litType` tells you which (`long`/`string`/`bool`). */
  | { kind: "lit"; litType: LitType; value: number | string | boolean }
  /** An entity literal used as a value, e.g. `User::"alice"`. */
  | { kind: "litEntity"; entity: EntityRef }
  /** A set literal `[a, b, c]`. */
  | { kind: "set"; elements: Expr[] }
  /** A record literal `{ k1: v1, k2: v2 }`. `pairs` preserves source order. */
  | { kind: "record"; pairs: { key: string; value: Expr }[] }
  /**
   * Attribute access `of.attr` (the `.` operator). For a dotted path like
   * `context.custom.amount`, `of` is itself an `attr`/`var` chain.
   * `type` / `source` / `label` are **display-only** annotations (see module docs).
   */
  | { kind: "attr"; of: Expr; attr: string; type?: string; source?: SourceKind; label?: string }
  /** Attribute presence test `of has attr` (Cedar nests `has a.b.c` as a `.`
   *  chain on `of` plus the final single `attr`). */
  | { kind: "has"; of: Expr; attr: string }
  /** Binary operation `left <op> right` (see {@link BinaryOp}). */
  | { kind: "binary"; op: BinaryOp; left: Expr; right: Expr }
  /** Unary operation (see {@link UnaryOp}). */
  | { kind: "unary"; op: UnaryOp; operand: Expr }
  /** String pattern match `of like <pattern>` (see {@link LikePattern}). */
  | { kind: "like"; of: Expr; pattern: LikePattern }
  /** Entity-type test `of is Type` (optionally `is Type in <in>`). */
  | { kind: "is"; of: Expr; entityType: string; in?: Expr }
  /** Conditional `if cond then <then> else <else>`. */
  | { kind: "if"; cond: Expr; then: Expr; else: Expr }
  /** Extension function call, e.g. `ip("…")`, `decimal("…")`, `x.isInRange(y)`.
   *  `fn` is the function name; `args` are the arguments (receiver first for
   *  method-style calls). */
  | { kind: "ext"; fn: string; args: Expr[] }
  /** ESCAPE HATCH: an EST node not structurally mapped. Carries the verbatim EST
   *  subtree; round-trips losslessly. Render read-only. */
  | { kind: "raw"; est: unknown }
  /** RESERVED (not produced by `estToBlocks`): a named parameter slot for the
   *  customizable-fields feature (see {@link HoleNode}). `blocksToEst` throws on
   *  an unfilled hole. */
  | HoleNode;

/** One `when` / `unless` clause of a policy. A policy's conditions are ANDed. */
export interface Condition {
  kind: "when" | "unless";
  body: Expr;
}

/**
 * A whole Cedar policy as blocks — the top-level value you render and edit.
 * `annotations` are policy-level `@key("value")` pairs (e.g. `@id`, `@severity`),
 * in source order.
 */
export interface PolicyIR {
  kind: "policy";
  effect: Effect;
  annotations: { name: string; value: string }[];
  scope: { principal: Scope; action: ActionScope; resource: Scope };
  conditions: Condition[];
}

// ── Parameterization (customizable fields) ──────────────────────────────

/** Structural shape a parameter hole fills — inferred from the marked value node. */
export type Expected = "lit:long" | "lit:string" | "lit:bool" | "litEntity" | "set";

/** Adopter-input constraints an author may attach to a parameter. */
export interface ParamConstraints {
  min?: number;
  max?: number;
  enum?: (string | number)[];
}

/** A parameter slot: a value node the author exposed for adopters to edit.
 *  `default` is the captured original value (applied only when `optional` and
 *  unsupplied). `type`/`label` are author-set display hints. */
export interface HoleNode {
  kind: "hole";
  name: string;
  expected: Expected;
  default: Expr;
  optional?: boolean;
  label?: string;
  type?: string;
  constraints?: ParamConstraints;
}

/** Form spec for one parameter, surfaced to the adopter UI. */
export interface ParamSpec {
  name: string;
  expected: Expected;
  default: Expr;
  optional?: boolean;
  label?: string;
  type?: string;
  constraints?: ParamConstraints;
}

/** A parameterized policy artifact: a PolicyIR containing one or more HoleNodes. */
export interface PolicyTemplate {
  version: 1;
  policy: PolicyIR;
}

/** A value an adopter supplies for a parameter, shaped per the hole's `expected`. */
export type ParamFillValue =
  | number
  | string
  | boolean
  | (string | number)[]
  | { type: string; id: string };

/** One validation failure from `fillParams`, surfaced to the adopter form. */
export interface ParamError {
  name: string;
  reason: "missing" | "type" | "range" | "enum" | "unknown";
  message: string;
}
