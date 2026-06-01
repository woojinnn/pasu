import { RequestType, type ExecutionReportPayload } from "@lib/types";

import { appendExecutionReport } from "./execution-report-storage";

/**
 * Best-effort execution report sink.
 *
 * Reports are written into chrome.storage.local so this per-device activity log
 * does not round-trip through the policy server.
 */
export async function reportExecutionOutcome(
  report: ExecutionReportPayload,
): Promise<void> {
  if (report.type !== RequestType.EXECUTION_REPORT) return;

  try {
    await appendExecutionReport(report);
  } catch (err) {
    console.warn("[Scopeball] execution report storage failed", {
      err: err instanceof Error ? err.message : String(err),
    });
  }
}
