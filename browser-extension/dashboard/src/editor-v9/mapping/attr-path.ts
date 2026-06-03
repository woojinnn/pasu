/**
 * Attribute-chain ↔ dotted-path utilities.
 *
 * A Cedar attribute access like `context.tokenIn.key.address` is shaped in
 * PolicyIR as a left-leaning chain of `attr` nodes terminating in a `var`:
 *
 *   attr( attr( attr( var("context"), "tokenIn"), "key"), "address")
 *
 * `chainToDottedPath` flattens such a chain into the dotted string
 * (`"context.tokenIn.key.address"`); `dottedPathToChain` does the reverse.
 *
 * Used by the field-block render path (irToWorkspace) to detect chains that
 * match a gloss entry and collapse them into a single field block, and by
 * the workspaceToIR side to expand a chosen field block back into the
 * canonical attr chain (the IR shape cedar/blocks expects).
 */

import type { Expr, VarName } from "../../cedar/blocks";

/** True iff `name` is a Cedar request variable that may root an attr chain. */
function isVarName(name: string): name is VarName {
  return name === "principal" || name === "action" || name === "resource" || name === "context";
}

/** Flatten an attr-chain Expr into `[root, attr0, attr1, ...]`. Returns null
 *  if `e` is not a chain rooted at a `var`. */
export function chainToSegments(e: Expr): readonly string[] | null {
  const acc: string[] = [];
  let cur: Expr = e;
  while (cur.kind === "attr") {
    acc.unshift(cur.attr);
    cur = cur.of;
  }
  if (cur.kind === "var") {
    acc.unshift(cur.name);
    return acc;
  }
  return null;
}

/** Convenience — flatten + join with `.`. */
export function chainToDottedPath(e: Expr): string | null {
  const segs = chainToSegments(e);
  return segs ? segs.join(".") : null;
}

/** Build the canonical attr chain for a dotted path. Root segment MUST be a
 *  Cedar request variable (principal/action/resource/context). Gloss paths
 *  are kept valid Cedar paths so non-stdlib roots never reach this helper —
 *  we return null defensively. */
export function dottedPathToChain(path: string): Expr | null {
  const parts = path.split(".");
  if (parts.length < 2) return null; // a bare "context" alone isn't a field
  const [root, ...rest] = parts;
  if (!root || !isVarName(root)) return null;
  let cur: Expr = { kind: "var", name: root };
  for (const attr of rest) {
    cur = { kind: "attr", of: cur, attr };
  }
  return cur;
}
