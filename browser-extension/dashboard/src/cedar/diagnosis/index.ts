/**
 * Public barrel for denial diagnosis. **Start at `./README.md`** for the mental
 * model and the end-to-end integration recipe.
 *
 * Typical consumer flow (see `editor-v9/Workspace.tsx::onSimulate` for the live
 * reference):
 *
 *   const { probes, diagnosable } = buildProbes(policy);   // 1. enumerate boolean nodes
 *   const result = await runDiagnosisProbes({ ...sample(), probes }); // 2. Cedar oracle (WASM); sample is a factory
 *   const { culprits } = diagnoseFromResult(policy, probes.map(p => p.id), result); // 3. blame
 *
 * `runDiagnosisProbes` lives in `../../server-api/diagnosis` (the bridge call).
 * `blame` is re-exported for advanced use, but most callers go through
 * `diagnoseFromResult`, which builds the false-inclusive truth map and strips
 * errored paths for you.
 */
export { buildProbes, isBooleanNode, type Probe, type ProbeSet } from "./probes";
export { blame, type TruthMap } from "./blame";
export { nodeAtPath, eachChild, type Child } from "./path";

import type { PolicyIR } from "../blocks/ir";
import { blame, type TruthMap } from "./blame";
import type { DiagnosisResultDto } from "../../server-api/diagnosis";

export interface Diagnosis {
  /** Structural paths of the responsible leaf nodes (highlight these). */
  culprits: string[];
  /** Paths whose probe errored (render a distinct "uneval" state). */
  errored: string[];
}

/**
 * Turn a WASM truth-map result into culprit leaf paths via the blame walker.
 *
 * @param policy   the SAME `PolicyIR` object the probes were built from (object
 *                 identity matters downstream — see README §4).
 * @param probeIds every probe id sent to the oracle (`probes.map(p => p.id)`).
 *                 Used to build a *false-inclusive* truth map: any id NOT in
 *                 `result.true_ids` is recorded as `false`, which is what makes
 *                 the `unless` / false-side blame branches fire correctly.
 * @param result   `{ true_ids, error_ids }` from `runDiagnosisProbes`.
 * @returns `culprits` (responsible leaf paths — highlight these) and `errored`
 *          (paths whose probe errored — render a distinct "uneval" state; these
 *          are excluded from `culprits`, never shown as a confident red box).
 */
export function diagnoseFromResult(
  policy: PolicyIR,
  probeIds: string[],
  result: DiagnosisResultDto,
): Diagnosis {
  const trueSet = new Set(result.true_ids);
  const errSet = new Set(result.error_ids);
  const truth: TruthMap = {};
  for (const id of probeIds) truth[id] = trueSet.has(id);
  const culprits = blame(policy, truth).filter((p) => !errSet.has(p));
  return { culprits, errored: result.error_ids };
}
