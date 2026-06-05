// Pasu rename storage-key migration (service-worker / chrome.storage.local).
//
// The product was renamed scopeball → pasu. The persisted chrome.storage.local
// keys the SW owns were renamed at the same time:
//
//   scopeball_jwt           → pasu_jwt
//   scopeball_jwt_refresh   → pasu_jwt_refresh
//   scopeball_server_url    → pasu_server_url
//   scopeball_diag_timeouts → pasu_diag_timeouts
//
// Without this one-time copy, existing users would be silently logged out
// (lost JWT) and would lose their chosen server URL on first boot of the
// renamed build. This module copies each old key to its new key on SW boot,
// before any code reads the new keys.
//
// Per-key semantics (modelled on `migrateAdapterLoaderStorageKey`):
//   - old absent                → no-op for that key (fresh install OR already
//                                 migrated and cleaned up).
//   - old present, new absent    → copy old → new, then delete old.
//   - both present               → new key wins; drop old.
//
// Idempotent: safe to run on every boot. After the initial migration window
// each boot does at most one `remove` (and no `set`) per key.

import Browser from "webextension-polyfill";

/** [oldKey, newKey] pairs for every renamed SW chrome.storage.local key. */
const KEY_RENAMES: ReadonlyArray<readonly [string, string]> = [
  ["scopeball_jwt", "pasu_jwt"],
  ["scopeball_jwt_refresh", "pasu_jwt_refresh"],
  ["scopeball_server_url", "pasu_server_url"],
  ["scopeball_diag_timeouts", "pasu_diag_timeouts"],
];

export interface PasuRenameMigrationResult {
  /** Number of keys whose value was copied from the old key to the new key. */
  copied: number;
  /** Number of stale old keys removed (copied + dropped-because-new-wins). */
  removed: number;
}

export async function migratePasuRenameStorageKeys(): Promise<PasuRenameMigrationResult> {
  const allKeys = KEY_RENAMES.flatMap(([oldKey, newKey]) => [oldKey, newKey]);
  const store = await Browser.storage.local.get(allKeys);

  let copied = 0;
  let removed = 0;

  for (const [oldKey, newKey] of KEY_RENAMES) {
    const oldVal = store[oldKey];
    if (oldVal === undefined) continue; // nothing to migrate for this key

    if (store[newKey] === undefined) {
      await Browser.storage.local.set({ [newKey]: oldVal });
      copied += 1;
    }
    // New key wins when both exist — either way the old key is now stale.
    await Browser.storage.local.remove(oldKey);
    removed += 1;
  }

  if (copied > 0 || removed > 0) {
    console.info("[Pasu] rename storage migration (SW):", { copied, removed });
  }

  return { copied, removed };
}
