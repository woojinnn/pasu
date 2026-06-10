/**
 * PolicyDiagram — a UML-feel SVG tree of a Cedar policy's logical structure.
 *
 * Renders a {@link PolicyIR} as nodes + edges: the `forbid`/`permit` head at the
 * root, `WHEN`/`UNLESS` clauses, `AND`/`OR`/`NOT`/`IF` boolean gates, and atomic
 * conditions as leaf boxes. AND/OR chains are flattened so a multi-term
 * conjunction reads as one gate with N children (rather than a nested binary
 * staircase).
 *
 * Renderer-agnostic to its data: pass `highlightPaths` (a set of structural node
 * paths, e.g. from the denial-diagnosis core) to red-trace the sub-clause(s)
 * responsible for a block. Without it the diagram is purely structural — the
 * mode used in the editor while authoring a policy.
 *
 * Same component powers four surfaces (editor structure view, simulation
 * verdict, history detail, confirm popup) — only the data source and `compact`
 * sizing differ.
 */
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import type { BinaryOp, Expr, PolicyIR } from "../blocks/ir";
import { isAllOf, setLiteralOperand } from "../diagnosis/membership";
import { eachChild, pathByNode } from "../diagnosis/path";
import { naturalCondition } from "../nl";
import { getGloss } from "../../editor-v9/gloss/paths";

import "./policy-diagram.css";

type NodeKind = "root" | "when" | "unless" | "and" | "or" | "not" | "if" | "leaf" | "memberset";

/** A membership fan-out (`x in [a, b, …]`) rendered as ONE box with the members
 *  as chips (instead of N connected boxes). Each member keeps its canonical path
 *  so a diagnosis can still highlight the individual chip that matched. */
interface MemberSet {
  mode: "any" | "all";
  members: { text: string; path: string }[];
}

interface DNode {
  /** Structural path, stable across renders; the unit `highlightPaths` targets. */
  path: string;
  kind: NodeKind;
  /** Primary label (e.g. "AND", or a leaf's condition text). */
  title: string;
  /** Optional secondary line (e.g. the action under a FORBID head, or an
   *  IF branch tag like "then"). */
  detail?: string;
  /** Present on a `memberset` node — the chips to render inside the box. */
  memberset?: MemberSet;
  children: DNode[];
}

// ── IR → tree ────────────────────────────────────────────────────────────

/** Flatten a same-operator binary chain into its leaf operands, preserving each
 *  operand's identity so its canonical path resolves via {@link pathByNode}:
 *  `A && B && C` → `[A, B, C]`. */
function flatten(e: Expr, op: "&&" | "||"): Expr[] {
  if (e.kind === "binary" && e.op === op) {
    return [...flatten(e.left, op), ...flatten(e.right, op)];
  }
  return [e];
}

/** Dotted access path of a `var`/`attr` chain (e.g. `context.custom.amount`),
 *  or null for anything else. */
function attrPath(e: Expr): string | null {
  if (e.kind === "var") return e.name;
  if (e.kind === "attr") {
    const p = attrPath(e.of);
    return p ? `${p}.${e.attr}` : null;
  }
  return null;
}

/** Every access path read anywhere inside `e` (each `attr` node's dotted path). */
function collectAccessed(e: Expr, out: Set<string>): void {
  if (e.kind === "attr") {
    const p = attrPath(e);
    if (p) out.add(p);
  }
  for (const c of eachChild(e)) collectAccessed(c.node, out);
}

/**
 * Drop `has` presence-guards from an AND group when the path they guard is
 * actually read by a sibling. `context has custom && context.custom has amount
 * && context.custom.amount < 5` carries the first two clauses only to safely
 * reach `context.custom.amount` — they're scaffolding, not conditions, so the
 * diagram hides them and shows just the real comparison. Never hides everything
 * (a group of only-guards keeps them).
 */
function dropHasGuards(operands: Expr[]): Expr[] {
  const accessed = new Set<string>();
  for (const op of operands) {
    if (op.kind === "has") continue; // a guard's own `of` is scaffolding too
    collectAccessed(op, accessed);
  }
  const kept = operands.filter((op) => {
    if (op.kind !== "has") return true;
    const base = attrPath(op.of);
    const guarded = base ? `${base}.${op.attr}` : null;
    return !(guarded && accessed.has(guarded));
  });
  return kept.length > 0 ? kept : operands;
}

/**
 * Convert an Expr to a diagram node. Node paths come from {@link pathByNode}
 * (the diagnosis module's single path producer), so they are byte-identical to
 * the `culprits`/`errored` paths a diagnosis returns — that is what lets
 * `highlightPaths` light up the right node. AND/OR chains are flattened for a
 * clean gate, but each flattened operand keeps its true nested path.
 */
function exprToNode(e: Expr, pathOf: Map<Expr, string>): DNode {
  const path = pathOf.get(e) ?? "?";
  if (e.kind === "binary" && (e.op === "&&" || e.op === "||")) {
    let operands = flatten(e, e.op);
    if (e.op === "&&") operands = dropHasGuards(operands);
    // A gate with one surviving operand adds no information — show it directly.
    if (operands.length === 1) return exprToNode(operands[0], pathOf);
    return {
      path,
      kind: e.op === "&&" ? "and" : "or",
      title: e.op === "&&" ? "모두 해당" : "하나라도 해당",
      children: operands.map((c) => exprToNode(c, pathOf)),
    };
  }
  // Fold `!(a < b)` → a single `a ≥ b` leaf: the negated comparison reads far
  // cleaner than a NOT gate over a `<`. The leaf carries the INNER comparison's
  // canonical path (what the diagnosis blames), so highlight still lines up.
  if (
    e.kind === "unary" &&
    e.op === "!" &&
    e.operand.kind === "binary" &&
    NEGATE_BINARY[e.operand.op]
  ) {
    const inner = e.operand;
    const negated: Expr = { kind: "binary", op: NEGATE_OP[inner.op] ?? inner.op, left: inner.left, right: inner.right };
    return { path: pathOf.get(inner) ?? path, kind: "leaf", ...leafParts(negated), children: [] };
  }
  // `!(x contains v)` / `!([…].contains(x))` — the form's negative membership
  // ops. Fold to a leaf/memberset phrased negatively, carrying the INNER
  // comparison's path (what a diagnosis blames) so highlight lines up.
  if (
    e.kind === "unary" &&
    e.op === "!" &&
    e.operand.kind === "binary" &&
    e.operand.op === "contains"
  ) {
    const inner = e.operand;
    const innerPath = pathOf.get(inner) ?? path;
    const mem = setLiteralOperand(inner);
    if (mem) {
      const fieldPath = attrPath(mem.other);
      return {
        path: innerPath,
        kind: "memberset",
        title: (fieldPath && getGloss(fieldPath)?.ko) || exprToText(mem.other),
        detail: "다음 중 어느 것도 아님",
        memberset: {
          mode: "any",
          members: mem.set.elements.map((m) => ({ text: exprToText(m), path: pathOf.get(m) ?? "?" })),
        },
        children: [],
      };
    }
    const lp = leafParts(inner);
    return {
      path: innerPath,
      kind: "leaf",
      // No structured detail (e.g. an empty-set placeholder) → mark the
      // negation in the title so the box never reads as the positive form.
      title: lp.detail ? lp.title : `${lp.title} — 아님`,
      ...(lp.detail ? { detail: lp.detail.replace(/^포함/, "포함 안 함") } : {}),
      children: [],
    };
  }
  if (e.kind === "unary" && e.op === "!") {
    return { path, kind: "not", title: "아니다", children: [exprToNode(e.operand, pathOf)] };
  }
  if (e.kind === "if") {
    const branch = (b: Expr, tag: string): DNode => ({
      ...exprToNode(b, pathOf),
      detail: tag,
    });
    return {
      path,
      kind: "if",
      title: "IF",
      children: [branch(e.cond, "조건"), branch(e.then, "then"), branch(e.else, "else")],
    };
  }
  // `[..] contains x` / `x in [..]` / `set containsAny [..]` over a LITERAL set →
  // fan out one leaf per member, so the user sees WHICH entry is at play (and the
  // diagnosis can red-trace the single matched one). Members keep their canonical
  // `…elements[i]` path (via pathOf), so highlight lines up. A scalar membership
  // (no literal set) falls through to a single leaf.
  if (e.kind === "binary") {
    const mem = setLiteralOperand(e);
    if (mem) {
      const fieldPath = attrPath(mem.other);
      return {
        path,
        kind: "memberset",
        title: (fieldPath && getGloss(fieldPath)?.ko) || exprToText(mem.other),
        detail: isAllOf(e.op) ? "다음 전부 포함" : "다음 중 하나",
        memberset: {
          mode: isAllOf(e.op) ? "all" : "any",
          members: mem.set.elements.map((m) => ({ text: exprToText(m), path: pathOf.get(m) ?? "?" })),
        },
        children: [],
      };
    }
  }
  return { path, kind: "leaf", ...leafParts(e), children: [] };
}

function buildTree(ir: PolicyIR): DNode {
  const pathOf = pathByNode(ir);
  const clauses: DNode[] = ir.conditions.map((c, i) => ({
    // Display wrapper for the clause; its body carries the canonical `c{i}.body`.
    path: `clause${i}`,
    kind: c.kind === "unless" ? "unless" : "when",
    title: c.kind === "unless" ? "예외" : "발동 조건",
    children: [exprToNode(c.body, pathOf)],
  }));
  // The FORBID/PERMIT head is dropped — the action is shown in the form's
  // trigger, and the effect is implied. A single clause becomes the root; with
  // both when + unless we keep a light neutral junction (no FORBID box).
  if (clauses.length === 0) {
    return { path: "root", kind: "when", title: "조건 없음 · 항상 적용", children: [] };
  }
  if (clauses.length === 1) {
    const clause = clauses[0];
    // A single WHEN over one leaf / one memberset reads fine on its own — drop
    // the wrapper box. Keep it for gates and for UNLESS (예외).
    if (
      clause.kind === "when" &&
      clause.children.length === 1 &&
      (clause.children[0].kind === "leaf" || clause.children[0].kind === "memberset")
    ) {
      return clause.children[0];
    }
    // Sentence flow: a lone WHEN over a gate reads as the gate itself, with the
    // top gate phrased as the sentence opener ("아래 중 하나라도 해당하면").
    if (clause.kind === "when" && clause.children.length === 1) {
      return sentenceGate(clause.children[0]);
    }
    return clause;
  }
  return { path: "root", kind: "root", title: "규칙", children: clauses };
}

/** Rephrase the TOP gate as a sentence opener (lower gates keep the short
 *  모두/하나라도 라벨). Non-gates pass through. */
function sentenceGate(n: DNode): DNode {
  if (n.kind === "or") return { ...n, title: "아래 중 하나라도 해당하면" };
  if (n.kind === "and") return { ...n, title: "아래에 모두 해당하면" };
  return n;
}

/**
 * Test/debug helper: every canonical node path the diagram assigns to an Expr
 * node (excludes the synthetic root/WHEN/UNLESS wrappers). Asserting these are a
 * subset of the diagnosis module's `enumeratePaths` proves the diagram can never
 * drift from the paths a diagnosis blames.
 */
export function policyDiagramPaths(ir: PolicyIR): string[] {
  const out: string[] = [];
  const walk = (n: DNode) => {
    if (n.kind !== "root" && n.kind !== "when" && n.kind !== "unless") out.push(n.path);
    // memberset chips carry their own canonical paths (rendered as chips, not
    // child boxes) — keep them in the path set so diagnosis alignment holds.
    if (n.memberset) for (const m of n.memberset.members) out.push(m.path);
    n.children.forEach(walk);
  };
  walk(buildTree(ir));
  return out;
}

/** Boolean comparison extension fns → their operator glyph, so a leaf reads
 *  `inputUsd ≥ 0.05` instead of `greaterThanOrEqual(inputUsd, decimal("0.05"))`. */
const EXT_OP: Record<string, string> = {
  greaterThan: ">",
  greaterThanOrEqual: "≥",
  lessThan: "<",
  lessThanOrEqual: "≤",
};

/** Comparison operator → its negation, so `!(a < b)` folds to `a ≥ b`. */
const NEGATE_BINARY: Record<string, string> = {
  "<": "≥",
  "<=": ">",
  ">": "≤",
  ">=": "<",
  "==": "≠",
  "!=": "==",
};

/** Comparison operator → its negation as an operator (for Korean phrasing). */
const NEGATE_OP: Record<string, BinaryOp> = {
  "<": ">=",
  "<=": ">",
  ">": "<=",
  ">=": "<",
  "==": "!=",
  "!=": "==",
};

/** Boolean comparison extension fns → the operator they mean. */
const EXT_TO_OP: Record<string, string> = {
  greaterThan: ">",
  greaterThanOrEqual: ">=",
  lessThan: "<",
  lessThanOrEqual: "<=",
};

/** Human value text for a comparison's right-hand side. */
function valueExprText(rhs: Expr): string {
  const lit = unwrapExtLiteral(rhs);
  if (lit !== null) return lit;
  if (rhs.kind === "lit") {
    if (rhs.litType === "bool") return rhs.value ? "참" : "거짓";
    if (rhs.litType === "string") return String(rhs.value) === "" ? "" : `"${rhs.value}"`;
    return String(rhs.value);
  }
  if (rhs.kind === "set") {
    return `[${rhs.elements.map((el) => unwrapExtLiteral(el) ?? exprToText(el)).join(", ")}]`;
  }
  const p = attrPath(rhs);
  if (p) return getGloss(p)?.ko ?? p;
  return exprToText(rhs);
}

/** A leaf comparison as plain Korean (field label + op phrase + value), or null
 *  when `e` isn't a humanizable comparison (caller falls back to `exprToText`). */
function exprToKorean(e: Expr): string | null {
  if (e.kind === "binary") {
    const COMPARE = ["==", "!=", "<", "<=", ">", ">=", "contains", "in"];
    if (!COMPARE.includes(e.op)) return null;
    const path = attrPath(e.left);
    if (!path) return null;
    const emptyStr = e.right.kind === "lit" && e.right.litType === "string" && e.right.value === "";
    return naturalCondition({ subject: getGloss(path)?.ko ?? path, op: e.op, value: valueExprText(e.right), emptyStr });
  }
  if (e.kind === "ext" && EXT_TO_OP[e.fn] && e.args.length === 2) {
    const path = attrPath(e.args[0]);
    if (!path) return null;
    return naturalCondition({ subject: getGloss(path)?.ko ?? path, op: EXT_TO_OP[e.fn], value: valueExprText(e.args[1]) });
  }
  return null;
}

/** Operator → compact symbol for the leaf's value line. */
const OP_SYM: Record<string, string> = {
  "==": "=",
  "!=": "≠",
  "<": "<",
  "<=": "≤",
  ">": ">",
  ">=": "≥",
  contains: "포함",
  in: "중 하나",
};

/** Split a leaf comparison into a two-line card: `title` = field label,
 *  `detail` = operator + value (+ unit). Falls back to a single `title` line
 *  for anything that isn't a plain field-vs-value comparison. */
function leafParts(e: Expr): { title: string; detail?: string } {
  const fromCompare = (
    path: string | null,
    op: string,
    rhs: Expr,
  ): { title: string; detail?: string } | null => {
    if (!path) return null;
    const g = getGloss(path);
    const unit = g?.unit?.ko ? ` ${g.unit.ko}` : "";
    const emptyStr = rhs.kind === "lit" && rhs.litType === "string" && rhs.value === "";
    if (emptyStr) {
      return { title: g?.ko ?? path, detail: op === "==" ? "비어 있음" : "비어 있지 않음" };
    }
    return { title: g?.ko ?? path, detail: `${OP_SYM[op] ?? op} ${valueExprText(rhs)}${unit}` };
  };

  if (e.kind === "binary" && OP_SYM[e.op]) {
    const parts = fromCompare(attrPath(e.left), e.op, e.right);
    if (parts) return parts;
  }
  if (e.kind === "ext" && EXT_TO_OP[e.fn] && e.args.length === 2) {
    const parts = fromCompare(attrPath(e.args[0]), EXT_TO_OP[e.fn], e.args[1]);
    if (parts) return parts;
  }
  return { title: exprToKorean(e) ?? exprToText(e) };
}

/** `decimal("0.05")` / `ip("…")` used as a value → just its inner literal text. */
function unwrapExtLiteral(e: Expr): string | null {
  if (e.kind === "ext" && (e.fn === "decimal" || e.fn === "ip") && e.args.length === 1) {
    const a = e.args[0];
    if (a.kind === "lit") return String(a.value);
  }
  return null;
}

/** Compact Cedar-ish text for a leaf expression. Truncated by the renderer. */
export function exprToText(e: Expr): string {
  switch (e.kind) {
    case "var":
      return e.name;
    case "lit":
      return e.litType === "string" ? `"${e.value}"` : String(e.value);
    case "litEntity":
      return `${e.entity.type}::"${e.entity.id}"`;
    case "attr":
      return `${exprToText(e.of)}.${e.attr}`;
    case "has":
      return `${exprToText(e.of)} has ${e.attr}`;
    case "binary":
      return `${exprToText(e.left)} ${e.op} ${exprToText(e.right)}`;
    case "unary":
      return e.op === "!"
        ? `!${e.operand.kind === "binary" ? `(${exprToText(e.operand)})` : exprToText(e.operand)}`
        : e.op === "neg"
          ? `-${exprToText(e.operand)}`
          : `${exprToText(e.operand)}.isEmpty()`;
    case "like":
      return `${exprToText(e.of)} like "${e.pattern
        .map((p) => (p === "Wildcard" ? "*" : p.Literal))
        .join("")}"`;
    case "is":
      return `${exprToText(e.of)} is ${e.entityType}${
        e.in ? ` in ${exprToText(e.in)}` : ""
      }`;
    case "ext": {
      // A bare `decimal(...)`/`ip(...)` value → its inner literal.
      const lit = unwrapExtLiteral(e);
      if (lit !== null) return lit;
      // A comparison method → operator form: `a.greaterThanOrEqual(b)` → `a ≥ b`.
      const op = EXT_OP[e.fn];
      if (op && e.args.length === 2) {
        return `${exprToText(e.args[0])} ${op} ${exprToText(e.args[1])}`;
      }
      // Otherwise method-style: `receiver.fn(rest…)` (e.g. `ip.isInRange(r)`).
      if (e.args.length >= 1) {
        const [recv, ...rest] = e.args;
        return `${exprToText(recv)}.${e.fn}(${rest.map(exprToText).join(", ")})`;
      }
      return `${e.fn}()`;
    }
    case "set":
      return `[${e.elements.map(exprToText).join(", ")}]`;
    case "record":
      return `{${e.pairs.map((p) => `${p.key}: ${exprToText(p.value)}`).join(", ")}}`;
    case "hole":
      return `?${e.name}`;
    case "raw":
      return "‹raw›";
    default:
      return "?";
  }
}

// ── Layout (tidy top-down tree) ──────────────────────────────────────────

const NODE_H = 40;
const V_GAP = 28;
const H_GAP = 18;
const CHAR_W = 7.2;
const PAD_X = 24;
const MIN_W = 64;
const MAX_W = 380;
const LABEL_CAP = 48;

// memberset (chip box) sizing
const MS_HEADER = 38;
const MS_CHIP_H = 22;
const MS_CHIP_GAP = 6;
const MS_PAD = 12;
/** Box dimensions for a memberset — width grows with member count (capped),
 *  height is computed conservatively (2 chips/row) so chips never clip. */
function membersetSize(n: DNode): { w: number; h: number } {
  const count = n.memberset?.members.length ?? 1;
  const w = count <= 2 ? 200 : 300;
  const rows = Math.max(1, Math.ceil(count / 2));
  return { w, h: MS_HEADER + rows * (MS_CHIP_H + MS_CHIP_GAP) + MS_PAD };
}

interface Placed extends DNode {
  x: number; // center x
  y: number; // top y
  h: number; // box height (NODE_H, or taller for memberset)
  w: number; // box width
  children: Placed[];
}

const STACK_GAP = 9;
/** Stack a gate's children vertically (instead of one wide row) when it has
 *  many leaf-only children — e.g. an allowlist fan-out. Keeps each box full
 *  size & readable, and the diagram narrow enough to fit the pane. */
function isStacked(n: DNode): boolean {
  return (
    n.children.length > 3 &&
    n.children.every((c) => c.kind === "leaf" && c.children.length === 0)
  );
}

/** Label width in latin-char units — CJK glyphs run ~1.8× a latin char at the
 *  diagram's 12px font, so width math counts them accordingly. */
function textUnits(s: string): number {
  let u = 0;
  for (const ch of s) u += /[ᄀ-ᇿ　-〿一-鿿가-힯＀-￯]/.test(ch) ? 1.8 : 1;
  return u;
}

function nodeWidth(n: DNode): number {
  if (n.kind === "memberset") return membersetSize(n).w;
  const text = Math.max(textUnits(n.title), textUnits(n.detail ?? ""));
  return Math.min(MAX_W, Math.max(MIN_W, Math.min(text, LABEL_CAP) * CHAR_W + PAD_X));
}
/** Box height of a node (taller for a memberset chip box). */
function nodeHeight(n: DNode): number {
  return n.kind === "memberset" ? membersetSize(n).h : NODE_H;
}

/** Two-pass layout: size subtrees (width + height), then place. Returns the
 *  placed root and the total canvas extents. */
function layout(root: DNode): { placed: Placed; width: number; height: number } {
  const subtreeW = new Map<DNode, number>();
  const subtreeH = new Map<DNode, number>();
  const measure = (n: DNode): void => {
    const own = nodeWidth(n);
    if (n.children.length === 0) {
      subtreeW.set(n, own);
      subtreeH.set(n, nodeHeight(n));
      return;
    }
    n.children.forEach(measure);
    const childW = n.children.map((c) => subtreeW.get(c)!);
    const childH = n.children.map((c) => subtreeH.get(c)!);
    if (isStacked(n)) {
      // children stacked in a column: width = widest, height = sum.
      subtreeW.set(n, Math.max(own, Math.max(...childW)));
      const colH = childH.reduce((a, b) => a + b, 0) + STACK_GAP * (n.children.length - 1);
      subtreeH.set(n, NODE_H + V_GAP + colH);
    } else {
      const span = childW.reduce((a, b) => a + b, 0) + H_GAP * (n.children.length - 1);
      subtreeW.set(n, Math.max(own, span));
      subtreeH.set(n, NODE_H + V_GAP + Math.max(...childH));
    }
  };
  measure(root);

  // Second pass — assign positions. `left` = subtree's left edge, `top` = its y.
  let maxBottom = 0;
  const place = (n: DNode, left: number, top: number): Placed => {
    const w = nodeWidth(n);
    const h = nodeHeight(n);
    const subW = subtreeW.get(n) ?? w;
    maxBottom = Math.max(maxBottom, top + h);
    if (n.children.length === 0) {
      return { ...n, x: left + subW / 2, y: top, w, h, children: [] };
    }
    const childTop = top + h + V_GAP;
    if (isStacked(n)) {
      const cx = left + subW / 2;
      let cy = childTop;
      const placedKids = n.children.map((c) => {
        const cSub = subtreeW.get(c)!;
        const pk = place(c, cx - cSub / 2, cy);
        cy += subtreeH.get(c)! + STACK_GAP;
        return pk;
      });
      return { ...n, x: cx, y: top, w, h, children: placedKids };
    }
    let cursor = left;
    const placedKids = n.children.map((c) => {
      const cSub = subtreeW.get(c) ?? nodeWidth(c);
      const pk = place(c, cursor, childTop);
      cursor += cSub + H_GAP;
      return pk;
    });
    const first = placedKids[0].x;
    const last = placedKids[placedKids.length - 1].x;
    return { ...n, x: (first + last) / 2, y: top, w, h, children: placedKids };
  };
  const placed = place(root, 0, 0);
  return { placed, width: subtreeW.get(root) ?? nodeWidth(root), height: maxBottom };
}

/** Strip the surrounding quotes a string literal carries from `exprToText`. */
const stripQuotes = (s: string): string => s.replace(/^"|"$/g, "");
/** Shorten any leftover full 0x address to `0x1234…abcd` (chips stay compact). */
const shortenAddrs = (t: string): string =>
  t.replace(/0x[0-9a-fA-F]{40}/g, (m) => `${m.slice(0, 6)}…${m.slice(-4)}`);

// ── Render ───────────────────────────────────────────────────────────────

export interface PolicyDiagramProps {
  ir: PolicyIR | null;
  /** Canonical node paths of the responsible leaves (diagnosis `culprits`) —
   *  red-traced. Empty = structure only. */
  highlightPaths?: readonly string[];
  /** Canonical paths whose probe errored (diagnosis `errored`) — shown as a
   *  distinct "uneval" state rather than a confident red box. */
  erroredPaths?: readonly string[];
  /** Tighter sizing for cramped surfaces (e.g. the confirm popup). */
  compact?: boolean;
  /** Pan/zoom canvas (wheel zoom at cursor, drag pan, dblclick/버튼 fit) — the
   *  editor surface only; other surfaces stay static. */
  interactive?: boolean;
  /** Canonical paths to render as the user's SELECTION (editor click-sync) —
   *  blue outline, distinct from the diagnosis red. */
  selectedPaths?: readonly string[];
  /** Node click callback (editor click-sync). Wrapper nodes (root/when/unless)
   *  don't fire. */
  onNodeClick?: (path: string) => void;
  /** Optional resolver: rewrite 0x addresses in node labels to friendly names.
   *  The caller owns the address book (a hook), keeping this module pure. */
  humanizeLabel?: (text: string) => string;
}

/** Zoom clamps for the interactive canvas. */
const PZ_MIN = 0.25;
const PZ_MAX = 3;

export function PolicyDiagram({
  ir,
  highlightPaths,
  erroredPaths,
  compact,
  interactive,
  selectedPaths,
  onNodeClick,
  humanizeLabel,
}: PolicyDiagramProps) {
  const model = useMemo(() => (ir ? layout(buildTree(ir)) : null), [ir]);

  const PAD = compact ? 8 : 16;
  const W = (model?.width ?? 0) + PAD * 2;
  const H = (model?.height ?? 0) + PAD * 2;

  // ── pan/zoom (interactive mode only) ──────────────────────────────────
  const wrapRef = useRef<HTMLDivElement>(null);
  const [view, setView] = useState<{ k: number; x: number; y: number } | null>(null);
  // pan 후의 클릭(드래그 잔향)이 노드 선택으로 새지 않게 한 번 삼킨다.
  const suppressClick = useRef(false);
  const dragFrom = useRef<{ x: number; y: number; moved: boolean } | null>(null);

  const fit = useCallback(() => {
    const el = wrapRef.current;
    if (!el || !W || !H) return;
    const k = Math.min(el.clientWidth / W, el.clientHeight / H, 1);
    setView({ k, x: (el.clientWidth - W * k) / 2, y: Math.max((el.clientHeight - H * k) / 2, 8) });
  }, [W, H]);
  useLayoutEffect(() => {
    if (interactive) fit();
  }, [interactive, fit]);

  // wheel은 React 합성 이벤트가 passive라 preventDefault가 안 먹는다 — native로.
  useEffect(() => {
    const el = wrapRef.current;
    if (!el || !interactive) return;
    const onWheel = (ev: WheelEvent) => {
      ev.preventDefault();
      setView((v) => {
        if (!v) return v;
        const r = el.getBoundingClientRect();
        const px = ev.clientX - r.left;
        const py = ev.clientY - r.top;
        const k = Math.min(PZ_MAX, Math.max(PZ_MIN, v.k * Math.exp(-ev.deltaY * 0.0015)));
        return { k, x: px - ((px - v.x) * k) / v.k, y: py - ((py - v.y) * k) / v.k };
      });
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [interactive]);

  const zoomBy = (f: number) =>
    setView((v) => {
      const el = wrapRef.current;
      if (!v || !el) return v;
      const px = el.clientWidth / 2;
      const py = el.clientHeight / 2;
      const k = Math.min(PZ_MAX, Math.max(PZ_MIN, v.k * f));
      return { k, x: px - ((px - v.x) * k) / v.k, y: py - ((py - v.y) * k) / v.k };
    });

  const onPointerDown = (ev: React.PointerEvent<HTMLDivElement>) => {
    if (ev.button !== 0) return;
    // 캡처는 여기서 걸지 않는다 — pointerdown 즉시 캡처하면 자식(노드/버튼)의
    // click이 래퍼로 리타게팅되어 노드 선택·컨트롤 버튼이 죽는다.
    dragFrom.current = { x: ev.clientX, y: ev.clientY, moved: false };
  };
  const onPointerMove = (ev: React.PointerEvent<HTMLDivElement>) => {
    const d = dragFrom.current;
    if (!d) return;
    const dx = ev.clientX - d.x;
    const dy = ev.clientY - d.y;
    if (!d.moved) {
      if (Math.hypot(dx, dy) < 4) return; // 클릭/드래그 구분 임계값
      d.moved = true;
      ev.currentTarget.setPointerCapture(ev.pointerId); // 진짜 드래그일 때만 캡처
    }
    d.x = ev.clientX;
    d.y = ev.clientY;
    setView((v) => (v ? { ...v, x: v.x + dx, y: v.y + dy } : v));
  };
  const onPointerUp = () => {
    if (dragFrom.current?.moved) suppressClick.current = true;
    dragFrom.current = null;
  };

  // Friendly-name resolver for 0x addresses inside labels (supplied by the
  // caller, which owns the address book). Identity when not provided.
  const humanizeAddrs = humanizeLabel ?? ((s: string) => s);

  if (!ir || !model) {
    return <div className="pdiagram-empty">표시할 정책이 없습니다</div>;
  }

  const hl = new Set(highlightPaths ?? []);
  const err = new Set(erroredPaths ?? []);
  const sel = new Set(selectedPaths ?? []);
  const active = hl.size > 0 || err.size > 0;
  // A node is on the trace if it (or any descendant) is a culprit/errored, so we
  // can dim the branches that did NOT contribute to the block.
  const onTrace = new Set<string>();
  if (active) {
    const mark = (n: Placed): boolean => {
      const childHit = n.children.map(mark).some(Boolean);
      const memberHit = n.memberset?.members.some((m) => hl.has(m.path) || err.has(m.path)) ?? false;
      const hit = hl.has(n.path) || err.has(n.path) || memberHit || childHit;
      if (hit) onTrace.add(n.path);
      return hit;
    };
    mark(model.placed);
  }

  const edges: JSX.Element[] = [];
  const nodes: JSX.Element[] = [];

  const walk = (n: Placed) => {
    const cx = n.x + PAD;
    const cy = n.y + PAD;
    if (isStacked(n)) {
      // Vertical chain: parent → first child, then each child to the next. The
      // boxes are column-aligned, so adjacent-only segments never cross a box.
      const chain: Placed[] = [n, ...n.children];
      for (let i = 1; i < chain.length; i++) {
        const a = chain[i - 1];
        const b = chain[i];
        const dimmed = active && !onTrace.has(b.path);
        edges.push(
          <path
            key={`e-${a.path}-${b.path}`}
            className={`pd-edge${dimmed ? " pd-dim" : ""}`}
            d={`M ${a.x + PAD} ${a.y + PAD + a.h} L ${b.x + PAD} ${b.y + PAD}`}
          />,
        );
      }
      n.children.forEach(walk);
    } else {
      for (const c of n.children) {
        const dimmed = active && !onTrace.has(c.path);
        edges.push(
          <path
            key={`e-${n.path}-${c.path}`}
            className={`pd-edge${dimmed ? " pd-dim" : ""}`}
            d={`M ${cx} ${cy + n.h} C ${cx} ${cy + n.h + V_GAP / 2}, ${
              c.x + PAD
            } ${c.y + PAD - V_GAP / 2}, ${c.x + PAD} ${c.y + PAD}`}
          />,
        );
        walk(c);
      }
    }
    const msHit = n.memberset?.members.some((m) => hl.has(m.path)) ?? false;
    const culprit = hl.has(n.path) || msHit;
    const errored = !culprit && err.has(n.path);
    const dimmed = active && !onTrace.has(n.path);
    const title = humanizeAddrs(n.title);
    const detail = n.detail ? humanizeAddrs(n.detail) : undefined;
    const label = title.length > LABEL_CAP ? `${title.slice(0, LABEL_CAP - 1)}…` : title;
    const isWrapper = n.kind === "root" || n.kind === "when" || n.kind === "unless";
    const selected =
      sel.has(n.path) || (n.memberset?.members.some((m) => sel.has(m.path)) ?? false);
    const clickable = !!onNodeClick && !isWrapper;
    nodes.push(
      <g
        key={`n-${n.path}`}
        className={`pd-node pd-${n.kind}${culprit ? " pd-culprit" : ""}${
          errored ? " pd-errored" : ""
        }${dimmed ? " pd-dim" : ""}${selected ? " pd-selected" : ""}${
          clickable ? " pd-clickable" : ""
        }`}
        transform={`translate(${cx - n.w / 2}, ${cy})`}
        onClick={
          clickable
            ? (ev) => {
                ev.stopPropagation();
                if (suppressClick.current) {
                  suppressClick.current = false;
                  return;
                }
                onNodeClick(n.path);
              }
            : undefined
        }
      >
        {n.memberset ? (
          <foreignObject width={n.w} height={n.h}>
            <div className="pd-ms">
              <div className="pd-ms-head">
                <span className="pd-ms-field">{label}</span>
                {detail && <span className="pd-ms-mode">{detail}</span>}
              </div>
              <div className="pd-ms-chips">
                {n.memberset.members.map((m, i) => {
                  const chipHit = hl.has(m.path);
                  const chipErr = !chipHit && err.has(m.path);
                  return (
                    <span
                      key={i}
                      className={`pd-ms-chip${chipHit ? " hit" : ""}${chipErr ? " err" : ""}`}
                    >
                      {shortenAddrs(humanizeAddrs(stripQuotes(m.text)))}
                    </span>
                  );
                })}
              </div>
            </div>
          </foreignObject>
        ) : (
          <>
            <rect className="pd-box" width={n.w} height={n.h} rx={n.kind === "leaf" ? 7 : 10} />
            <text className="pd-title" x={n.w / 2} y={detail ? 17 : 24}>
              {label}
            </text>
            {detail && (
              <text className="pd-detail" x={n.w / 2} y={30}>
                {detail.length > LABEL_CAP ? `${detail.slice(0, LABEL_CAP - 1)}…` : detail}
              </text>
            )}
            <title>{detail ? `${title} · ${detail}` : title}</title>
          </>
        )}
      </g>,
    );
  };
  walk(model.placed);

  if (interactive) {
    return (
      <div
        className="pdiagram is-interactive"
        ref={wrapRef}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        onDoubleClick={fit}
      >
        <svg
          className="pdiagram-svg"
          width="100%"
          height="100%"
          role="img"
          aria-label="정책 구조 다이어그램"
        >
          <g transform={view ? `translate(${view.x} ${view.y}) scale(${view.k})` : undefined}>
            {edges}
            {nodes}
          </g>
        </svg>
        <div className="pd-controls">
          <button type="button" onClick={() => zoomBy(1.25)} aria-label="확대">
            ＋
          </button>
          <button type="button" onClick={() => zoomBy(1 / 1.25)} aria-label="축소">
            −
          </button>
          <button type="button" onClick={fit}>
            맞춤
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className={`pdiagram${compact ? " is-compact" : ""}`}>
      <svg
        className="pdiagram-svg"
        viewBox={`0 0 ${W} ${H}`}
        style={{ width: "100%", maxWidth: W, height: "auto" }}
        role="img"
        aria-label="정책 구조 다이어그램"
      >
        {edges}
        {nodes}
      </svg>
    </div>
  );
}
