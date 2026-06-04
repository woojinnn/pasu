/**
 * Editor-level validation result. Surfaced to the Workspace shell so the user
 * sees what's wrong before they can save (and so the save button can gate).
 *
 * Three layers feed this:
 *   L2 — workspaceToIR fills `EditorError.kind = "structural"` (missing inputs,
 *        unmapped blocks).
 *   L3 — blocksToEst throws → caught and surfaced as `"holes"` (unfilled
 *        parameters) or `"est"` (structural reject by cedar/blocks).
 *   L4 — wasm est_json_to_policy_text throws → `"cedar"` (engine rejected
 *        the EST; defensive, shouldn't happen for IR shapes we generate).
 *
 * Phase A only emits L2 errors. L3/L4 land alongside parameterization (Phase E).
 */

import type { PolicyIR } from "../cedar/blocks";

export type EditorErrorKind = "structural" | "holes" | "est" | "cedar";

export interface EditorError {
  kind: EditorErrorKind;
  /** Short user-facing message ("when 절이 비었습니다"). */
  message: string;
  /** Blockly block id the error attaches to, if any. Lets the shell jump to
   *  the offending block. */
  blockId?: string;
}

export interface ValidationResult {
  /** True iff no errors AND IR is non-null (i.e. a policy was buildable). */
  ok: boolean;
  ir: PolicyIR | null;
  errors: EditorError[];
}

/** Phase-A stub: a non-null IR is considered valid. Real per-Expr-kind checks
 *  arrive with Phase B (e.g. binary missing right operand). Holes/Cedar checks
 *  arrive with Phase E. */
export function validateIR(ir: PolicyIR | null, errors: EditorError[]): ValidationResult {
  if (ir === null || errors.length > 0) {
    return { ok: false, ir, errors };
  }
  return { ok: true, ir, errors: [] };
}
