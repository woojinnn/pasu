/**
 * Extension-page bootstrap: bridge the service worker's JWT into this page.
 *
 * When the dashboard runs as the options page (`chrome-extension://…`), it
 * has its own `localStorage`, separate from the SW's `chrome.storage.local`
 * where the JWT lives. We copy the SW-owned token into `localStorage` BEFORE
 * the first render, so the existing localStorage-based server-api client +
 * `useAuth` authenticate with no further changes — one token injection then
 * authenticates BOTH the SW (tx eval) and this dashboard.
 *
 * In the standalone dev build (localhost:5173) this is a no-op. It never
 * throws — any failure degrades to "no token synced → /login", never a
 * blank page.
 */
import { isExtensionContext } from "./env";

/** Keys mirrored from the SW's tokenStore (`chrome.storage.local`). */
const TOKEN_KEYS = ["scopeball_jwt", "scopeball_jwt_refresh"] as const;

type StorageChange = { newValue?: unknown };
type ChromeStorage = {
  local?: { get(keys: string[]): Promise<Record<string, unknown>> };
  onChanged?: {
    addListener(
      cb: (changes: Record<string, StorageChange>, area: string) => void,
    ): void;
  };
};

/** `chrome.storage`, only present on an extension page. Typed via the same
 * defensive cast SettingsPage uses (the dashboard workspace has no
 * `@types/chrome`). */
function extStorage(): ChromeStorage | undefined {
  return (globalThis as { chrome?: { storage?: ChromeStorage } }).chrome
    ?.storage;
}

/** Copy the SW-owned JWT (chrome.storage.local) into this page's localStorage
 * so the localStorage-based server-api client + useAuth pick it up. No-op
 * outside the extension; never throws. Called at boot AND right after an
 * in-extension SW sign-in so the dashboard authenticates without a reload. */
export async function syncTokensFromExtensionStorage(): Promise<void> {
  if (!isExtensionContext()) return;
  try {
    const storage = extStorage();
    if (!storage?.local) return;
    const got = await storage.local.get([...TOKEN_KEYS]);
    for (const k of TOKEN_KEYS) {
      const v = got[k];
      if (typeof v === "string" && v) localStorage.setItem(k, v);
    }
  } catch {
    // Degrade to "no token synced → /login"; never block the render.
  }
}

export async function bootstrapExtensionEnv(): Promise<void> {
  if (!isExtensionContext()) return;
  await syncTokensFromExtensionStorage();
  try {
    const storage = extStorage();
    // Keep localStorage in sync if the SW refreshes / clears the token, and
    // nudge useAuth's `storage` listener (same-tab setItem doesn't fire it).
    storage?.onChanged?.addListener((changes, area) => {
      if (area !== "local") return;
      for (const k of TOKEN_KEYS) {
        if (!(k in changes)) continue;
        const v = changes[k]?.newValue;
        if (typeof v === "string" && v) localStorage.setItem(k, v);
        else localStorage.removeItem(k);
        window.dispatchEvent(new StorageEvent("storage", { key: k }));
      }
    });
  } catch {
    // Listener setup best-effort.
  }
}
