import type { PolicyIR, Expr } from "../blocks/ir";
import { eachChild, nodeAtPath, pathByNode } from "./path";

export type TruthMap = Record<string, boolean>;

/** NOTE for consumers: you usually call `diagnoseFromResult` (in `index.ts`),
 *  NOT `blame` directly — it builds the false-inclusive `truth` map from the
 *  oracle result, calls this, then removes errored paths from the RESULT (errored
 *  ids stay in the map as `false`, so blame still sees them). Call `blame`
 *  directly only if you already hold a complete `TruthMap`.
 *
 *  Return the structural paths of the leaf nodes responsible for the forbid
 *  firing, per the AND/OR/NOT rule. `truth[path]` is the probed truth of the
 *  boolean node at `path`. Every boolean-POSITION node is now probed (see
 *  `buildProbes`/`childBoolPos`), so a Bool `attr`/`lit`/`if` operand always has
 *  a truth here; an absent `truth[path]` means the node is NOT in boolean
 *  position (e.g. a comparison's Long operand) and is transparent — `walk` never
 *  descends into such nodes, so it never reads their (absent) truth.
 *  Assumes `policy.effect === "forbid"` (the engine's only user shape).
 *  Paths come exclusively from `pathByNode` (which derives from `eachChild`),
 *  so blame's labels CANNOT drift from the probe builder / editor map. */
export function blame(policy: PolicyIR, truth: TruthMap): string[] {
  const out: string[] = [];
  const nodePath = pathByNode(policy);
  const P = (n: Expr): string => nodePath.get(n) ?? ""; // every walked node is mapped

  // target = the truth value that makes THIS node responsible.
  const walk = (node: Expr, target: boolean): void => {
    const path = P(node);
    switch (node.kind) {
      case "binary": {
        if (node.op === "&&") {
          if (target) { walk(node.left, true); walk(node.right, true); }
          else {
            if (truth[P(node.left)] === false) walk(node.left, false);
            if (truth[P(node.right)] === false) walk(node.right, false);
          }
          return;
        }
        if (node.op === "||") {
          if (target) {
            if (truth[P(node.left)] === true) walk(node.left, true);
            if (truth[P(node.right)] === true) walk(node.right, true);
          } else { walk(node.left, false); walk(node.right, false); }
          return;
        }
        out.push(path); // comparison leaf
        return;
      }
      case "unary": {
        if (node.op === "!") { walk(node.operand, !target); return; }
        out.push(path); // isEmpty leaf
        return;
      }
      case "if": {
        // cond's own truth + the taken branch carry the responsibility.
        const condTrue = truth[P(node.cond)];
        walk(node.cond, condTrue ?? target);
        if (condTrue) walk(node.then, target);
        else walk(node.else, target);
        return;
      }
      case "has":
      case "like":
      case "is":
      case "ext":
        out.push(path); // boolean leaf
        return;
      case "attr":
      case "lit":
        // A Bool `attr`/`lit` is only ever reached by `walk` when it sits in
        // boolean position (an `&&`/`||`/`!` operand, an `if` branch, or a
        // clause body) and is responsible — `walk` never descends into a
        // comparison's operands, so a Long/String `attr`/`lit` is unreachable.
        out.push(path); // boolean-position leaf
        return;
      default:
        // var/set/record/litEntity/raw/hole — transparent; stop.
        return;
    }
  };

  policy.conditions.forEach((cond) => {
    const path = P(cond.body);
    // forbid fired ⇒ each `when` body is responsible-for-true, each `unless` for-false.
    const target = cond.kind === "when";
    if (truth[path] === target) walk(cond.body, target);
  });

  // de-dup, stable order
  return [...new Set(out)];
}

/** Convenience re-export for callers needing node lookup by path. */
export { nodeAtPath, eachChild };
