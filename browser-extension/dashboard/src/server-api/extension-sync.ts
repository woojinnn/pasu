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

/** Lifecycle stage. `draft` hides the policy from the enforced set
 *  when the SW draft-gate ships; `publish` is the legacy default. */
export type PolicyLife = "draft" | "publish";

/** Provenance. `mine` = user-authored; `market` = installed from the
 *  marketplace (carries listing/version stamps for update detection). */
export type PolicySource = "mine" | "market";

/** Authoring surface chosen on create. Drives the editor's default tab. */
export type PolicyMethod = "form" | "block" | "cedar";

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
  life?: PolicyLife;
  source?: PolicySource;
  cat?: string;
  method?: PolicyMethod;
  dupKey?: string;
  memo?: string;
  sourceListingId?: string;
  sourceVersion?: string;
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
  /** Manifest JSON to persist alongside the policy. Seeded bundles need this
   *  so the v2 loader can compose their `policy_rpc` + `custom_context`
   *  schema; user-authored policies omit it and the loader falls back to a
   *  synthesized minimal manifest. */
  manifest?: unknown;
  life?: PolicyLife;
  source?: PolicySource;
  cat?: string;
  method?: PolicyMethod;
  dupKey?: string;
  memo?: string;
  sourceListingId?: string;
  sourceVersion?: string;
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
      ...(opts.manifest !== undefined ? { manifest: opts.manifest } : {}),
      ...(opts.life ? { life: opts.life } : {}),
      ...(opts.source ? { source: opts.source } : {}),
      ...(opts.cat ? { cat: opts.cat } : {}),
      ...(opts.method ? { method: opts.method } : {}),
      ...(opts.dupKey ? { dupKey: opts.dupKey } : {}),
      ...(opts.memo !== undefined ? { memo: opts.memo } : {}),
      ...(opts.sourceListingId ? { sourceListingId: opts.sourceListingId } : {}),
      ...(opts.sourceVersion ? { sourceVersion: opts.sourceVersion } : {}),
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

/** Storage key the SW writes when the popup or dashboard toggles a
 *  policy on/off. Exposed so the dashboard can scope its React Query
 *  invalidation to broadcasts that touch this key. */
export const ENABLED_IDS_STORAGE_KEY = "policy-selection:enabled-ids";

/** Read the set of enabled policy ids (the same set the popup's
 *  checkbox column mutates). Returns `[]` when the extension isn't
 *  installed so callers can treat "no extension" as "nothing enabled". */
export async function getEnabledPolicyIds(): Promise<string[]> {
  try {
    return await sendToExtension<string[]>({ type: "policy-selection:get" });
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return [];
    throw err;
  }
}

/** Replace the enabled-policy set. Sends the full desired list (the
 *  SW handler is a setter, not a toggle) so the caller must compute
 *  `next = current.with/without(id)` before calling. */
export async function setEnabledPolicyIds(ids: string[]): Promise<void> {
  try {
    await sendToExtension({ type: "set-enabled-ids", ids });
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return;
    throw err;
  }
}

/** Prefix the SW expects on dashboard-managed set ids. Distinct from
 *  the policy prefix so a single id space can't conflate the two. */
const SET_ID_PREFIX = "dashboard-set::";

/** A user-defined policy set: a named group of policy ids that can be
 *  toggled on/off together. Many-to-many — a single policy id can appear
 *  in multiple sets' memberIds. */
export interface PolicySet {
  id: string;
  displayName: string;
  description?: string;
  memberIds: readonly string[];
  source?: PolicySource;
  readOnly?: boolean;
  cat?: string;
  sourceListingId?: string;
  sourceVersion?: string;
  updatedAtMs: number;
  schemaVersion: 1;
}

export function dashboardSetId(idOrName: string | number): string {
  return `${SET_ID_PREFIX}${idOrName}`;
}

export function stripDashboardSetId(id: string): string {
  return id.startsWith(SET_ID_PREFIX) ? id.slice(SET_ID_PREFIX.length) : id;
}

export async function listPolicySets(): Promise<PolicySet[]> {
  try {
    return await sendToExtension<PolicySet[]>({ type: "dashboard:list-sets" });
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return [];
    throw err;
  }
}

export interface PutPolicySetOpts {
  id: string;
  displayName: string;
  description?: string;
  memberIds: readonly string[];
  source?: PolicySource;
  readOnly?: boolean;
  cat?: string;
  sourceListingId?: string;
  sourceVersion?: string;
}

export async function putPolicySet(opts: PutPolicySetOpts): Promise<void> {
  try {
    await sendToExtension({
      type: "dashboard:put-set",
      id: opts.id,
      displayName: opts.displayName,
      memberIds: opts.memberIds,
      ...(opts.description != null ? { description: opts.description } : {}),
      ...(opts.source ? { source: opts.source } : {}),
      ...(opts.readOnly !== undefined ? { readOnly: opts.readOnly } : {}),
      ...(opts.cat ? { cat: opts.cat } : {}),
      ...(opts.sourceListingId ? { sourceListingId: opts.sourceListingId } : {}),
      ...(opts.sourceVersion ? { sourceVersion: opts.sourceVersion } : {}),
    });
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return;
    throw err;
  }
}

export async function deletePolicySet(id: string): Promise<void> {
  try {
    await sendToExtension({ type: "dashboard:delete-set", id });
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return;
    throw err;
  }
}

/**
 * Tell the SW which user is now active. The SW uses this id to namespace
 * every per-user storage key (`dashboard:policies:<id>`,
 * `policy-selection:enabled-ids:<id>`, …) so a different account on the
 * same Chrome profile sees a disjoint policy space. Call this after a
 * successful `fetchMe()`. Idempotent — passing the same id is a no-op.
 */
export async function setCurrentUser(userId: string): Promise<void> {
  try {
    await sendToExtension({ type: "dashboard:set-current-user", userId });
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return;
    throw err;
  }
}

/**
 * Drop the active-user discriminator. After this the SW behaves as if no
 * user is logged in: managed-policy reads return `[]`, writes fail with
 * `no_user`, and only baked default policies stay enforced. Call from
 * the dashboard's logout path.
 */
export async function clearCurrentUser(): Promise<void> {
  try {
    await sendToExtension({ type: "dashboard:clear-current-user" });
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return;
    throw err;
  }
}

/** Read whatever current-user id the SW currently has stored. Useful for
 *  bootstrap parity checks (dashboard fetched `Me`, does the SW agree?). */
export async function getCurrentUser(): Promise<string | null> {
  try {
    const data = await sendToExtension<{ userId: string | null }>({
      type: "dashboard:get-current-user",
    });
    return data?.userId ?? null;
  } catch (err) {
    if (err instanceof ExtensionBridgeTimeout) return null;
    throw err;
  }
}
