/**
 * Runtime environment detection.
 *
 * The same React bundle ships in two hosts:
 *   - Vite dev server (http://127.0.0.1:5173) — full HMR for everyday work.
 *   - The extension's options page (chrome-extension://<id>/options.html)
 *     — what users actually open from `chrome://extensions`.
 *
 * Anywhere the two environments diverge (router type, auth flow, asset
 * paths) we branch on `isExtensionContext()` instead of sniffing
 * `window.location` ad-hoc. Keep the check here so the contract stays
 * in one place.
 */

export function isExtensionContext(): boolean {
  if (typeof window === "undefined") return false;
  return window.location.protocol === "chrome-extension:";
}
