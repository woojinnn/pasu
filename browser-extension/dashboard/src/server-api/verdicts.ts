/**
 * Verdict log client — chrome.storage.local-backed via the extension bridge.
 *
 * The server-side `/verdicts`, `/audit/*`, `/history/verdicts`,
 * `/findings/feed`, and `PATCH /verdicts/:id` endpoints have been retired.
 * The SW now owns the audit log in `chrome.storage.local` (see
 * `browser-extension/backend/service-worker/verdict-storage.ts`).
 *
 * This module keeps the same EXPORTED function names + signatures the
 * dashboard's React Query hooks use — only the implementation switched from
 * `fetch` to `sendToExtension`.
 *
 * Notable contract change: `id` is now a UUID string (assigned by
 * `crypto.randomUUID` inside the SW), not an autoincrement i64. Pages and
 * components that previously typed `id: number` were updated alongside.
 *
 * Removed:
 * - `createVerdict` — the SW writes verdicts itself at the end of
 *   `decideMessage`. The dashboard no longer POSTs verdicts.
 *
 * Changed:
 * - `auditExportUrl(opts) → string` is now `exportAuditCsv(opts) → Promise<Blob>`
 *   because the URL was an authenticated server route; the SW now hands back
 *   the CSV body directly. Caller wraps the Blob in a `URL.createObjectURL`
 *   anchor download.
 */

import type {
  Address,
  I18nString,
  PolicySeverity,
  UnixSeconds,
  Verdict,
} from "./types";

import { sendToExtension } from "./extension-bridge";

// ---------- shared dto ----------

export interface ContractRef {
  addr: Address;
  symbol?: string | null;
}

export interface SelectorRef {
  sig: string;
  decoded?: string | null;
}

export interface PolicyRef {
  id: number | null;
  name: string | null;
  severity: PolicySeverity;
}

export interface VerdictDto {
  /** UUID string assigned by the SW at append time (replaces the old DB autoincrement). */
  id: string;
  ts: UnixSeconds;
  wallet: Address | null;
  verdict: Verdict;
  severity: PolicySeverity;
  method?: string | null;
  decoded_fn?: string | null;
  dapp_origin?: string | null;
  contract?: ContractRef;
  selector?: SelectorRef;
  policy?: PolicyRef;
  /** Both locales; the FE picks one (Decision #8). */
  reason: { ko?: string | null; en?: string | null };
  user_decision: "trusted" | "cancelled" | null;
  decided_at: UnixSeconds | null;
  delta_id: number | null;
}

// ---------- query shape ----------

export type VerdictRangeAlias = "1h" | "6h" | "24h" | "7d";

export interface VerdictListOpts {
  /** "1h" / "6h" / "24h" / "7d" — overrides `since`/`until` when set. */
  range?: VerdictRangeAlias;
  since?: UnixSeconds;
  until?: UnixSeconds;
  verdict?: Verdict;
  origin?: string;
  policy_id?: number;
  wallet?: Address;
  /** Substring search across policy_name + reason_en + reason_ko. */
  search?: string;
  /** Cursor — fetch rows older than this unix-seconds timestamp. */
  before?: UnixSeconds;
  /** Default 50, max 500. */
  limit?: number;
}

// ---------- read endpoints (bridge-routed) ----------
//
// Browser-page context (no extension installed) → these calls would hang
// for the bridge default 10s before timing out, leaving every verdict
// view stuck on "불러오는 중…". We short-circuit with a 800 ms probe:
// if the bridge can't answer in that window, return a safe empty value
// so React Query lands on a clean empty state instead of an error.
const BRIDGE_PROBE_MS = 800;
async function safeBridge<T>(payload: unknown, fallback: T): Promise<T> {
  try {
    return await sendToExtension<T>(payload, BRIDGE_PROBE_MS);
  } catch {
    return fallback;
  }
}

/** Filtered list. Default newest-first; the SW applies `opts.limit`. */
export async function listAuditVerdicts(
  opts: VerdictListOpts = {},
): Promise<VerdictDto[]> {
  return safeBridge<VerdictDto[]>({ type: "verdicts:list", opts }, []);
}

/** Pass/warn/fail summary under the same filter as `listAuditVerdicts`. */
export async function getAuditCounts(
  opts: VerdictListOpts = {},
): Promise<{ pass: number; warn: number; fail: number }> {
  return safeBridge<{ pass: number; warn: number; fail: number }>(
    { type: "verdicts:count", opts },
    { pass: 0, warn: 0, fail: 0 },
  );
}

/**
 * Fetch the filtered slice as CSV and wrap in a Blob. Callers create an
 * object URL + anchor download. (Replaces the old `auditExportUrl`, which
 * returned a server route the page opened directly.)
 *
 * Unlike the list/count calls, this one stays on the long 10 s timeout
 * and surfaces failures — the user explicitly clicked "Export CSV", so
 * silent fallback to an empty file would be confusing.
 */
export async function exportAuditCsv(
  opts: VerdictListOpts = {},
): Promise<Blob> {
  const { csv } = await sendToExtension<{ csv: string }>({
    type: "verdicts:export-csv",
    opts,
  });
  return new Blob([csv], { type: "text/csv;charset=utf-8" });
}

/** Same row shape as audit, paginated via `before` cursor. */
export async function listHistoryVerdicts(
  opts: VerdictListOpts = {},
): Promise<VerdictDto[]> {
  return safeBridge<VerdictDto[]>(
    { type: "verdicts:list", opts: { ...opts, before: opts.before } },
    [],
  );
}

/** Recent stream for the monitoring page. Defaults to limit 20. */
export async function listFindings(
  opts: VerdictListOpts = {},
): Promise<VerdictDto[]> {
  return safeBridge<VerdictDto[]>(
    { type: "verdicts:list", opts: { ...opts, limit: opts.limit ?? 20 } },
    [],
  );
}

// ---------- write endpoint ----------

/**
 * Resolve a `warn` row's user decision. Id changed from `number` to `string`
 * to match the SW's UUID assignment.
 */
export async function setVerdictDecision(
  id: string,
  decision: "trusted" | "cancelled",
): Promise<void> {
  await sendToExtension<{ updated: boolean }>({
    type: "verdicts:set-decision",
    id,
    decision,
  });
}

// ---------- re-exports for code that still imports the old shapes ----------

/** Kept for callers that imported the verdict-creation request body type.
 *  The dashboard no longer creates verdicts directly (the SW does), so this
 *  is documentation-only — left as a typed comment of the historical shape. */
export interface CreateVerdictBody {
  wallet: Address;
  verdict: Verdict;
  severity: PolicySeverity;
  delta_id?: number;
  policy_id?: number;
  dapp_origin?: string;
  method?: string;
  decoded_fn?: string;
  contract?: ContractRef;
  selector?: SelectorRef;
  policy_name?: string;
  reason?: I18nString | { ko?: string; en?: string };
}
