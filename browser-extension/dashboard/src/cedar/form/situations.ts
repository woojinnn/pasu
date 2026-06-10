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

/** Where a dragged condition can land. */
export type DropTarget =
  | { kind: "situation"; index: number }
  | { kind: "group"; group: FormGroupNode }
  | { kind: "new-situation" };

/** Move `cond` (matched by identity; a top-level row or a group alternative) to
 *  `target`. Pure; an emptied group is dropped. No-op if already there. */
export function moveCondTo(nodes: FormNode[], cond: FormCondition, target: DropTarget): FormNode[] {
  if (target.kind === "group" && target.group.conds.includes(cond)) return nodes;
  // 1) remove from wherever it is
  const removed: FormNode[] = [];
  for (const n of nodes) {
    if (n === cond) continue;
    if (isGroupNode(n)) {
      const conds = n.conds.filter((x) => x !== cond);
      if (conds.length === 0) continue;
      removed.push(conds.length === n.conds.length ? n : { ...n, conds });
    } else removed.push(n);
  }
  // 2) insert at the target
  if (target.kind === "group") {
    // `cond` was never inside `target.group` (guarded above), so the group's
    // identity survived the removal pass — match it by reference.
    return removed.map((n) =>
      n === target.group ? { ...n, conds: [...n.conds, { ...cond, joiner: "or" as const }] } : n,
    );
  }
  const runs = situationsOf(removed);
  if (target.kind === "new-situation" || runs.length === 0) {
    return flattenSituations([...runs, [{ ...cond, joiner: "and" }]]);
  }
  const i = Math.min(target.index, runs.length - 1);
  runs[i] = [...runs[i], { ...cond, joiner: "and" }];
  return flattenSituations(runs);
}
