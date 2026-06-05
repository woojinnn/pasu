/**
 * Diagnosis-context log — parallel to `state-delta-storage`, keyed by the same
 * `decisionId` (UUID) the verdict log references via `delta_id`.
 *
 * Why store this? A history/confirm-popup deny wants to show *which clause*
 * blocked the tx, computed against the SAME context the live verdict used — not
 * a generic sample. The dashboard can't recompute that context (it never saw the
 * live enrichment), so at deny time we capture the inputs the diagnosis needs —
 * the decoded `action`/`meta`, the `tx`, and the materialized enrichment
 * `results` — and the dashboard re-runs the diagnosis on demand via the existing
 * `run-diagnosis-probes` oracle op (Option B: store context, dashboard re-runs).
 *
 * Only written for DENY decisions (the only case a culprit is meaningful), so
 * the ring buffer stays small. Cap mirrors `state-delta-storage` (500 rows).
 */

import Browser from "webextension-polyfill";

const STORAGE_KEY = "diagnosis-contexts:log";
export const MAX_ROWS = 500;

/** The inputs a dashboard surface needs to re-run denial diagnosis for a single
 *  decision. `id` is the shared `decisionId`; `action`/`meta`/`tx`/`results`
 *  reproduce the exact context the live verdict was computed against. */
export interface DiagnosisContextRow {
  id: string;
  ts: number;
  /** Decoded `ActionBody` the verdict ran on. */
  action: unknown;
  /** Action meta (submitter, nature, …). */
  meta: unknown;
  /** Transaction frame the lowering uses for `$.root.*`. */
  tx: { chain_id: string; from: string; to: string };
  /** Materialized enrichment map (`call_id` → value) the verdict used, so a
   *  `context.custom.*` policy's culprit reproduces. */
  results: Record<string, unknown>;
}

export async function listAllDiagnosisContexts(): Promise<DiagnosisContextRow[]> {
  const raw = (await Browser.storage.local.get(STORAGE_KEY)) as Record<
    string,
    unknown
  >;
  const rows = raw[STORAGE_KEY];
  return Array.isArray(rows) ? (rows as DiagnosisContextRow[]) : [];
}

/** Append a row, evicting the oldest when the ring buffer is full. Newest wins
 *  on a duplicate id (a re-decode of the same decision overwrites). */
export async function appendDiagnosisContext(
  row: DiagnosisContextRow,
): Promise<DiagnosisContextRow> {
  const current = await listAllDiagnosisContexts();
  const filtered = current.filter((r) => r.id !== row.id);
  const next = [...filtered, row];
  const trimmed = next.length > MAX_ROWS ? next.slice(-MAX_ROWS) : next;
  await Browser.storage.local.set({ [STORAGE_KEY]: trimmed });
  return row;
}

export async function getDiagnosisContext(
  id: string,
): Promise<DiagnosisContextRow | null> {
  const rows = await listAllDiagnosisContexts();
  return rows.find((r) => r.id === id) ?? null;
}

export async function clearDiagnosisContexts(): Promise<void> {
  await Browser.storage.local.remove(STORAGE_KEY);
}
