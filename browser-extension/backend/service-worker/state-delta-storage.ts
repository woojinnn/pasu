/**
 * State-delta log — parallel to `verdict-storage` for the HistoryPage's
 * state-diff drill-down.
 *
 * Why two stores? The verdict log carries one ROW per matched policy,
 * but a single decision produces ONE state delta (the reducer's
 * `StateDelta` for the action). So we key both by `decisionId` (UUID
 * stamped at the top of `decideMessage`): N verdict rows + at most 1
 * state-delta row, joined on that field.
 *
 * Source of the delta: `recordSimulationOnServer` already POSTs the
 * action to `policy-server` and gets back `EvaluateResponseDto`, whose
 * `policyRequest.deltas[0]` is the reducer's predicted delta. We capture
 * that opaque blob here verbatim — the dashboard's `parseStateDelta`
 * projects it into a typed view at render time.
 *
 * Ring-buffer cap: 500 rows (~ 5-10× a typical session of dApp activity).
 * Older rows fall off when the limit is hit, mirroring the verdict log's
 * `MAX_ROWS = 1000` rotation policy.
 */

import Browser from "webextension-polyfill";

const STORAGE_KEY = "state-deltas:log";
export const MAX_ROWS = 500;

/** One persisted decision's state-delta + the inputs that produced it.
 *  `id` is the shared `decisionId` (UUID) the verdict log references via
 *  `delta_id`. The tx fields (chain/from/to/calldata/value) are stored
 *  so the HistoryPage's "다시 시뮬" button can hand them to
 *  `/simulation?...` without an extra round-trip. */
export interface StateDeltaRow {
  id: string;
  ts: number;
  chain: string;
  from: string;
  to: string;
  /** Raw `0x`-prefixed calldata. Empty / `"0x"` for selector-less
   *  native transfers. Some flows (typed-data signatures) have no
   *  calldata at all — we still store an entry so the verdict joins,
   *  but the field is empty. */
  calldata: string;
  /** `msg.value` as a base-10 decimal string. `"0"` when unset. */
  value: string;
  /** Opaque reducer-side `StateDelta`. Shape matches
   *  `policy-server/asset-model/state::StateDelta` (token_changes,
   *  position_changes, pending_changes, gas_paid). Kept opaque here;
   *  the dashboard projects it with `parseStateDelta`. */
  delta: unknown;
}

export async function listAllStateDeltas(): Promise<StateDeltaRow[]> {
  const raw = (await Browser.storage.local.get(STORAGE_KEY)) as Record<
    string,
    unknown
  >;
  const rows = raw[STORAGE_KEY];
  return Array.isArray(rows) ? (rows as StateDeltaRow[]) : [];
}

/** Append a row, evicting the oldest when the ring buffer is full. Returns
 *  the persisted row so the caller can confirm the id round-tripped. */
export async function appendStateDelta(
  row: StateDeltaRow,
): Promise<StateDeltaRow> {
  const current = await listAllStateDeltas();
  // Defensive: skip duplicate ids (a retry of `recordSimulationOnServer`
  // shouldn't double-append). Newest wins so a retry overwrites stale
  // delta payloads (e.g. from a partial server response).
  const filtered = current.filter((r) => r.id !== row.id);
  const next = [...filtered, row];
  // Drop from the FRONT (oldest first) when over the cap. `.slice(-MAX_ROWS)`
  // keeps the trailing window, matching verdict-storage's eviction policy.
  const trimmed = next.length > MAX_ROWS ? next.slice(-MAX_ROWS) : next;
  await Browser.storage.local.set({ [STORAGE_KEY]: trimmed });
  return row;
}

export async function getStateDelta(id: string): Promise<StateDeltaRow | null> {
  const rows = await listAllStateDeltas();
  return rows.find((r) => r.id === id) ?? null;
}

export async function clearStateDeltas(): Promise<void> {
  await Browser.storage.local.remove(STORAGE_KEY);
}
