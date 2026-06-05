import type { PolicyIR, Expr } from "../blocks/ir";
import { blocksToEst } from "../blocks/blocksToEst";
import { eachChild } from "./path";

/** Boolean-valued extension functions (from spike S3). */
const BOOL_EXT = new Set([
  "greaterThan", "greaterThanOrEqual", "lessThan", "lessThanOrEqual",
  "isInRange", "isIpv4", "isIpv6", "isLoopback", "isMulticast",
]);

const BOOL_BINARY = new Set([
  "==", "!=", "<", "<=", ">", ">=", "in", "contains", "containsAll", "containsAny",
]);

/** True iff this node is a boolean-valued expression (safe to wrap in `when`). */
export function isBooleanNode(e: Expr): boolean {
  switch (e.kind) {
    case "binary": return BOOL_BINARY.has(e.op) || e.op === "&&" || e.op === "||";
    case "unary": return e.op === "!" || e.op === "isEmpty";
    case "has":
    case "like":
    case "is": return true;
    case "ext": return BOOL_EXT.has(e.fn);
    default: return false; // var, lit, attr, set, record, if, litEntity, raw, hole
  }
}

/** A probe: a permit policy (EST) wrapping a boolean subtree, keyed by path. */
export interface Probe {
  id: string;
  est: unknown;
}

export interface ProbeSet {
  probes: Probe[];
  /** True iff the policy is fully diagnosable (no hole/raw under any clause). */
  diagnosable: boolean;
}

/** A synthetic unconstrained permit wrapping `body`, annotated with `@id(path)`. */
function probePolicy(path: string, body: Expr): PolicyIR {
  return {
    kind: "policy", effect: "permit", annotations: [{ name: "id", value: path }],
    scope: { principal: { kind: "scopeAll" }, action: { kind: "scopeAll" }, resource: { kind: "scopeAll" } },
    conditions: [{ kind: "when", body }],
  };
}

/** Whether a child sits in BOOLEAN POSITION given its parent and the path step
 *  reaching it (plus whether the PARENT is itself in boolean position). Pure
 *  function of `(parent.kind, step, parentBoolPos)`; the step strings mirror
 *  `eachChild`'s labels (the sanctioned coupling — paths still come from
 *  `eachChild`). A node in boolean position is evaluated as a Bool by Cedar's
 *  grammar even when `isBooleanNode` is false for its kind (a Bool `attr`/`lit`,
 *  or an `if` whose result is Bool), so it must be probed to give blame a truth. */
function childBoolPos(parent: Expr, step: string, parentBoolPos: boolean): boolean {
  switch (parent.kind) {
    case "binary":
      return (parent.op === "&&" || parent.op === "||") && (step === "left" || step === "right");
    case "unary":
      return parent.op === "!" && step === "operand";
    case "if":
      // `cond` is ALWAYS boolean; `then`/`else` are boolean IFF the `if` is.
      if (step === "cond") return true;
      if (step === "then" || step === "else") return parentBoolPos;
      return false;
    default:
      return false;
  }
}

/** ENTRY POINT (consumers call this). Enumerate every boolean-POSITION node (and
 *  every structurally-boolean node) of `policy` and build one probe each — a
 *  `permit(...) when { <subtree> }` EST tagged with the node's structural path as
 *  its `@id`. Pass the resulting `probes` to `runDiagnosisProbes` and the path
 *  ids back into `diagnoseFromResult`.
 *
 *  Subtrees containing a `hole`/`raw` node are skipped and flip `diagnosable` to
 *  `false`; when `diagnosable === false`, do NOT run the oracle — fall back to the
 *  policy's static `@reason` annotation (a partial probe set would mis-attribute). */
export function buildProbes(policy: PolicyIR): ProbeSet {
  const probes: Probe[] = [];
  let diagnosable = true;

  const hasUninterpretable = (e: Expr): boolean => {
    if (e.kind === "hole" || e.kind === "raw") return true;
    for (const c of eachChild(e)) if (hasUninterpretable(c.node)) return true;
    return false;
  };

  // A clause `when`/`unless` body root is in boolean position by Cedar's
  // grammar. From there boolean position propagates down per `childBoolPos`
  // (binary connectives, `!`, and `if`). Probing every boolean-position node —
  // not just the structurally-boolean ones — ensures blame's truth lookup is
  // defined for Bool `attr`/`lit`/`if` operands too, so they (and their
  // subtrees) are no longer silently dropped.
  const visit = (e: Expr, path: string, inBoolPos: boolean): void => {
    if (e.kind === "hole" || e.kind === "raw") { diagnosable = false; return; }
    if ((inBoolPos || isBooleanNode(e)) && !hasUninterpretable(e)) {
      probes.push({ id: path, est: blocksToEst(probePolicy(path, e)) });
    }
    for (const c of eachChild(e))
      visit(c.node, `${path}.${c.step}`, childBoolPos(e, c.step, inBoolPos));
  };

  policy.conditions.forEach((cond, i) => visit(cond.body, `c${i}.body`, true));
  return { probes, diagnosable };
}
