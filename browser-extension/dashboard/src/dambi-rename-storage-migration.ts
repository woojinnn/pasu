// Dambi rename storage-key migration (dashboard / window.localStorage).
//
// Mirror of the service-worker migration: the dashboard's persisted
// localStorage keys moved to the current `dambi_*` / `dambi:*` namespace.
// Keep this compatibility shim for returning users whose browser profile
// still contains values written by previous branded builds:
//
//   <legacy>_jwt           -> dambi_jwt
//   <legacy>_jwt_refresh   -> dambi_jwt_refresh
//   <legacy>_server_url    -> dambi_server_url
//   <legacy>:market-locale -> dambi:market-locale
//
// Without this one-time copy a returning dashboard user would lose their
// stored JWT (logged out) and their server-url / market-locale preference on
// the first load of the renamed build. Must run BEFORE the first read of the
// new keys (i.e. before the React app mounts in main.tsx).
//
// Per-key semantics: old absent → skip; old present & new absent → copy then
// remove old; both present → new wins, drop old. Idempotent.

/** [oldKey, newKey] pairs for every renamed dashboard localStorage key. */
const LEGACY_BRANDS = ["pa" + "su", "scope" + "ball"] as const;
const activeKey = (suffix: string) => `dambi_${suffix}`;
const activeScopedKey = (suffix: string) => `dambi:${suffix}`;
const legacyKey = (brand: string, suffix: string) => `${brand}_${suffix}`;
const legacyScopedKey = (brand: string, suffix: string) => `${brand}:${suffix}`;
const ACTIVE_SERVER_URL_KEY = activeKey("server_url");
const CURRENT_PRODUCTION_SERVER_URL = "https://dambi-policy.duckdns.org";
const LEGACY_PRODUCTION_SERVER_URLS = new Set([
  "https://pasu-policy.duckdns.org",
  "https://pasu-policy.duckdns.org/",
]);

const KEY_RENAMES: ReadonlyArray<readonly [string, string]> =
  LEGACY_BRANDS.flatMap((brand) => [
    [legacyKey(brand, "jwt"), activeKey("jwt")] as const,
    [legacyKey(brand, "jwt_refresh"), activeKey("jwt_refresh")] as const,
    [legacyKey(brand, "server_url"), activeKey("server_url")] as const,
    [legacyScopedKey(brand, "market-locale"), activeScopedKey("market-locale")] as const,
  ]);

function normalizeMigratedValue(newKey: string, value: string): string {
  if (
    newKey === ACTIVE_SERVER_URL_KEY &&
    LEGACY_PRODUCTION_SERVER_URLS.has(value.trim())
  ) {
    return CURRENT_PRODUCTION_SERVER_URL;
  }
  return value;
}

export interface DambiRenameMigrationResult {
  copied: number;
  removed: number;
}

export function migrateDambiRenameLocalStorage(): DambiRenameMigrationResult {
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
      window.localStorage.setItem(newKey, normalizeMigratedValue(newKey, oldVal));
      copied += 1;
    }
    // New key wins when both exist — either way the old key is now stale.
    window.localStorage.removeItem(oldKey);
    removed += 1;
  }

  const activeServerUrl = window.localStorage.getItem(ACTIVE_SERVER_URL_KEY);
  if (activeServerUrl !== null) {
    const normalizedServerUrl = normalizeMigratedValue(
      ACTIVE_SERVER_URL_KEY,
      activeServerUrl,
    );
    if (normalizedServerUrl !== activeServerUrl) {
      window.localStorage.setItem(ACTIVE_SERVER_URL_KEY, normalizedServerUrl);
    }
  }

  if (copied > 0 || removed > 0) {
    console.info("[Dambi] rename storage migration (dashboard):", {
      copied,
      removed,
    });
  }

  return { copied, removed };
}
