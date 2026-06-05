/**
 * PolicyDiagnosisByText — resolve a policy's Cedar text to IR (via the WASM
 * bridge), then render {@link PolicyDiagnosis}. The convenience wrapper used by
 * surfaces that hold a policy's *text* (a verdict row, a history entry) rather
 * than a live IR. Mount it lazily (only when a "구조 보기" toggle is open) to
 * defer the parse.
 */
import { useQuery } from "@tanstack/react-query";

import { textToBlocks } from "..";
import { PolicyDiagram } from "./PolicyDiagram";
import { PolicyDiagnosis, type DiagnosisContext } from "./PolicyDiagnosis";

export interface PolicyDiagnosisByTextProps {
  cedarText: string;
  compact?: boolean;
  /** Real captured context to diagnose against (else SAMPLE). */
  request?: DiagnosisContext;
  /** Auto-run the diagnosis on mount (for a real captured context). */
  autoRun?: boolean;
  /** Render the structure ONLY — no diagnosis runner. Used when a sample
   *  diagnosis would be misleading (e.g. an enrichment policy with no captured
   *  context, where every `context.custom.*` probe just errors). */
  structureOnly?: boolean;
}

export function PolicyDiagnosisByText({
  cedarText,
  compact,
  request,
  autoRun,
  structureOnly,
}: PolicyDiagnosisByTextProps) {
  const q = useQuery({
    queryKey: ["policy-diagram-ir-by-text", cedarText],
    queryFn: async () => (await textToBlocks(cedarText))[0] ?? null,
    retry: false,
  });

  if (q.isError) {
    return (
      <div className="pdiagram-empty">정책을 파싱할 수 없어 다이어그램을 못 그려요</div>
    );
  }
  if (structureOnly) {
    return <PolicyDiagram ir={q.data ?? null} compact={compact} />;
  }
  return (
    <PolicyDiagnosis
      ir={q.data ?? null}
      compact={compact}
      request={request}
      autoRun={autoRun}
    />
  );
}
