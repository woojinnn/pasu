/**
 * Dual-write + initial-sync bridge between simulation-server DB and the
 * extension's `chrome.storage.local` (SW dashboard handlers).
 *
 * The server DB is authoritative — `user_policies` rows persist across
 * sessions and devices. The extension storage is a per-browser working
 * copy that the wasm cedar engine actually evaluates against. Keeping
 * them in step requires two pieces:
 *
 *   1. **Dual-write**: every successful CRUD against the server fires a
 *      matching SW message so the local engine reflects the change
 *      immediately, without waiting for a page reload.
 *   2. **Initial sync** on dashboard load — pushes the server's list
 *      into the SW so policies created on another device / from a tool
 *      that bypassed the UI still show up in the local popup.
 *
 * Errors are non-fatal: if the extension isn't installed (timeout) or
 * a single put fails (bad cedar), we log + continue. The DB stays the
 * source of truth; the popup just falls a beat behind until next sync.
 */

import { sendToExtension, ExtensionBridgeTimeout } from "./extension-bridge";
import type { InstalledPolicy } from "./types";

/** Prefix the SW expects on dashboard-managed policy ids. */
const ID_PREFIX = "dashboard::";

export function dashboardId(serverPolicyId: number): string {
  return `${ID_PREFIX}${serverPolicyId}`;
}

/** Install or update a single policy in the extension storage + wasm engine. */
export async function installPolicyToExtension(
  serverPolicyId: number,
  cedarText: string,
): Promise<void> {
  try {
    await sendToExtension({
      type: "dashboard:put-raw",
      id: dashboardId(serverPolicyId),
      text: cedarText,
    });
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return; // extension not installed
    // SW returned an error (likely bad cedar). Log but don't throw —
    // the server already accepted the policy; popup will lag behind.
    // eslint-disable-next-line no-console
    console.warn("[extension-sync] put-raw failed:", err);
  }
}

/** Remove a policy from extension storage. */
export async function removePolicyFromExtension(
  serverPolicyId: number,
): Promise<void> {
  try {
    await sendToExtension({
      type: "dashboard:delete",
      id: dashboardId(serverPolicyId),
    });
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return;
    // eslint-disable-next-line no-console
    console.warn("[extension-sync] delete failed:", err);
  }
}

/**
 * Push every server-side policy into the extension. Called on dashboard
 * load + whenever the policy list refetches. Safe to call repeatedly —
 * `put-raw` is idempotent and the SW dedupes by id.
 *
 * Returns silently when the extension isn't installed.
 */
export async function syncAllPoliciesToExtension(
  policies: readonly InstalledPolicy[],
): Promise<void> {
  // Single failed bridge call is enough to know the extension isn't
  // there — bail early so we don't fan-out N timeouts.
  try {
    await sendToExtension({ type: "dashboard:ping" }, 1_500);
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return;
    // Some other error — fall through and try the actual puts.
  }
  // Fire-and-forget puts. We don't await all to keep the page snappy;
  // any individual failure is non-fatal.
  await Promise.allSettled(
    policies
      .filter((p) => p.enabled)
      .map((p) => installPolicyToExtension(p.id, p.cedar_text)),
  );
}
