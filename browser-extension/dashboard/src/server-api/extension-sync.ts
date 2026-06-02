/**
 * Dashboard ↔ extension SW bridge for managed policies.
 *
 * Server-side `user_policies` storage has been retired (see
 * `policies.ts` stubs); the extension's `chrome.storage.local` is now
 * the single source of truth. This module wraps the SW handlers
 * (`dashboard:put-raw`, `dashboard:delete`, `dashboard:list-managed`)
 * with a slim, Promise-based API that the dashboard pages call
 * instead of HTTP fetches.
 *
 * All calls fail soft when the extension isn't installed
 * (`ExtensionBridgeTimeout`) so the page renders empty state instead
 * of an error wall.
 */

import { sendToExtension, ExtensionBridgeTimeout } from "./extension-bridge";

/** Prefix the SW expects on dashboard-managed policy ids. */
const ID_PREFIX = "dashboard::";

/** A managed policy as the SW exposes it via `dashboard:list-managed`.
 *  Mirror of `ManagedPolicy` in the SW source. */
export interface ManagedPolicy {
  id: string;
  kind: "raw" | "template";
  text: string;
  policyTree?: string;
  displayName?: string;
  manifest?: unknown;
  manifests?: readonly unknown[];
  updatedAtMs: number;
  schemaVersion: 1;
}

/** Convert a small per-policy numeric handle into the SW's expected id.
 *  We use a stable hash of the displayName + random nonce so re-saving
 *  the same policy from another device produces the same id. For
 *  Phase-1 we just use the displayName slug — collisions are surfaced
 *  to the user as overwrite warnings. */
export function dashboardId(idOrName: string | number): string {
  return `${ID_PREFIX}${idOrName}`;
}

/** Strip the `dashboard::` prefix; returns the raw suffix. */
export function stripDashboardId(id: string): string {
  return id.startsWith(ID_PREFIX) ? id.slice(ID_PREFIX.length) : id;
}

export interface PutPolicyOpts {
  id: string;
  cedarText: string;
  policyTree?: string | null;
  displayName?: string;
}

/** Install/update a policy in the extension's local store + wasm engine. */
export async function putPolicy(opts: PutPolicyOpts): Promise<void> {
  try {
    await sendToExtension({
      type: "dashboard:put-raw",
      id: opts.id,
      text: opts.cedarText,
      ...(opts.policyTree != null ? { policyTree: opts.policyTree } : {}),
      ...(opts.displayName ? { displayName: opts.displayName } : {}),
    });
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return; // extension not installed
    throw err;
  }
}

/** Delete a policy from the extension's local store. */
export async function deletePolicy(id: string): Promise<void> {
  try {
    await sendToExtension({ type: "dashboard:delete", id });
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return;
    throw err;
  }
}

/** Read every dashboard-managed policy from the SW. Returns an empty
 *  list when the extension isn't installed. */
export async function listManagedPolicies(): Promise<ManagedPolicy[]> {
  try {
    return await sendToExtension<ManagedPolicy[]>({ type: "dashboard:list-managed" });
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return [];
    throw err;
  }
}
