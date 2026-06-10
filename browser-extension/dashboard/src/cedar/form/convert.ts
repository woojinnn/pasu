/**
 * FormModel ⇄ PolicyIR conversion — the heart of the form editor.
 *
 * `formToIr` builds a `forbid` PolicyIR from the form's trigger + OR-of-AND
 * condition runs, auto-inserting `has` guards into each run for its
 * `context.custom.*`/optional fields (the form's safety net — the block editor
 * makes users add these by hand; an unguarded custom field fails open).
 * `irToForm` is the reverse and returns `null` for anything outside the
 * form-representable subset.
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
  FormGroupNode,
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
 * Guards are de-duped preserving order (parent-before-child) and prepended
 * inside each AND-run (positive polarity) so Cedar short-circuits cleanly —
 * and so a missing optional field only disables ITS run, not an OR-sibling.
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

/** Side maps recorded while building the IR — what the editor's click-sync
 *  (form row ↔ diagram node) needs. */
export interface FormIrMaps {
  ir: PolicyIR;
  /** Form node → the Expr(s) it produced: `[outer]`, or `[outer, inner]` for a
   *  negated node (the diagram folds `!(a<b)` to a leaf carrying the INNER
   *  comparison's path, so both are valid selection targets). */
  exprsByNode: Map<FormNode, Expr[]>;
  /** A situation(run)의 머리 노드 → run 루트 Expr (카드 헤더 선택용 게이트). */
  runRootByHead: Map<FormNode, Expr>;
}

type Recorder = Pick<FormIrMaps, "exprsByNode" | "runRootByHead">;

/** A single condition's Cedar expr, wrapped in `!(…)` when negated. */
function condExpr(c: FormCondition, rec?: Recorder): Expr {
  const inner = leafToExpr(c);
  const e: Expr = c.not ? { kind: "unary", op: "!", operand: inner } : inner;
  rec?.exprsByNode.set(c, c.not ? [e, inner] : [e]);
  return e;
}

/** Split joiner-carrying items into AND-runs (cut before each `or`). */
export function splitRuns<T extends { joiner: GroupOp }>(items: T[]): T[][] {
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

/** A node holds at least one real condition somewhere (recursively). Groups a
 *  user emptied out contribute nothing and are dropped before folding. */
function hasAnyLeaf(n: FormNode): boolean {
  return !isGroupNode(n) || n.conds.some(hasAnyLeaf);
}

/** Direct leaf conditions of an AND context (deeper groups guard themselves). */
function directLeaves(nodes: FormNode[]): FormCondition[] {
  return nodes.filter((n): n is FormCondition => !isGroupNode(n));
}

/**
 * A group's expr by nesting parity: OR of alternatives when `orCtx`, else AND.
 * `has` guards insert at the NEAREST AND context — an AND group prepends guards
 * for its direct leaves; a single-leaf OR alternative wraps just itself in
 * `(guards && leaf)` — so a missing optional field only disables its own
 * branch, never an OR sibling.
 */
function groupExpr(g: FormGroupNode, orCtx: boolean, trigger: FormTrigger, rec?: Recorder): Expr {
  const present = g.conds.filter(hasAnyLeaf);
  const terms = present.map((n) => {
    if (isGroupNode(n)) return groupExpr(n, !orCtx, trigger, rec);
    const e = condExpr(n, rec);
    if (!orCtx) return e;
    const guards = presenceGuards([n], trigger);
    return guards.length > 0 ? fold("&&", [...guards, e]) : e;
  });
  let body = fold(orCtx ? "||" : "&&", terms);
  if (!orCtx) {
    const guards = presenceGuards(directLeaves(present), trigger);
    if (guards.length > 0) body = fold("&&", [...guards, body]);
  }
  const out: Expr = g.not ? { kind: "unary", op: "!", operand: body } : body;
  rec?.exprsByNode.set(g, g.not ? [out, body] : [out]);
  return out;
}

/** One AND-run's expr: guards for the run's DIRECT leaves + the run's terms
 *  (nested groups carry their own guards — see {@link groupExpr}). */
function runExpr(run: FormNode[], trigger: FormTrigger, rec?: Recorder): Expr {
  const body = fold(
    "&&",
    run.map((n) => (isGroupNode(n) ? groupExpr(n, true, trigger, rec) : condExpr(n, rec))),
  );
  const guards = presenceGuards(directLeaves(run), trigger);
  const root = guards.length > 0 ? fold("&&", [...guards, body]) : body;
  rec?.runRootByHead.set(run[0], root);
  return root;
}

/**
 * Build a clause body from a node list. Nodes split into AND-runs at each `or`
 * joiner; a group node contributes its parity-folded sub-expr. Each AND context
 * carries its own `has` guards. Null when empty.
 */
function clauseBody(nodes: FormNode[], trigger: FormTrigger, rec?: Recorder): Expr | null {
  const present = nodes.filter(hasAnyLeaf);
  if (present.length === 0) return null;
  return fold("||", splitRuns(present).map((run) => runExpr(run, trigger, rec)));
}

/** The dead-but-normalized joiner for a group child (head "and", rest "or"). */
const groupJoiner = (i: number): GroupOp => (i === 0 ? "and" : "or");

/**
 * Parse one term of an AND context into a node: a (possibly negated) leaf, or
 * an `||`-rooted OR-group. A negated `&&` (`!(A && B)`) breaks parity and is
 * not form-representable. Null when outside the subset.
 */
function parseAndAtom(atom: Expr, joiner: GroupOp): FormNode | null {
  let not = false;
  let node = atom;
  if (node.kind === "unary" && node.op === "!") {
    not = true;
    node = node.operand;
  }
  const leaf = exprToLeaf(node);
  if (leaf) return { ...leaf, joiner, ...(not ? { not: true } : {}) };
  if (node.kind === "binary" && node.op === "||") {
    const conds = parseOrChildren(node);
    if (conds) return { kind: "group", joiner, conds, ...(not ? { not: true } : {}) };
  }
  return null;
}

/** Parse an `||` chain into an OR-group's children (each a leaf alternative or
 *  an AND-subgroup). Mutually recursive with {@link parseAndAtom}. */
function parseOrChildren(orNode: Expr): FormNode[] | null {
  const disj = flattenBinary(orNode, "||");
  const out: FormNode[] = [];
  for (let i = 0; i < disj.length; i++) {
    const n = parseOrAlternative(disj[i], groupJoiner(i));
    if (!n) return null;
    out.push(n);
  }
  return out;
}

/** One OR alternative: a (guarded) leaf, or an `&&` chain → AND-subgroup. A
 *  negated `||` alternative breaks parity → null. */
function parseOrAlternative(alt: Expr, joiner: GroupOp): FormNode | null {
  // A negated alternative: `!(F && G)` → AND-subgroup with not; `!leaf` → leaf.
  if (alt.kind === "unary" && alt.op === "!") {
    const inner = alt.operand;
    if (inner.kind === "binary" && inner.op === "&&") {
      const conds = parseAndChildren(flattenBinary(inner, "&&"));
      if (!conds) return null;
      return { kind: "group", joiner, conds, not: true };
    }
    return parseAndAtom(alt, joiner); // negated leaf (or null)
  }
  const terms = flattenBinary(alt, "&&").filter((t) => t.kind !== "has");
  if (terms.length === 0) return null; // guards-only alternative
  if (terms.length === 1) {
    // A lone `||` here would be an OR directly inside an OR (only reachable via
    // a guard wrapper like `has && (B || C)`) — shared-guard shapes are outside
    // the parity subset.
    if (terms[0].kind === "binary" && terms[0].op === "||") return null;
    return parseAndAtom(terms[0], joiner);
  }
  const conds = parseAndChildren(terms);
  if (!conds) return null;
  return { kind: "group", joiner, conds };
}

/** Parse the (guard-stripped) terms of an AND-subgroup into children. */
function parseAndChildren(terms: Expr[]): FormNode[] | null {
  const real = terms.filter((t) => t.kind !== "has");
  if (real.length === 0) return null;
  const out: FormNode[] = [];
  for (let i = 0; i < real.length; i++) {
    const n = parseAndAtom(real[i], groupJoiner(i));
    if (!n) return null;
    out.push(n);
  }
  return out;
}

/** Parse one AND-run's terms (its `has` guards stripped) into nodes. Null when
 *  nothing but guards remains (a guards-only run isn't a form condition) or a
 *  term isn't representable. */
function parseRun(run: Expr[], joiner: GroupOp): FormNode[] | null {
  const terms = run.filter((t) => t.kind !== "has");
  if (terms.length === 0) return null;
  const out: FormNode[] = [];
  for (let i = 0; i < terms.length; i++) {
    const n = parseAndAtom(terms[i], i === 0 ? joiner : "and");
    if (!n) return null;
    out.push(n);
  }
  return out;
}

/** Parse a clause body into a node list; null if not representable. `has`
 *  guards are scaffolding and stripped per run: a lone `||` expands into
 *  OR-joined AND-runs of atoms; otherwise the whole body is one run. */
function parseClause(body: Expr): FormNode[] | null {
  const top = flattenBinary(body, "&&").filter((t) => t.kind !== "has");
  if (top.length === 0) return [];

  if (top.length === 1 && top[0].kind === "binary" && top[0].op === "||") {
    const out: FormNode[] = [];
    const disj = flattenBinary(top[0], "||");
    for (let di = 0; di < disj.length; di++) {
      const run = parseRun(flattenBinary(disj[di], "&&"), di === 0 ? "and" : "or");
      if (!run) return null;
      out.push(...run);
    }
    return out;
  }
  return parseRun(top, "and");
}

// ── public API ────────────────────────────────────────────────────────────

/** Build a `forbid` PolicyIR plus the form-node↔Expr maps the editor's
 *  click-sync uses. {@link formToIr} is the map-free wrapper. */
export function formToIrWithMap(model: FormModel): FormIrMaps {
  const rec: Recorder = { exprsByNode: new Map(), runRootByHead: new Map() };
  return { ir: buildIr(model, rec), ...rec };
}

/** Build a `forbid` PolicyIR from the form model. */
export function formToIr(model: FormModel): PolicyIR {
  return buildIr(model);
}

function buildIr(model: FormModel, rec?: Recorder): PolicyIR {
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
  const whenBody = clauseBody(model.when, model.trigger, rec);
  if (whenBody) conditions.push({ kind: "when", body: whenBody });
  const unlessBody = clauseBody(model.unless, model.trigger, rec);
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
