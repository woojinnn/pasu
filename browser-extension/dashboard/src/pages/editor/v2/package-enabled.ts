/**
 * Per-package on/off bit, stored dashboard-side.
 *
 * A package's on/off is a *dashboard-only* concept: enforcement only cares
 * about the enabled-policy-ids set (which the SW persists). So we keep each
 * package's explicit switch in localStorage rather than round-tripping a new
 * field through the service worker — it works the moment the page reloads, no
 * extension rebuild required.
 *
 * Absent entry = "no explicit choice yet"; callers fall back to deriving the
 * state from member enabled bits. Once the user toggles a package, its bit is
 * remembered so "off, but shared policies stay on" survives reloads.
 */

const KEY = "pasu:pkg-enabled";

export type PkgBits = Record<string, boolean>;

export function loadPkgBits(): PkgBits {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as unknown;
    if (parsed && typeof parsed === "object") return parsed as PkgBits;
  } catch {
    /* corrupt / unavailable storage → empty */
  }
  return {};
}

export function savePkgBits(bits: PkgBits): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(bits));
  } catch {
    /* storage full / unavailable → best-effort, state still lives in memory */
  }
}
