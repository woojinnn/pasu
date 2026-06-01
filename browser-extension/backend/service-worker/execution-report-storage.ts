import Browser from "webextension-polyfill";

import type { ExecutionReportPayload } from "@lib/types";

const STORAGE_KEY = "execution-reports:log";

export const MAX_ROWS = 500;

export interface ExecutionReportRow extends ExecutionReportPayload {
  id: string;
  ts: number;
}

export type ExecutionReportInsert = ExecutionReportPayload;

export interface ExecutionReportFilter {
  wallet?: string;
  hostname?: string;
  since?: number;
  until?: number;
  limit?: number;
}

export interface ExecutionReportCounts {
  total: number;
  byKind: Record<string, number>;
}

export async function listAllExecutionReports(): Promise<ExecutionReportRow[]> {
  const result = await Browser.storage.local.get(STORAGE_KEY);
  const raw = result[STORAGE_KEY];
  return Array.isArray(raw) ? (raw as ExecutionReportRow[]) : [];
}

export function applyFilter(
  rows: ExecutionReportRow[],
  opts?: ExecutionReportFilter,
): ExecutionReportRow[] {
  if (!opts) return rows;

  let filtered = rows;
  const hostNeedle = opts.hostname?.toLowerCase();

  if (opts.wallet) {
    const walletNeedle = opts.wallet.toLowerCase();
    filtered = filtered.filter((row) => {
      const address = (row.wallet_id as { address?: string } | undefined)?.address;
      return address?.toLowerCase() === walletNeedle;
    });
  }
  if (hostNeedle) {
    filtered = filtered.filter((row) =>
      (row.hostname ?? "").toLowerCase().includes(hostNeedle),
    );
  }
  if (opts.since !== undefined) {
    filtered = filtered.filter((row) => row.ts >= opts.since!);
  }
  if (opts.until !== undefined) {
    filtered = filtered.filter((row) => row.ts <= opts.until!);
  }
  if (opts.limit !== undefined) {
    filtered = filtered.slice(0, opts.limit);
  }

  return filtered;
}

export async function listExecutionReports(
  opts?: ExecutionReportFilter,
): Promise<ExecutionReportRow[]> {
  return applyFilter(await listAllExecutionReports(), opts);
}

export async function countExecutionReports(
  opts?: ExecutionReportFilter,
): Promise<ExecutionReportCounts> {
  const rows = await listExecutionReports(opts);
  const byKind: Record<string, number> = {};
  for (const row of rows) {
    const kind = (row.outcome as { kind?: string } | undefined)?.kind ?? "unknown";
    byKind[kind] = (byKind[kind] ?? 0) + 1;
  }
  return { total: rows.length, byKind };
}

export async function appendExecutionReport(
  insert: ExecutionReportInsert,
): Promise<ExecutionReportRow> {
  const row: ExecutionReportRow = {
    ...insert,
    id: crypto.randomUUID(),
    ts: Math.floor(Date.now() / 1000),
  };
  const rows = await listAllExecutionReports();
  rows.unshift(row);
  if (rows.length > MAX_ROWS) rows.length = MAX_ROWS;
  await Browser.storage.local.set({ [STORAGE_KEY]: rows });
  return row;
}

export async function clearExecutionReports(): Promise<void> {
  await Browser.storage.local.remove(STORAGE_KEY);
}
