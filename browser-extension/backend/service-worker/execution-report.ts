import { RequestType, type ExecutionReportPayload } from "@lib/types";

const DEFAULT_SIMULATION_SERVER_URL = "http://127.0.0.1:8788";

/**
 * Best-effort execution report sink.
 *
 * This is deliberately narrower than the future simulation-server API client:
 * verdict/evaluate traffic still uses the existing policy path, while this
 * adapter only forwards post-policy lifecycle facts to `/execution-report`.
 */
export async function reportExecutionOutcome(
  report: ExecutionReportPayload,
): Promise<void> {
  const {
    type: _type,
    hostname: _hostname,
    bypassed: _bypassed,
    ...body
  } = report;
  if (_type !== RequestType.EXECUTION_REPORT) return;

  try {
    const base =
      process.env.SIMULATION_SERVER_URL ?? DEFAULT_SIMULATION_SERVER_URL;
    const resp = await fetch(`${base}/execution-report`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!resp.ok) {
      console.warn("[Scopeball] execution report rejected", {
        status: resp.status,
        statusText: resp.statusText,
      });
    }
  } catch (err) {
    console.warn("[Scopeball] execution report failed", {
      err: err instanceof Error ? err.message : String(err),
    });
  }
}
