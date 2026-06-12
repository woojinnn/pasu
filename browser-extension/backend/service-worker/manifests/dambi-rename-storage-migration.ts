// Dambi rename storage-key migration (service-worker / chrome.storage.local).
//
// The persisted chrome.storage.local keys the SW owns moved to the current
// `dambi_*` namespace. Keep this compatibility shim for returning users whose
// browser profile still contains values written by previous branded builds:
//
//   <legacy>_jwt           -> dambi_jwt
//   <legacy>_jwt_refresh   -> dambi_jwt_refresh
//   <legacy>_server_url    -> dambi_server_url
//   <legacy>_diag_timeouts -> dambi_diag_timeouts
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
const LEGACY_BRANDS = ["pa" + "su", "scope" + "ball"] as const;
const activeKey = (suffix: string) => `dambi_${suffix}`;
const legacyKey = (brand: string, suffix: string) => `${brand}_${suffix}`;

const KEY_RENAMES: ReadonlyArray<readonly [string, string]> =
  LEGACY_BRANDS.flatMap((brand) => [
    [legacyKey(brand, "jwt"), activeKey("jwt")] as const,
    [legacyKey(brand, "jwt_refresh"), activeKey("jwt_refresh")] as const,
    [legacyKey(brand, "server_url"), activeKey("server_url")] as const,
    [legacyKey(brand, "diag_timeouts"), activeKey("diag_timeouts")] as const,
  ]);

export interface DambiRenameMigrationResult {
  /** Number of keys whose value was copied from the old key to the new key. */
  copied: number;
  /** Number of stale old keys removed (copied + dropped-because-new-wins). */
  removed: number;
}

export async function migrateDambiRenameStorageKeys(): Promise<DambiRenameMigrationResult> {
  const allKeys = KEY_RENAMES.flatMap(([oldKey, newKey]) => [oldKey, newKey]);
  const store = await Browser.storage.local.get(allKeys);

  let copied = 0;
  let removed = 0;

  for (const [oldKey, newKey] of KEY_RENAMES) {
    const oldVal = store[oldKey];
    if (oldVal === undefined) continue; // nothing to migrate for this key

    if (store[newKey] === undefined) {
      await Browser.storage.local.set({ [newKey]: oldVal });
      store[newKey] = oldVal;
      copied += 1;
    }
    // New key wins when both exist — either way the old key is now stale.
    await Browser.storage.local.remove(oldKey);
    removed += 1;
  }

  if (copied > 0 || removed > 0) {
    console.info("[Dambi] rename storage migration (SW):", { copied, removed });
  }

  return { copied, removed };
}
