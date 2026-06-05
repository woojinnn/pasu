/**
 * Dashboard ↔ SW bridge for the diagnosis-context log
 * (`diagnosis-context-storage.ts`). A history/confirm-popup deny row carries a
 * `delta_id` (UUID = `message.requestId` at decision time); calling
 * `getDiagnosisContextRow(delta_id)` returns the inputs the live verdict was
 * computed against — the decoded `action`/`meta`, the `tx`, and the materialized
 * enrichment `results` — so the dashboard can re-run "which clause blocked this"
 * against the REAL context (not a sample).
 *
 * Returns `null` for non-deny rows, legacy rows, or when the extension isn't
 * installed (fails soft, like the other extension-sync helpers).
 */

import { sendToExtension, ExtensionBridgeTimeout } from "./extension-bridge";

/** Mirror of the SW's `DiagnosisContextRow`. `action`/`meta`/`results` are
 *  opaque here; the diagnosis runner forwards them verbatim to the WASM oracle. */
export interface DiagnosisContextRow {
  id: string;
  ts: number;
  action: unknown;
  meta: unknown;
  tx: { chain_id: string; from: string; to: string };
  results: Record<string, unknown>;
}

export async function getDiagnosisContextRow(
  id: string,
): Promise<DiagnosisContextRow | null> {
  try {
    return await sendToExtension<DiagnosisContextRow | null>({
      type: "diagnosis-context:get",
      id,
    });
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return null;
    throw err;
  }
}
