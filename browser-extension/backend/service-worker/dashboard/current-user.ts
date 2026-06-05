/**
 * Current-user id storage — the discriminator that namespaces every other
 * per-user dashboard key. The dashboard writes this on successful login
 * (via `dashboard:set-current-user`) and clears it on logout (via
 * `dashboard:clear-current-user`). All other storage modules
 * (`dashboard/storage`, `dashboard/sets-storage`, `policy-selection`)
 * read this and key their reads/writes under `<base-key>:<userId>`.
 *
 * The value is intentionally a plain string (no schema/version) — it
 * mirrors a stable id minted by the policy-server (`Me.user_id`) and is
 * never touched by user input. When no value is present, the SW treats
 * the extension as "logged out": dashboard storage reads return [], and
 * write attempts surface a `no_user` error. Baked default policies
 * (`default-policies/policy-set-v2.json`) keep applying regardless.
 */

import Browser from "webextension-polyfill";

export const CURRENT_USER_STORAGE_KEY = "dashboard:current-user-id";

export async function getCurrentUserId(): Promise<string | null> {
  const raw = (await Browser.storage.local.get(CURRENT_USER_STORAGE_KEY)) as Record<
    string,
    unknown
  >;
  const v = raw[CURRENT_USER_STORAGE_KEY];
  return typeof v === "string" && v.length > 0 ? v : null;
}

export async function setCurrentUserId(id: string): Promise<void> {
  if (typeof id !== "string" || id.length === 0) {
    throw new Error("invalid_user: id must be a non-empty string");
  }
  await Browser.storage.local.set({ [CURRENT_USER_STORAGE_KEY]: id });
}

export async function clearCurrentUserId(): Promise<void> {
  await Browser.storage.local.remove(CURRENT_USER_STORAGE_KEY);
}
