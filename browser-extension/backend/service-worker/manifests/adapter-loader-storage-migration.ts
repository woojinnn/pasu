// Adapter-loader storage key migration.
//
// The `marketplace/` directory was renamed to `adapter-loader/` to
// disambiguate it from the (unimplemented) policy-preset marketplace
// described in the UPSide 4기 중간발표 PDF (p11/p12/p21). chrome.storage
// key `"marketplace:bundles"` (the installed adapter bundle cache) was
// renamed to `"adapter-loader:bundles"` at the same time. This module
// handles the one-time copy from the old key to the new key on SW boot
// so existing users do not lose their cached bundles.
//
// Semantics:
//   - If old key is absent → no-op (fresh install OR already migrated
//     and old key already cleaned up).
//   - If old key has value AND new key is absent → copy old → new,
//     then delete old.
//   - If both keys have values → new key wins (must have been re-installed
//     after the user already migrated on another device); drop old.
//
// Idempotent: safe to run on every boot. Each boot at most one
// `set` + `remove` after the initial migration window.

import Browser from "webextension-polyfill";

const OLD_KEY = "marketplace:bundles";
const NEW_KEY = "adapter-loader:bundles";

export type AdapterLoaderMigrationReason =
  | "no_old_data"
  | "copied"
  | "new_exists";

export interface AdapterLoaderMigrationResult {
  migrated: boolean;
  reason: AdapterLoaderMigrationReason;
}

export async function migrateAdapterLoaderStorageKey(): Promise<AdapterLoaderMigrationResult> {
  const both = await Browser.storage.local.get([OLD_KEY, NEW_KEY]);
  const oldVal = both[OLD_KEY];
  const newVal = both[NEW_KEY];

  if (oldVal === undefined) {
    return { migrated: false, reason: "no_old_data" };
  }

  if (newVal === undefined) {
    await Browser.storage.local.set({ [NEW_KEY]: oldVal });
    await Browser.storage.local.remove(OLD_KEY);
    console.info("[Dambi] adapter-loader storage migration: copied", {
      bundleCount: Array.isArray(oldVal) ? oldVal.length : "unknown",
    });
    return { migrated: true, reason: "copied" };
  }

  // Both exist — new key wins; safe to drop old.
  await Browser.storage.local.remove(OLD_KEY);
  console.info(
    "[Dambi] adapter-loader storage migration: new key exists, dropping old",
  );
  return { migrated: false, reason: "new_exists" };
}
