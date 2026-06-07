// Pasu rename storage-key migration (dashboard / window.localStorage).
//
// Mirror of the service-worker migration: the product was renamed
// scopeball → pasu and the dashboard's persisted localStorage keys moved:
//
//   scopeball_jwt          → pasu_jwt
//   scopeball_jwt_refresh  → pasu_jwt_refresh
//   scopeball_server_url   → pasu_server_url
//   scopeball:market-locale → pasu:market-locale
//
// Without this one-time copy a returning dashboard user would lose their
// stored JWT (logged out) and their server-url / market-locale preference on
// the first load of the renamed build. Must run BEFORE the first read of the
// new keys (i.e. before the React app mounts in main.tsx).
//
// Per-key semantics: old absent → skip; old present & new absent → copy then
// remove old; both present → new wins, drop old. Idempotent.

/** [oldKey, newKey] pairs for every renamed dashboard localStorage key. */
const KEY_RENAMES: ReadonlyArray<readonly [string, string]> = [
  ["scopeball_jwt", "pasu_jwt"],
  ["scopeball_jwt_refresh", "pasu_jwt_refresh"],
  ["scopeball_server_url", "pasu_server_url"],
  ["scopeball:market-locale", "pasu:market-locale"],
];

export interface PasuRenameMigrationResult {
  copied: number;
  removed: number;
}

export function migratePasuRenameLocalStorage(): PasuRenameMigrationResult {
  let copied = 0;
  let removed = 0;

  // Guard for non-browser / extension contexts where localStorage is absent.
  if (typeof window === "undefined" || !window.localStorage) {
    return { copied, removed };
  }

  for (const [oldKey, newKey] of KEY_RENAMES) {
    const oldVal = window.localStorage.getItem(oldKey);
    if (oldVal === null) continue; // nothing to migrate for this key

    if (window.localStorage.getItem(newKey) === null) {
      window.localStorage.setItem(newKey, oldVal);
      copied += 1;
    }
    // New key wins when both exist — either way the old key is now stale.
    window.localStorage.removeItem(oldKey);
    removed += 1;
  }

  if (copied > 0 || removed > 0) {
    console.info("[Pasu] rename storage migration (dashboard):", {
      copied,
      removed,
    });
  }

  return { copied, removed };
}
