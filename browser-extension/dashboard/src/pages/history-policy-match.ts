/** verdict의 policy_id(=Cedar @id annotation) → ps2 def 매칭(순수).
 *  ① skeleton IR의 @id annotation ② def.id 직접 일치. builtin def도 라이브러리에
 *  있으므로 baked 정책의 구조 다이어그램이 처음으로 가능해진다. */
import type { PolicyDef } from "../../../sdk/policy-store-types";

export function annotationIdOf(ir: unknown): string | null {
  const ann = (ir as { annotations?: { name: string; value: string }[] } | null)?.annotations;
  if (!Array.isArray(ann)) return null;
  return ann.find((a) => a.name === "id")?.value ?? null;
}

export function matchDefForVerdict(
  defs: Record<string, PolicyDef>,
  policyId: string,
): PolicyDef | null {
  for (const d of Object.values(defs)) {
    if (annotationIdOf(d.skeleton.ir) === policyId) return d;
  }
  return defs[policyId] ?? null;
}
