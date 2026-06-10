/**
 * "위험 상황" 카드 모델 — UI 계층이 FormModel의 평탄 joiner 리스트를 2층
 * (상황 = AND-run, 상황들끼리 OR)으로 다루기 위한 순수 헬퍼.
 * 의미론의 단일 원천은 convert.ts의 `splitRuns`(같은 함수를 재사용)다.
 */

import { splitRuns } from "./convert";
import type { FormCondition, FormGroupNode, FormNode, GroupOp } from "./model";
import { isGroupNode } from "./model";

/** Node list → situations (AND-runs, split at each `or` joiner). */
export function situationsOf(nodes: FormNode[]): FormNode[][] {
  return nodes.length === 0 ? [] : splitRuns(nodes);
}

const withJoiner = (n: FormNode, joiner: GroupOp): FormNode =>
  n.joiner === joiner ? n : { ...n, joiner };

/** Situations → flat node list, joiners normalized (run head `or`, rest `and`;
 *  the very first node's joiner is ignored by convert). Empty runs dropped. */
export function flattenSituations(runs: FormNode[][]): FormNode[] {
  const out: FormNode[] = [];
  for (const run of runs) {
    run.forEach((n, i) => out.push(withJoiner(n, i === 0 ? "or" : "and")));
  }
  return out;
}

/** Every leaf condition anywhere under `nodes` (recursive). */
export function conditionsDeep(nodes: FormNode[]): FormCondition[] {
  return nodes.flatMap((n) => (isGroupNode(n) ? conditionsDeep(n.conds) : [n]));
}

/** Where a dragged condition can land. */
export type DropTarget =
  | { kind: "situation"; index: number }
  | { kind: "group"; group: FormGroupNode }
  | { kind: "new-situation" };

/** Remove `cond` (by identity) wherever it sits, recursively. A group emptied
 *  by the removal is dropped; untouched nodes keep their identity. */
function removeDeep(
  nodes: FormNode[],
  cond: FormCondition,
): { nodes: FormNode[]; changed: boolean } {
  let changed = false;
  const out: FormNode[] = [];
  for (const n of nodes) {
    if (n === cond) {
      changed = true;
      continue;
    }
    if (isGroupNode(n)) {
      const r = removeDeep(n.conds, cond);
      if (r.changed) {
        changed = true;
        if (r.nodes.length > 0) out.push({ ...n, conds: r.nodes });
      } else out.push(n);
    } else out.push(n);
  }
  return { nodes: out, changed };
}

/** Append `cond` to `group` (matched by identity anywhere in the tree).
 *  `found` is false when the reference didn't match (e.g. the group object was
 *  rebuilt because the dragged row was removed from inside it). */
function appendToGroup(
  nodes: FormNode[],
  group: FormGroupNode,
  cond: FormCondition,
): { nodes: FormNode[]; found: boolean } {
  let found = false;
  const out = nodes.map((n): FormNode => {
    if (!isGroupNode(n) || found) return n;
    if (n === group) {
      found = true;
      return { ...n, conds: [...n.conds, { ...cond, joiner: "or" as const }] };
    }
    const r = appendToGroup(n.conds, group, cond);
    if (r.found) {
      found = true;
      return { ...n, conds: r.nodes };
    }
    return n;
  });
  return { nodes: out, found };
}

/** Move `cond` (matched by identity; a row anywhere in the tree) to `target`.
 *  Pure; an emptied group is dropped. No-op if already in the target group, or
 *  when the target group's identity didn't survive the removal (dropping a row
 *  onto its own ancestor group). */
export function moveCondTo(nodes: FormNode[], cond: FormCondition, target: DropTarget): FormNode[] {
  if (target.kind === "group" && target.group.conds.includes(cond)) return nodes;
  const removed = removeDeep(nodes, cond).nodes;
  if (target.kind === "group") {
    const r = appendToGroup(removed, target.group, cond);
    return r.found ? r.nodes : nodes;
  }
  const runs = situationsOf(removed);
  if (target.kind === "new-situation" || runs.length === 0) {
    return flattenSituations([...runs, [{ ...cond, joiner: "and" }]]);
  }
  const i = Math.min(target.index, runs.length - 1);
  runs[i] = [...runs[i], { ...cond, joiner: "and" }];
  return flattenSituations(runs);
}
