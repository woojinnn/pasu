/**
 * Dashboard ↔ SW bridge for the state-delta log
 * (`state-delta-storage.ts`). Each verdict row in the HistoryPage carries
 * a `delta_id` (UUID = `message.requestId` at decision time); calling
 * `getStateDeltaRow(delta_id)` returns the captured reducer-side delta
 * plus the originating tx fields (chain / from / to / calldata / value)
 * so the same payload can power both the diff renderer and the
 * "다시 시뮬" button.
 *
 * Returns `null` for missing ids (legacy verdict rows persisted before
 * the schema migration, or decisions whose `recordSimulationOnServer`
 * couldn't reach the policy-server). The bridge fails soft when the
 * extension isn't installed — same pattern as the other extension-sync
 * helpers.
 */

import { sendToExtension, ExtensionBridgeTimeout } from "./extension-bridge";

/** Mirror of the SW's `StateDeltaRow`. `delta` is kept opaque — the
 *  dashboard projects it with `parseStateDelta` at render time. */
export interface StateDeltaRow {
  id: string;
  ts: number;
  chain: string;
  from: string;
  to: string;
  calldata: string;
  value: string;
  delta: unknown;
}

export async function getStateDeltaRow(
  id: string,
): Promise<StateDeltaRow | null> {
  try {
    return await sendToExtension<StateDeltaRow | null>({
      type: "state-deltas:get",
      id,
    });
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return null;
    throw err;
  }
}

export async function clearStateDeltas(): Promise<void> {
  try {
    await sendToExtension({ type: "state-deltas:clear" });
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return;
    throw err;
  }
}
