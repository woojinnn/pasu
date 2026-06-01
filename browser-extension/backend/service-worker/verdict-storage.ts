import Browser from "webextension-polyfill";

const STORAGE_KEY = "verdicts:log";

export const MAX_ROWS = 1000;

export type Verdict = "pass" | "warn" | "fail";
export type PolicySeverity = "deny" | "warn" | "info";

export interface ContractRef {
  addr: string;
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

export interface VerdictRow {
  id: string;
  ts: number;
  wallet: string | null;
  verdict: Verdict;
  severity: PolicySeverity;
  method?: string | null;
  decoded_fn?: string | null;
  dapp_origin?: string | null;
  contract?: ContractRef;
  selector?: SelectorRef;
  policy?: PolicyRef;
  reason: { ko?: string | null; en?: string | null };
  user_decision: "trusted" | "cancelled" | null;
  decided_at: number | null;
  delta_id: number | null;
}

export type VerdictInsert = Omit<
  VerdictRow,
  "id" | "user_decision" | "decided_at"
>;

export interface VerdictFilter {
  range?: "1h" | "6h" | "24h" | "7d";
  since?: number;
  until?: number;
  verdict?: Verdict;
  origin?: string;
  policy_id?: number;
  wallet?: string;
  search?: string;
  before?: number;
  limit?: number;
}

export interface VerdictCounts {
  pass: number;
  warn: number;
  fail: number;
}

export async function listAllVerdicts(): Promise<VerdictRow[]> {
  const result = await Browser.storage.local.get(STORAGE_KEY);
  const raw = result[STORAGE_KEY];
  return Array.isArray(raw) ? (raw as VerdictRow[]) : [];
}

export function applyFilter(
  rows: VerdictRow[],
  opts?: VerdictFilter,
): VerdictRow[] {
  if (!opts) return rows;

  const now = Math.floor(Date.now() / 1000);
  const since =
    opts.since ??
    (opts.range
      ? now -
        ({ "1h": 3600, "6h": 21600, "24h": 86400, "7d": 604800 }[
          opts.range
        ] ?? 0)
      : undefined);
  const originNeedle = opts.origin?.toLowerCase();
  const walletNeedle = opts.wallet?.toLowerCase();
  const searchNeedle = opts.search?.trim().toLowerCase();

  let filtered = rows;
  if (since !== undefined) {
    filtered = filtered.filter((row) => row.ts >= since);
  }
  if (opts.until !== undefined) {
    filtered = filtered.filter((row) => row.ts <= opts.until!);
  }
  if (opts.before !== undefined) {
    filtered = filtered.filter((row) => row.ts < opts.before!);
  }
  if (opts.verdict) {
    filtered = filtered.filter((row) => row.verdict === opts.verdict);
  }
  if (originNeedle) {
    filtered = filtered.filter((row) =>
      (row.dapp_origin ?? "").toLowerCase().includes(originNeedle),
    );
  }
  if (walletNeedle) {
    filtered = filtered.filter(
      (row) => row.wallet?.toLowerCase() === walletNeedle,
    );
  }
  if (opts.policy_id !== undefined) {
    filtered = filtered.filter((row) => row.policy?.id === opts.policy_id);
  }
  if (searchNeedle) {
    filtered = filtered.filter((row) => {
      const haystack = [
        row.policy?.name,
        row.reason?.ko,
        row.reason?.en,
        row.method,
        row.decoded_fn,
        row.dapp_origin,
        row.contract?.addr,
        row.selector?.sig,
      ]
        .filter(Boolean)
        .join(" ")
        .toLowerCase();
      return haystack.includes(searchNeedle);
    });
  }
  if (opts.limit !== undefined) {
    filtered = filtered.slice(0, opts.limit);
  }

  return filtered;
}

export async function listVerdicts(
  opts?: VerdictFilter,
): Promise<VerdictRow[]> {
  return applyFilter(await listAllVerdicts(), opts);
}

export async function countVerdicts(
  opts?: VerdictFilter,
): Promise<VerdictCounts> {
  const rows = await listVerdicts(opts);
  let pass = 0;
  let warn = 0;
  let fail = 0;
  for (const row of rows) {
    if (row.verdict === "pass") pass += 1;
    if (row.verdict === "warn") warn += 1;
    if (row.verdict === "fail") fail += 1;
  }
  return { pass, warn, fail };
}

export async function appendVerdict(insert: VerdictInsert): Promise<VerdictRow> {
  const row: VerdictRow = {
    ...insert,
    id: crypto.randomUUID(),
    user_decision: null,
    decided_at: null,
  };
  const rows = await listAllVerdicts();
  rows.unshift(row);
  if (rows.length > MAX_ROWS) rows.length = MAX_ROWS;
  await Browser.storage.local.set({ [STORAGE_KEY]: rows });
  return row;
}

export async function setVerdictDecision(
  id: string,
  decision: "trusted" | "cancelled",
): Promise<boolean> {
  const rows = await listAllVerdicts();
  const idx = rows.findIndex((row) => row.id === id);
  if (idx < 0) return false;
  rows[idx] = {
    ...rows[idx],
    user_decision: decision,
    decided_at: Math.floor(Date.now() / 1000),
  };
  await Browser.storage.local.set({ [STORAGE_KEY]: rows });
  return true;
}

export async function clearVerdicts(): Promise<void> {
  await Browser.storage.local.remove(STORAGE_KEY);
}

export async function exportVerdictsAsCsv(
  opts?: VerdictFilter,
): Promise<string> {
  const rows = await listVerdicts(opts);
  const header = [
    "id",
    "ts",
    "wallet",
    "severity",
    "verdict",
    "method",
    "decoded_fn",
    "dapp_origin",
    "contract_addr",
    "contract_symbol",
    "selector_sig",
    "selector_decoded",
    "policy_id",
    "policy_name",
    "reason_ko",
    "reason_en",
    "user_decision",
    "decided_at",
  ];
  return [
    header.join(","),
    ...rows.map((row) =>
      [
        row.id,
        row.ts,
        row.wallet,
        row.severity,
        row.verdict,
        row.method,
        row.decoded_fn,
        row.dapp_origin,
        row.contract?.addr,
        row.contract?.symbol,
        row.selector?.sig,
        row.selector?.decoded,
        row.policy?.id,
        row.policy?.name,
        row.reason.ko,
        row.reason.en,
        row.user_decision,
        row.decided_at,
      ]
        .map(csvEscape)
        .join(","),
    ),
  ].join("\n");
}

function csvEscape(value: unknown): string {
  if (value === null || value === undefined) return "";
  const text = String(value);
  if (/[",\n]/.test(text)) {
    return `"${text.replace(/"/g, '""')}"`;
  }
  return text;
}
