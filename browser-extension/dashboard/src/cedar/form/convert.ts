/**
 * FormModel ⇄ PolicyIR conversion — the heart of the form editor.
 *
 * `formToIr` builds a `forbid` PolicyIR from the form's trigger + AND-of-OR
 * condition groups, auto-inserting `has` guards for any `context.custom.*` field
 * (the form's safety net — the block editor makes users add these by hand; an
 * unguarded custom field fails open). `irToForm` is the reverse and returns
 * `null` for anything outside the form-representable subset.
 *
 * The form is built on IR, not text, so `blocksToEst`/`blocksToText`,
 * `generateManifest`, address-casing normalization, and the diagram all apply
 * unchanged.
 */

import type {
  ActionScope,
  Condition,
  Expr,
  PolicyIR,
  VarName,
} from "../blocks/ir";

import type {
  FormCondition,
  FormLeaf,
  FormModel,
  FormNode,
  FormOp,
  FormSeverity,
  FormTrigger,
  FormValue,
  GroupOp,
} from "./model";
import { isGroupNode } from "./model";
import { guardsForPath } from "./schema-catalog";

const REQUEST_VARS = new Set<VarName>(["principal", "action", "resource", "context"]);

/** Decimal comparison ops are emitted as Cedar extension methods
 *  (`x.greaterThanOrEqual(decimal("…"))`), not binary operators. */
const OP_TO_EXT: Partial<Record<FormOp, string>> = {
  "<": "lessThan",
  "<=": "lessThanOrEqual",
  ">": "greaterThan",
  ">=": "greaterThanOrEqual",
};
const EXT_TO_OP: Record<string, FormOp> = {
  lessThan: "<",
  lessThanOrEqual: "<=",
  greaterThan: ">",
  greaterThanOrEqual: ">=",
};

const CUSTOM_PREFIX = "context.custom.";

// ── path ⇄ attr-chain ─────────────────────────────────────────────────────

/** `"context.custom.inputUsd"` → `attr(attr(var context, custom), inputUsd)`. */
function pathToExpr(path: string): Expr {
  const parts = path.split(".");
  let e: Expr = { kind: "var", name: parts[0] as VarName };
  for (let i = 1; i < parts.length; i++) e = { kind: "attr", of: e, attr: parts[i] };
  return e;
}

/** Dotted path of a pure `var`/`attr` chain, or null. */
function exprToPath(e: Expr): string | null {
  if (e.kind === "var") return REQUEST_VARS.has(e.name) ? e.name : null;
  if (e.kind === "attr") {
    const p = exprToPath(e.of);
    return p ? `${p}.${e.attr}` : null;
  }
  return null;
}

// ── value ⇄ Expr ──────────────────────────────────────────────────────────

function valueToExpr(v: FormValue): Expr {
  switch (v.kind) {
    case "bool":
      return { kind: "lit", litType: "bool", value: v.value };
    case "long":
      return { kind: "lit", litType: "long", value: v.value };
    case "string":
      return { kind: "lit", litType: "string", value: v.value };
    case "decimal":
      return { kind: "ext", fn: "decimal", args: [{ kind: "lit", litType: "string", value: v.value }] };
    case "set":
      return {
        kind: "set",
        elements: v.values.map((s) => ({ kind: "lit", litType: "string", value: s })),
      };
    case "field":
      return pathToExpr(v.path);
  }
}

function exprToValue(e: Expr): FormValue | null {
  if (e.kind === "lit") {
    if (e.litType === "bool") return { kind: "bool", value: Boolean(e.value) };
    if (e.litType === "long") return { kind: "long", value: Number(e.value) };
    if (e.litType === "string") return { kind: "string", value: String(e.value) };
  }
  if (e.kind === "ext" && e.fn === "decimal" && e.args.length === 1) {
    const a = e.args[0];
    if (a.kind === "lit") return { kind: "decimal", value: String(a.value) };
  }
  if (e.kind === "set") {
    const values: string[] = [];
    for (const el of e.elements) {
      if (el.kind === "lit" && el.litType === "string") values.push(String(el.value));
      else return null;
    }
    return { kind: "set", values };
  }
  // A pure var/attr chain → a field-vs-field comparison RHS.
  const path = exprToPath(e);
  if (path) return { kind: "field", path };
  return null;
}

// ── leaf ⇄ Expr ───────────────────────────────────────────────────────────

/** Build the Cedar Expr for one leaf — exported so the form UI can render an
 *  inline preview chip via `exprToText`. */
export function leafToExpr(leaf: FormLeaf): Expr {
  const attr = pathToExpr(leaf.fieldPath);
  const rhs = valueToExpr(leaf.value);
  // Decimal comparisons (< <= > >=) use the extension-method form.
  const extFn = leaf.value.kind === "decimal" ? OP_TO_EXT[leaf.op] : undefined;
  if (extFn) return { kind: "ext", fn: extFn, args: [attr, rhs] };
  // `in` (the form's "다음 중 하나") is membership in a LITERAL set. Cedar's
  // `in` operator is for entity-hierarchy only, so `attr in [strings]` fails
  // schema validation ("expected AnyEntity but saw String"). Emit the set's
  // `.contains(attr)` form instead — that's how Cedar tests set membership.
  if (leaf.op === "in") {
    return { kind: "binary", op: "contains", left: rhs, right: attr };
  }
  return { kind: "binary", op: leaf.op, left: attr, right: rhs };
}

function exprToLeaf(e: Expr): FormLeaf | null {
  // Decimal comparison via an extension method.
  if (e.kind === "ext" && EXT_TO_OP[e.fn] && e.args.length === 2) {
    const path = exprToPath(e.args[0]);
    const value = exprToValue(e.args[1]);
    if (!path || !value || value.kind !== "decimal") return null;
    return { fieldPath: path, op: EXT_TO_OP[e.fn], value };
  }
  // `[set].contains(attr)` is membership over a literal set — open it as the
  // form's single `in` leaf (and the form re-emits it as the same
  // `[set].contains(attr)`, the Cedar-valid form of set membership).
  if (e.kind === "binary" && e.op === "contains" && e.left.kind === "set") {
    const path = exprToPath(e.right);
    const value = exprToValue(e.left);
    if (path && value && value.kind === "set") return { fieldPath: path, op: "in", value };
    return null;
  }
  if (e.kind === "binary") {
    const op = e.op;
    if (
      op === "==" || op === "!=" || op === "<" || op === "<=" ||
      op === ">" || op === ">=" || op === "contains" || op === "in"
    ) {
      const path = exprToPath(e.left);
      const value = exprToValue(e.right);
      if (!path || !value) return null;
      // `in` takes a literal set; the scalar ops must not.
      if (op === "in" && value.kind !== "set") return null;
      if (op !== "in" && value.kind === "set") return null;
      return { fieldPath: path, op, value };
    }
  }
  return null;
}

// ── boolean tree helpers ──────────────────────────────────────────────────

function flattenBinary(e: Expr, op: "&&" | "||"): Expr[] {
  if (e.kind === "binary" && e.op === op) {
    return [...flattenBinary(e.left, op), ...flattenBinary(e.right, op)];
  }
  return [e];
}

/** Left-fold a non-empty list into a `&&`/`||` chain. */
function fold(op: "&&" | "||", terms: Expr[]): Expr {
  return terms.reduce((left, right) => ({ kind: "binary", op, left, right }));
}

/**
 * `has` guards every optional field needs before it is compared — the form's
 * safety net against fail-open policies (an unguarded optional attribute makes
 * the whole `when`/`unless` short-circuit to false). Covers:
 *   - custom fields (`context.custom.<name>` — guarded by construction), and
 *   - schema-optional fields, whose exact guard chain comes from the generated
 *     catalog under the policy's action (`context has tokenOut`,
 *     `context.tokenOut.key has address`, …).
 *
 * Guards are de-duped preserving order (parent-before-child) and prepended at
 * the top-level AND (positive polarity) so Cedar short-circuits cleanly.
 */
function presenceGuards(leaves: FormLeaf[], trigger: FormTrigger): Expr[] {
  const seen = new Set<string>();
  const pairs: { of: string; attr: string }[] = [];
  const add = (of: string, attr: string) => {
    const k = `${of}|${attr}`;
    if (!seen.has(k)) {
      seen.add(k);
      pairs.push({ of, attr });
    }
  };
  const guardPath = (path: string) => {
    if (path.startsWith(CUSTOM_PREFIX)) {
      const name = path.slice(CUSTOM_PREFIX.length).split(".")[0];
      add("context", "custom");
      if (name) add("context.custom", name);
    } else {
      for (const g of guardsForPath(trigger, path)) add(g.of, g.attr);
    }
  };
  for (const l of leaves) {
    guardPath(l.fieldPath);
    // field-vs-field RHS: the compared-against path may itself be optional.
    if (l.value.kind === "field") guardPath(l.value.path);
  }
  return pairs.map(({ of, attr }): Expr => ({ kind: "has", of: pathToExpr(of), attr }));
}

/** A single condition's Cedar expr, wrapped in `!(…)` when negated. */
function condExpr(c: FormCondition): Expr {
  const e = leafToExpr(c);
  return c.not ? { kind: "unary", op: "!", operand: e } : e;
}

/** Split joiner-carrying items into AND-runs (cut before each `or`). */
function splitRuns<T extends { joiner: GroupOp }>(items: T[]): T[][] {
  const runs: T[][] = [];
  let cur: T[] = [];
  items.forEach((c, i) => {
    if (i > 0 && c.joiner === "or") {
      runs.push(cur);
      cur = [];
    }
    cur.push(c);
  });
  runs.push(cur);
  return runs;
}

/** Boolean expr for a flat list of conditions (OR of AND-runs); no guards. */
function condsExpr(conds: FormCondition[]): Expr {
  return fold("||", splitRuns(conds).map((run) => fold("&&", run.map(condExpr))));
}

/** A node's expr — a leaf condition, or a `(…)` group's inner expr (negated). */
function nodeExpr(node: FormNode): Expr {
  if (isGroupNode(node)) {
    const inner = condsExpr(node.conds);
    return node.not ? { kind: "unary", op: "!", operand: inner } : inner;
  }
  return condExpr(node);
}

/** Every leaf condition across nodes (for has-guard collection). */
function allLeaves(nodes: FormNode[]): FormCondition[] {
  return nodes.flatMap((n) => (isGroupNode(n) ? n.conds : [n]));
}

/**
 * Build a clause body (with `has` guards) from a node list. Nodes split into
 * AND-runs at each `or` joiner; a group node contributes its parenthesized
 * sub-expr. `has` guards sit at a top-level AND (safe short-circuit). Null when
 * empty.
 */
function clauseBody(nodes: FormNode[], trigger: FormTrigger): Expr | null {
  // Drop empty group boxes (a box the user emptied) so `fold` never sees an
  // empty list.
  const present = nodes.filter((n) => !isGroupNode(n) || n.conds.length > 0);
  if (present.length === 0) return null;
  const body = fold("||", splitRuns(present).map((run) => fold("&&", run.map(nodeExpr))));
  const guards = presenceGuards(allLeaves(present), trigger);
  return guards.length > 0 ? fold("&&", [...guards, body]) : body;
}

/** Parse one term into a condition, peeling a `!(…)` negation. Null if not a leaf. */
function parseCond(expr: Expr, joiner: GroupOp): FormCondition | null {
  let not = false;
  let node = expr;
  if (node.kind === "unary" && node.op === "!") {
    not = true;
    node = node.operand;
  }
  const leaf = exprToLeaf(node);
  if (!leaf) return null;
  return { ...leaf, joiner, ...(not ? { not: true } : {}) };
}

/** Parse a sub-expr into a flat condition list (group internals; no nesting). */
function parseConds(body: Expr): FormCondition[] | null {
  const disj = body.kind === "binary" && body.op === "||" ? flattenBinary(body, "||") : [body];
  const out: FormCondition[] = [];
  for (let di = 0; di < disj.length; di++) {
    const run = flattenBinary(disj[di], "&&");
    for (let ri = 0; ri < run.length; ri++) {
      const c = parseCond(run[ri], di > 0 && ri === 0 ? "or" : "and");
      if (!c) return null;
      out.push(c);
    }
  }
  return out;
}

/** Parse one atom (a run element) into a node — a leaf, or a `(…)` group when
 *  the atom is itself a binary connective. Null if not representable. */
function parseAtom(atom: Expr, joiner: GroupOp): FormNode | null {
  let not = false;
  let node = atom;
  if (node.kind === "unary" && node.op === "!") {
    not = true;
    node = node.operand;
  }
  const leaf = exprToLeaf(node);
  if (leaf) return { ...leaf, joiner, ...(not ? { not: true } : {}) };
  if (node.kind === "binary" && (node.op === "||" || node.op === "&&")) {
    const conds = parseConds(node);
    if (conds) return { kind: "group", joiner, conds, ...(not ? { not: true } : {}) };
  }
  return null;
}

/** Parse a clause body into a node list; null if not representable. After
 *  stripping `has` guards: a lone `||` expands into OR-joined AND-runs of atoms;
 *  otherwise the top-level AND-terms become AND-joined atoms (leaf or group). */
function parseClause(body: Expr): FormNode[] | null {
  const terms = flattenBinary(body, "&&").filter((t) => t.kind !== "has");
  if (terms.length === 0) return [];

  if (terms.length === 1 && terms[0].kind === "binary" && terms[0].op === "||") {
    const disj = flattenBinary(terms[0], "||");
    const out: FormNode[] = [];
    for (let di = 0; di < disj.length; di++) {
      const run = flattenBinary(disj[di], "&&");
      for (let ri = 0; ri < run.length; ri++) {
        const n = parseAtom(run[ri], di > 0 && ri === 0 ? "or" : "and");
        if (!n) return null;
        out.push(n);
      }
    }
    return out;
  }

  const out: FormNode[] = [];
  for (const t of terms) {
    const n = parseAtom(t, "and");
    if (!n) return null;
    out.push(n);
  }
  return out;
}

// ── public API ────────────────────────────────────────────────────────────

/** Build a `forbid` PolicyIR from the form model. */
export function formToIr(model: FormModel): PolicyIR {
  const annotations: { name: string; value: string }[] = [
    { name: "id", value: model.id || "untitled-policy" },
    { name: "severity", value: model.severity },
    ...(model.reason ? [{ name: "reason", value: model.reason }] : []),
  ];

  const action: ActionScope =
    model.trigger.kind === "actionEq"
      ? { kind: "scopeEq", entity: { type: model.trigger.entityType, id: model.trigger.id } }
      : { kind: "scopeAll" };

  const conditions: Condition[] = [];
  const whenBody = clauseBody(model.when, model.trigger);
  if (whenBody) conditions.push({ kind: "when", body: whenBody });
  const unlessBody = clauseBody(model.unless, model.trigger);
  if (unlessBody) conditions.push({ kind: "unless", body: unlessBody });

  return {
    kind: "policy",
    effect: "forbid",
    annotations,
    scope: { principal: { kind: "scopeAll" }, action, resource: { kind: "scopeAll" } },
    conditions,
  };
}

/** Reverse of {@link formToIr}; `null` if `ir` is outside the form subset. */
export function irToForm(ir: PolicyIR): FormModel | null {
  if (ir.effect !== "forbid") return null;
  if (ir.scope.principal.kind !== "scopeAll" || ir.scope.resource.kind !== "scopeAll") return null;

  let trigger: FormTrigger;
  const a = ir.scope.action;
  if (a.kind === "scopeAll") trigger = { kind: "any" };
  else if (a.kind === "scopeEq") trigger = { kind: "actionEq", entityType: a.entity.type, id: a.entity.id };
  else return null; // scopeIn — not form-representable

  const id = ir.annotations.find((x) => x.name === "id")?.value ?? "untitled-policy";
  const sev = ir.annotations.find((x) => x.name === "severity")?.value;
  const severity: FormSeverity = sev === "deny" || sev === "info" || sev === "warn" ? sev : "warn";
  const reason = ir.annotations.find((x) => x.name === "reason")?.value ?? "";

  // At most one `when` and one `unless` clause.
  let when: FormNode[] = [];
  let unless: FormNode[] = [];
  let sawWhen = false;
  let sawUnless = false;
  for (const cond of ir.conditions) {
    const parsed = parseClause(cond.body);
    if (!parsed) return null;
    if (cond.kind === "when") {
      if (sawWhen) return null;
      sawWhen = true;
      when = parsed;
    } else {
      if (sawUnless) return null;
      sawUnless = true;
      unless = parsed;
    }
  }
  return { trigger, when, unless, id, severity, reason };
}
