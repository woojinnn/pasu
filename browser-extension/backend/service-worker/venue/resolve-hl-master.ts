/**
 * Resolve the HL master account address whose per-asset leverage applies to a
 * venue order, for the `activeAssetData` lookup.
 *
 * Priority:
 *   1. `payload.vaultAddress` — when the order is placed on behalf of a vault /
 *      subaccount, THAT account's leverage is what the venue applies.
 *   2. `payload.wallet_id.address` — the connected EVM account the fetch-hook
 *      read from `window.ethereum` (`eth_accounts`) and stamped on the payload
 *      (the master for normal, non-vault trading).
 *   3. The per-origin connected account in {@link getConnectedAccount} — a
 *      manually-seeded / future-captured fallback when the payload carries none.
 *   4. `null` — unknown. The caller then omits leverage (best-effort), leaving
 *      a `context has leverage` policy dormant rather than over-blocking.
 */
import type { VenueOrderPayload } from "@lib/types";
import { getConnectedAccount } from "./hl-master-store";

const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/i;

function validAddr(value: unknown): string | null {
  return typeof value === "string" && ADDRESS_RE.test(value)
    ? value.toLowerCase()
    : null;
}

export async function resolveHlMaster(
  payload: VenueOrderPayload,
): Promise<string | null> {
  return (
    validAddr(payload.vaultAddress) ??
    validAddr(payload.wallet_id?.address) ??
    (await getConnectedAccount(payload.hostname))
  );
}
