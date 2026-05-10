import Browser from "webextension-polyfill";
import { rpcClient } from "./chains/rpc-client";
import { commitByTxHash, discardExpired, listPending } from "./pending-deltas";

const ALARM = "scopeball:receipt-poll";

export function installReceiptPoller(): void {
  // Wrapped in try/catch so a failed alarm registration (e.g. invalid
  // argument on older Chrome where periodInMinutes < 1 is rejected) can
  // not kill SW startup before the message handlers register.
  try {
    Browser.alarms.create(ALARM, { periodInMinutes: 1 });
    Browser.alarms.onAlarm.addListener((alarm) => {
      if (alarm.name !== ALARM) return;
      void poll();
    });
  } catch (err) {
    console.warn('[Scopeball] receipt-poller alarm registration failed:', err);
  }
}

async function poll(): Promise<void> {
  await discardExpired();
  const pending = await listPending();
  for (const entry of pending) {
    if (!entry.txHash) continue;
    try {
      const client = rpcClient(entry.chainId);
      const receipt = await client.getTransactionReceipt({
        hash: entry.txHash as `0x${string}`,
      });
      if (receipt && receipt.status === "success") {
        await commitByTxHash(entry.txHash, {
          chainId: entry.chainId,
          actor: entry.actor,
          windowEntries: entry.windowEntries,
        });
      }
      // null receipt → still mining; leave the entry in place. Expired
      // entries get swept by discardExpired on the next tick.
    } catch {
      // RPC failure: ignore; next poll retries.
    }
  }
}
