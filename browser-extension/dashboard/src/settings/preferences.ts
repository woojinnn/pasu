// Dashboard-local user preferences. Kept in localStorage so we never
// touch chrome.storage.local (per project rule: policy data only).
//
// This is a single-key blob — small enough that JSON serialization is
// trivial and easier to evolve than per-key access patterns.

const STORAGE_KEY = "scopeball:dashboard:preferences:v1";

export interface Preferences {
  /** Default chain id pre-filled in Policy Test form. */
  policyTestChainId: number;
  /** Default actor (from) address pre-filled in Policy Test form. */
  policyTestActor: string;
  /** Default `to` address pre-filled in Policy Test form. */
  policyTestTo: string;
  /** When true, sdk-context auto-refreshes catalog on extension change events. */
  autoRefreshOnChange: boolean;
}

export const DEFAULT_PREFERENCES: Preferences = {
  policyTestChainId: 1,
  policyTestActor: "0x0000000000000000000000000000000000000001",
  policyTestTo: "0x0000000000000000000000000000000000000002",
  autoRefreshOnChange: true,
};

export function loadPreferences(): Preferences {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_PREFERENCES;
    const parsed = JSON.parse(raw) as Partial<Preferences>;
    return { ...DEFAULT_PREFERENCES, ...parsed };
  } catch {
    return DEFAULT_PREFERENCES;
  }
}

export function savePreferences(next: Preferences): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    // Notify other tabs / contexts on this origin.
    window.dispatchEvent(new CustomEvent(PREF_EVENT));
  } catch {
    /* localStorage may be unavailable in private mode — degrade silently */
  }
}

const PREF_EVENT = "scopeball:preferences-changed";

// Lightweight subscription so PolicyTestPanel etc. can re-read live.
export function subscribePreferences(cb: () => void): () => void {
  const handler = () => cb();
  window.addEventListener(PREF_EVENT, handler);
  // Also pick up changes from other tabs.
  const storageHandler = (e: StorageEvent) => {
    if (e.key === STORAGE_KEY) cb();
  };
  window.addEventListener("storage", storageHandler);
  return () => {
    window.removeEventListener(PREF_EVENT, handler);
    window.removeEventListener("storage", storageHandler);
  };
}
