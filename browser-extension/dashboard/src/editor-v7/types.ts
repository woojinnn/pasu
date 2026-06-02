/**
 * Tree model for the v7 block editor.
 *
 * Three node kinds:
 *   - HatNode      — root of a policy: `permit | deny` × action.
 *   - LogicNode    — AND / OR / NOT. Children are referenced by id.
 *   - PredicateNode — single comparison: `param op value`.
 *
 * The tree is stored as a flat `Doc.nodes` map keyed by id, with edges
 * encoded by `parentId` and `childId(s)`. Two roots:
 *   - `hatId` always points at the HatNode.
 *   - `rootId` points at the top-level LogicNode under the hat (usually
 *     an AND that joins guards).
 *
 * `drafts` holds the ids of palette-dropped predicates the user hasn't
 * wired up yet — they sit on the canvas as floating chips and are
 * excluded from the compiled Cedar text.
 */

import type { FieldKind, Op, PredicateValue } from "./schema";

export type NodeId = string;

export interface HatNode {
  id: NodeId;
  type: "hat";
  effect: "permit" | "deny";
  /** Cedar action id, e.g. `Swap`, `Erc20Approve`. Must match an
   *  `action "X" appliesTo {}` entry in the extension's cedarschema. */
  action: string;
  /** The single LogicNode that holds the body (`when { ... }`). null = empty body. */
  childId: NodeId | null;
  x: number;
  y: number;
}

export interface LogicNode {
  id: NodeId;
  type: "logic";
  op: "AND" | "OR" | "NOT";
  childIds: NodeId[];
  /** Optional guard label rendered above the block. Useful for UX
   *  ("슬리피지 가드") and surfaced as Cedar comments in the
   *  compile output. */
  guardId?: string;
  label?: string;
  /** Inspector-only annotations (headline + plain-language summary)
   *  shown to the user when this block is selected. */
  userCopy?: { headline?: string; plain?: string };
  enabled?: boolean;
  parentId: NodeId | null;
  x: number;
  y: number;
}

export interface PredicateNode {
  id: NodeId;
  type: "predicate";
  /** Dotted-path field name from V7_GLOSS, e.g. `context.slippageBp`. */
  param: string;
  /** Subset of {@link FieldKind} pinned for this specific block —
   *  written by Inspector when the user fixes the type (e.g. forcing
   *  a `String` param to be compared `eq` rather than `in`). */
  fieldKind: FieldKind;
  op: Op;
  value: PredicateValue | null;
  /** How to evaluate when the runtime value is absent — `treatAsFalse`
   *  is the default for `enrichment.*` fields so a missing host enrich
   *  doesn't accidentally block the user. */
  absence?: "treatAsFalse" | "treatAsTrue";
  /** Same identity helpers as LogicNode. */
  guardId?: string;
  label?: string;
  userCopy?: { headline?: string; plain?: string };
  enabled?: boolean;
  parentId: NodeId | null;
  /** A predicate that exists on the canvas but isn't connected to the
   *  tree. Rendered as a floating "draft" chip; excluded from compile. */
  float?: boolean;
  note?: string;
  x: number;
  y: number;
}

export type Node = HatNode | LogicNode | PredicateNode;

export interface Doc {
  /** Flat node map. Source of truth for all canvas state. */
  nodes: Record<NodeId, Node>;
  hatId: NodeId;
  /** The root LogicNode under the hat. */
  rootId: NodeId;
  /** Floating (unconnected) predicate ids. Surfaced to remind the user
   *  there are drafts not yet plugged in. */
  drafts: NodeId[];
  locale: "ko" | "en";
  /** Human-friendly name shown in the policy list. */
  policyName: string;
  /** Cedar action id this policy targets — duplicated from `hat.action`
   *  for convenience (filtering the palette in the canvas). */
  action: string;
  /** Optional reason string the extension surfaces on a deny verdict. */
  denyMessage?: string;
  /** Header text shown above the canvas in reading mode. */
  readingHeader?: string;
  /** Pan/zoom of the infinite canvas. Persisted with the doc so the
   *  user re-enters where they left off. */
  pan: { x: number; y: number };
  zoom: number;
}
