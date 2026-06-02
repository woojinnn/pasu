/**
 * Doc constructors + mutation helpers.
 *
 * Two construction paths:
 *   - `initialDoc(action)` — blank slate: hat + empty AND root.
 *   - `goldenSwapDoc()`    — V7_SAMPLE_POLICY rebuilt as a tree, kept
 *      verbatim from `front/scopeball-v3/editor-v7-data.js v7BuildDoc()`
 *      so unit tests can confirm the serialize/eval roundtrip matches
 *      the design's golden output.
 *
 * Helpers are non-mutating where reasonable — the reducer rebuilds
 * the `nodes` map for each action. Local helpers (`addPredicate`,
 * `addLogic`) just produce a `Node`; the caller patches `nodes` and
 * `childIds` in one pass.
 */

import type { Doc, HatNode, LogicNode, Node, NodeId, PredicateNode } from "./types";
import {
  isLiveField,
  type FieldKind,
  type Op,
  type PredicateValue,
} from "./schema";

// ── id generator ───────────────────────────────────────────────────────

let _counter = 0;
export function newId(prefix: "p" | "L" | "hat" = "p"): NodeId {
  _counter += 1;
  return `${prefix}_${Date.now().toString(36)}${_counter}`;
}

// ── value normalizer ───────────────────────────────────────────────────
//
// Mirrors V7's `v7Val()` — accepts a JS value of any shape and returns
// the discriminated `PredicateValue`. Strings beginning with `@` are
// dynamic refs (e.g. `@meta.from`), numbers/booleans get their own
// kinds, anything else becomes a string literal.

export function toPredicateValue(v: unknown): PredicateValue {
  if (v && typeof v === "object" && "kind" in (v as Record<string, unknown>)) {
    return v as PredicateValue;
  }
  if (typeof v === "string") {
    if (v.startsWith("@")) return { kind: "ref", text: v };
    return { kind: "str", text: v };
  }
  if (typeof v === "number") return { kind: "num", text: String(v) };
  if (typeof v === "boolean") return { kind: "bool", text: v ? "true" : "false" };
  return { kind: "str", text: v == null ? "" : String(v) };
}

// ── factory helpers (return a Node, caller wires childIds + nodes map) ─

export interface PredicateConfig {
  fk: FieldKind;
  op: Op;
  value?: unknown;
  absence?: "treatAsFalse" | "treatAsTrue";
  parentId?: NodeId | null;
  guardId?: string;
  label?: string;
  userCopy?: { headline?: string; plain?: string };
  enabled?: boolean;
  float?: boolean;
  note?: string;
  x?: number;
  y?: number;
}

export function makePredicate(param: string, cfg: PredicateConfig): PredicateNode {
  const value = cfg.value !== undefined ? toPredicateValue(cfg.value) : null;
  const absence = cfg.absence ?? (isLiveField(param) ? "treatAsFalse" : undefined);
  return {
    id: newId("p"),
    type: "predicate",
    param,
    fieldKind: cfg.fk,
    op: cfg.op,
    value,
    absence,
    parentId: cfg.parentId ?? null,
    guardId: cfg.guardId,
    label: cfg.label,
    userCopy: cfg.userCopy,
    enabled: cfg.enabled,
    float: cfg.float,
    note: cfg.note,
    x: cfg.x ?? 0,
    y: cfg.y ?? 0,
  };
}

export interface LogicConfig {
  parentId?: NodeId | null;
  guardId?: string;
  label?: string;
  userCopy?: { headline?: string; plain?: string };
  enabled?: boolean;
  x?: number;
  y?: number;
}

export function makeLogic(op: "AND" | "OR" | "NOT", cfg: LogicConfig = {}): LogicNode {
  return {
    id: newId("L"),
    type: "logic",
    op,
    childIds: [],
    parentId: cfg.parentId ?? null,
    guardId: cfg.guardId,
    label: cfg.label,
    userCopy: cfg.userCopy,
    enabled: cfg.enabled,
    x: cfg.x ?? 0,
    y: cfg.y ?? 0,
  };
}

// ── canonical constructors ─────────────────────────────────────────────

/** Empty policy doc: `permit` + empty root AND. The user fills the
 *  body by dragging predicates from the palette. */
export function initialDoc(opts: {
  action: string;
  policyName?: string;
  effect?: "permit" | "deny";
}): Doc {
  const hat: HatNode = {
    id: "hat",
    type: "hat",
    effect: opts.effect ?? "permit",
    action: opts.action,
    childId: null,
    x: 80,
    y: 120,
  };
  const root = makeLogic("AND", { parentId: hat.id });
  hat.childId = root.id;
  return {
    nodes: { [hat.id]: hat, [root.id]: root },
    hatId: hat.id,
    rootId: root.id,
    drafts: [],
    locale: "ko",
    policyName: opts.policyName ?? "untitled",
    action: opts.action,
    pan: { x: 0, y: 0 },
    zoom: 1,
  };
}

/** Golden doc — V7_SAMPLE_POLICY ("Swap baseline · 안전조건 묶음").
 *  Rebuilt 1:1 from `v7BuildDoc()` so the test suite has a stable
 *  fixture for serializer / evaluator regression checks. */
export function goldenSwapDoc(): Doc {
  const nodes: Record<NodeId, Node> = {};
  const put = <N extends Node>(n: N): N => {
    nodes[n.id] = n;
    return n;
  };

  const hat: HatNode = {
    id: "hat",
    type: "hat",
    effect: "permit",
    action: "Amm::Swap",
    childId: null,
    x: 80,
    y: 120,
  };
  put(hat);

  const root = put(makeLogic("AND", { parentId: hat.id }));
  hat.childId = root.id;

  // s1 — NOT( AND( recipient ≠ from, recipientIsContract ) )
  const s1 = put(
    makeLogic("NOT", {
      parentId: root.id,
      guardId: "s1",
      label: "swap-and-send 배제",
      enabled: true,
      userCopy: {
        headline: "외부 컨트랙트로 빼돌리기 차단",
        plain: "수신자가 내 지갑이 아닌 컨트랙트면 막습니다",
      },
    }),
  );
  const s1and = put(makeLogic("AND", { parentId: s1.id }));
  const s1a = put(
    makePredicate("context.recipient", {
      fk: "primitive.String",
      op: "neq",
      value: "@meta.from",
      parentId: s1and.id,
    }),
  );
  const s1b = put(
    makePredicate("enrichment.recipientIsContract", {
      fk: "primitive.Bool",
      op: "isTrue",
      parentId: s1and.id,
    }),
  );
  s1and.childIds = [s1a.id, s1b.id];
  s1.childIds = [s1and.id];

  // s2 — slippageBp < 100
  const s2 = put(
    makePredicate("context.slippageBp", {
      fk: "primitive.Long",
      op: "lt",
      value: 100,
      parentId: root.id,
      guardId: "s2",
      label: "슬리피지 가드",
      enabled: true,
      userCopy: {
        headline: "슬리피지 상한",
        plain: "슬리피지가 100bp를 넘지 않아야 합니다",
      },
    }),
  );

  // s3 — NOT( AND( validityDeltaSec < 30, priceImpactBp > 50 ) )
  const s3 = put(
    makeLogic("NOT", {
      parentId: root.id,
      guardId: "s3",
      label: "만료임박+고임팩트 배제",
      enabled: true,
      userCopy: {
        headline: "만료 임박 + 프라이스 임팩트 과다 차단",
        plain: "마감 30초 안 남았는데 프라이스 임팩트가 50bp를 넘으면 막습니다",
      },
    }),
  );
  const s3and = put(makeLogic("AND", { parentId: s3.id }));
  const s3a = put(
    makePredicate("enrichment.validityDeltaSec", {
      fk: "primitive.Long",
      op: "lt",
      value: 30,
      absence: "treatAsFalse",
      parentId: s3and.id,
    }),
  );
  const s3b = put(
    makePredicate("context.priceImpactBp", {
      fk: "primitive.Long",
      op: "gt",
      value: 50,
      parentId: s3and.id,
    }),
  );
  s3and.childIds = [s3a.id, s3b.id];
  s3.childIds = [s3and.id];

  root.childIds = [s1.id, s2.id, s3.id];

  // Unconnected drafts — visible on the canvas, excluded from compile.
  const d1 = put(
    makePredicate("enrichment.effectiveRateVsOracleBps", {
      fk: "primitive.Long",
      op: "lt",
      value: 100,
      float: true,
      x: 120,
      y: 720,
      note: "오라클 슬리피지 안전조건 후보",
    }),
  );
  const d2 = put(
    makePredicate("enrichment.totalInputUsd", {
      fk: "primitive.decimal",
      op: "lt",
      value: 10000,
      float: true,
      x: 400,
      y: 720,
      note: "대형거래 한도 후보",
    }),
  );

  return {
    nodes,
    hatId: hat.id,
    rootId: root.id,
    drafts: [d1.id, d2.id],
    locale: "ko",
    policyName: "Swap baseline",
    action: "Amm::Swap",
    denyMessage: "swap baseline not satisfied",
    readingHeader: "이 Swap을 허용하려면 — 아래를 모두 만족해야 합니다",
    pan: { x: 0, y: 0 },
    zoom: 1,
  };
}

// ── walkers ────────────────────────────────────────────────────────────

/** All descendant node ids reachable from `start`, in DFS pre-order.
 *  Used by reducer DELETE actions to know the orphan set. */
export function descendants(doc: Doc, start: NodeId): NodeId[] {
  const out: NodeId[] = [];
  const stack: NodeId[] = [start];
  while (stack.length) {
    const id = stack.pop()!;
    const n = doc.nodes[id];
    if (!n) continue;
    out.push(id);
    if (n.type === "logic") stack.push(...n.childIds);
    else if (n.type === "hat" && n.childId) stack.push(n.childId);
  }
  return out;
}

/** True iff `n` is reachable from the hat (i.e. part of the compiled
 *  body, not floating in `drafts`). */
export function isConnected(doc: Doc, id: NodeId): boolean {
  const hat = doc.nodes[doc.hatId];
  if (!hat || hat.type !== "hat") return false;
  if (!hat.childId) return false;
  const seen = new Set(descendants(doc, hat.childId));
  return seen.has(id);
}

// Re-export the type alias for convenience.
export type { FieldKind, Op, PredicateValue };
