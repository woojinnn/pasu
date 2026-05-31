/**
 * `/verdicts` + `/audit` + `/history` + `/findings` — Cedar policy audit log.
 *
 * Read flow: the editor / audit / history / monitoring pages all fetch
 * the same row shape (`VerdictDto`) with different filters.
 *
 * Write flow: the extension posts a verdict after it locally evaluates a
 * Cedar policy bundle (`POST /verdicts`). The dashboard sets the user's
 * resolution on a `warn` row via `PATCH /verdicts/:id`.
 */

import type {
  Address,
  I18nString,
  PolicySeverity,
  UnixSeconds,
  Verdict,
} from "@scopeball/types";

import { request } from "./client";

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
  id: number;
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
  /** Cursor — fetch rows with `id < before`. Newest-first ordering. */
  before?: number;
  /** Default 50, max 500. */
  limit?: number;
}

function buildQuery(opts: VerdictListOpts): string {
  const params = new URLSearchParams();
  for (const [k, v] of Object.entries(opts)) {
    if (v === undefined || v === null) continue;
    params.set(k, String(v));
  }
  const qs = params.toString();
  return qs ? `?${qs}` : "";
}

// ---------- read endpoints ----------

/** `GET /audit/verdicts` — filtered list. Default newest-first, limit 50. */
export async function listAuditVerdicts(opts: VerdictListOpts = {}): Promise<VerdictDto[]> {
  return request<VerdictDto[]>(`/audit/verdicts${buildQuery(opts)}`);
}

/** `GET /audit/counts` — pass/warn/fail summary under the same filter. */
export async function getAuditCounts(
  opts: VerdictListOpts = {},
): Promise<{ pass: number; warn: number; fail: number }> {
  return request<{ pass: number; warn: number; fail: number }>(
    `/audit/counts${buildQuery(opts)}`,
  );
}

/** `GET /audit/export` — CSV download (caller handles save / blob). */
export function auditExportUrl(opts: VerdictListOpts = {}): string {
  return `/audit/export${buildQuery(opts)}`;
}

/** `GET /history/verdicts` — same shape as audit, paginated via `before` cursor. */
export async function listHistoryVerdicts(opts: VerdictListOpts = {}): Promise<VerdictDto[]> {
  return request<VerdictDto[]>(`/history/verdicts${buildQuery(opts)}`);
}

/** `GET /findings/feed` — recent stream for the monitoring page. */
export async function listFindings(opts: VerdictListOpts = {}): Promise<VerdictDto[]> {
  return request<VerdictDto[]>(`/findings/feed${buildQuery(opts)}`);
}

// ---------- write endpoints ----------

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

/** `POST /verdicts` — extension submits after Cedar evaluation. */
export async function createVerdict(
  body: CreateVerdictBody,
): Promise<{ id: number; ts: UnixSeconds }> {
  return request<{ id: number; ts: UnixSeconds }>("/verdicts", {
    method: "POST",
    body,
  });
}

/** `PATCH /verdicts/:id` — user resolves a `warn` row. */
export async function setVerdictDecision(
  id: number,
  decision: "trusted" | "cancelled",
): Promise<void> {
  await request<void>(`/verdicts/${id}`, {
    method: "PATCH",
    body: { decision },
  });
}
