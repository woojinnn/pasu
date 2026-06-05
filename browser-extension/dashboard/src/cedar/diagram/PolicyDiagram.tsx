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
import { useMemo } from "react";

import type { ActionScope, Expr, PolicyIR } from "../blocks/ir";
import { pathByNode } from "../diagnosis/path";

import "./policy-diagram.css";

type NodeKind = "root" | "when" | "unless" | "and" | "or" | "not" | "if" | "leaf";

interface DNode {
  /** Structural path, stable across renders; the unit `highlightPaths` targets. */
  path: string;
  kind: NodeKind;
  /** Primary label (e.g. "AND", or a leaf's condition text). */
  title: string;
  /** Optional secondary line (e.g. the action under a FORBID head, or an
   *  IF branch tag like "then"). */
  detail?: string;
  children: DNode[];
}

// ── IR → tree ────────────────────────────────────────────────────────────

function actionLabel(a: ActionScope): string {
  switch (a.kind) {
    case "scopeAll":
      return "any action";
    case "scopeEq":
      return a.entity.id;
    case "scopeIn":
      return a.entities.map((e) => e.id).join(" / ") || "any action";
  }
}

/** Flatten a same-operator binary chain into its leaf operands, preserving each
 *  operand's identity so its canonical path resolves via {@link pathByNode}:
 *  `A && B && C` → `[A, B, C]`. */
function flatten(e: Expr, op: "&&" | "||"): Expr[] {
  if (e.kind === "binary" && e.op === op) {
    return [...flatten(e.left, op), ...flatten(e.right, op)];
  }
  return [e];
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
    return {
      path,
      kind: e.op === "&&" ? "and" : "or",
      title: e.op === "&&" ? "AND" : "OR",
      children: flatten(e, e.op).map((c) => exprToNode(c, pathOf)),
    };
  }
  if (e.kind === "unary" && e.op === "!") {
    return { path, kind: "not", title: "NOT", children: [exprToNode(e.operand, pathOf)] };
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
  return { path, kind: "leaf", title: exprToText(e), children: [] };
}

function buildTree(ir: PolicyIR): DNode {
  const pathOf = pathByNode(ir);
  return {
    path: "root",
    kind: "root",
    title: ir.effect === "forbid" ? "FORBID" : "PERMIT",
    detail: actionLabel(ir.scope.action),
    children: ir.conditions.map((c, i) => ({
      // Display wrapper for the clause; its body carries the canonical `c{i}.body`.
      path: `clause${i}`,
      kind: c.kind === "unless" ? "unless" : "when",
      title: c.kind === "unless" ? "UNLESS" : "WHEN",
      children: [exprToNode(c.body, pathOf)],
    })),
  };
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
    n.children.forEach(walk);
  };
  walk(buildTree(ir));
  return out;
}

/** Compact Cedar-ish text for a leaf expression. Truncated by the renderer. */
function exprToText(e: Expr): string {
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
        ? `!${exprToText(e.operand)}`
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
    case "ext":
      return `${e.fn}(${e.args.map(exprToText).join(", ")})`;
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
const PAD_X = 22;
const MIN_W = 64;
const MAX_W = 240;
const LABEL_CAP = 30;

interface Placed extends DNode {
  x: number; // center x
  y: number; // top y
  w: number; // box width
  children: Placed[];
}

function nodeWidth(n: DNode): number {
  const text = Math.max(n.title.length, (n.detail ?? "").length);
  return Math.min(MAX_W, Math.max(MIN_W, Math.min(text, LABEL_CAP) * CHAR_W + PAD_X));
}

/** Two-pass layout: size subtrees, then place. Returns the placed root and the
 *  total canvas extents. */
function layout(root: DNode): { placed: Placed; width: number; height: number } {
  // First pass — subtree width (max of own box and children span).
  const subtreeW = new Map<DNode, number>();
  const measure = (n: DNode): number => {
    const own = nodeWidth(n);
    if (n.children.length === 0) {
      subtreeW.set(n, own);
      return own;
    }
    const kids = n.children.map(measure);
    const span = kids.reduce((a, b) => a + b, 0) + H_GAP * (n.children.length - 1);
    const w = Math.max(own, span);
    subtreeW.set(n, w);
    return w;
  };
  measure(root);

  // Second pass — assign positions. `left` is the subtree's left edge.
  let maxBottom = 0;
  const place = (n: DNode, left: number, depth: number): Placed => {
    const w = nodeWidth(n);
    const subW = subtreeW.get(n) ?? w;
    const y = depth * (NODE_H + V_GAP);
    maxBottom = Math.max(maxBottom, y + NODE_H);
    if (n.children.length === 0) {
      return { ...n, x: left + subW / 2, y, w, children: [] };
    }
    let cursor = left;
    const placedKids = n.children.map((c) => {
      const cSub = subtreeW.get(c) ?? nodeWidth(c);
      const pk = place(c, cursor, depth + 1);
      cursor += cSub + H_GAP;
      return pk;
    });
    const first = placedKids[0].x;
    const last = placedKids[placedKids.length - 1].x;
    return { ...n, x: (first + last) / 2, y, w, children: placedKids };
  };
  const placed = place(root, 0, 0);
  return { placed, width: subtreeW.get(root) ?? nodeWidth(root), height: maxBottom };
}

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
}

export function PolicyDiagram({
  ir,
  highlightPaths,
  erroredPaths,
  compact,
}: PolicyDiagramProps) {
  const model = useMemo(() => (ir ? layout(buildTree(ir)) : null), [ir]);

  if (!ir || !model) {
    return <div className="pdiagram-empty">표시할 정책이 없습니다</div>;
  }

  const hl = new Set(highlightPaths ?? []);
  const err = new Set(erroredPaths ?? []);
  const active = hl.size > 0 || err.size > 0;
  // A node is on the trace if it (or any descendant) is a culprit/errored, so we
  // can dim the branches that did NOT contribute to the block.
  const onTrace = new Set<string>();
  if (active) {
    const mark = (n: Placed): boolean => {
      const childHit = n.children.map(mark).some(Boolean);
      const hit = hl.has(n.path) || err.has(n.path) || childHit;
      if (hit) onTrace.add(n.path);
      return hit;
    };
    mark(model.placed);
  }

  const PAD = compact ? 8 : 16;
  const W = model.width + PAD * 2;
  const H = model.height + PAD * 2;

  const edges: JSX.Element[] = [];
  const nodes: JSX.Element[] = [];

  const walk = (n: Placed) => {
    const cx = n.x + PAD;
    const cy = n.y + PAD;
    for (const c of n.children) {
      const dimmed = active && !onTrace.has(c.path);
      edges.push(
        <path
          key={`e-${n.path}-${c.path}`}
          className={`pd-edge${dimmed ? " pd-dim" : ""}`}
          d={`M ${cx} ${cy + NODE_H} C ${cx} ${cy + NODE_H + V_GAP / 2}, ${
            c.x + PAD
          } ${c.y + PAD - V_GAP / 2}, ${c.x + PAD} ${c.y + PAD}`}
        />,
      );
      walk(c);
    }
    const culprit = hl.has(n.path);
    const errored = !culprit && err.has(n.path);
    const dimmed = active && !onTrace.has(n.path);
    const label =
      n.title.length > LABEL_CAP ? `${n.title.slice(0, LABEL_CAP - 1)}…` : n.title;
    nodes.push(
      <g
        key={`n-${n.path}`}
        className={`pd-node pd-${n.kind}${culprit ? " pd-culprit" : ""}${
          errored ? " pd-errored" : ""
        }${dimmed ? " pd-dim" : ""}`}
        transform={`translate(${cx - n.w / 2}, ${cy})`}
      >
        <rect className="pd-box" width={n.w} height={NODE_H} rx={n.kind === "leaf" ? 7 : 10} />
        <text className="pd-title" x={n.w / 2} y={n.detail ? 17 : 24}>
          {label}
        </text>
        {n.detail && (
          <text className="pd-detail" x={n.w / 2} y={30}>
            {n.detail.length > LABEL_CAP ? `${n.detail.slice(0, LABEL_CAP - 1)}…` : n.detail}
          </text>
        )}
        <title>{n.detail ? `${n.title} · ${n.detail}` : n.title}</title>
      </g>,
    );
  };
  walk(model.placed);

  return (
    <div className={`pdiagram${compact ? " is-compact" : ""}`}>
      <svg
        className="pdiagram-svg"
        width={W}
        height={H}
        viewBox={`0 0 ${W} ${H}`}
        role="img"
        aria-label="정책 구조 다이어그램"
      >
        {edges}
        {nodes}
      </svg>
    </div>
  );
}
