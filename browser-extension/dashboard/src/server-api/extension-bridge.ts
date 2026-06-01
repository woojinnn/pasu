/**
 * `extension-bridge` — page-side counterpart to
 * `browser-extension/backend/content-scripts/dashboard-bridge.ts`.
 *
 * Sends one request through `window.postMessage` (tagged
 * `source: "scopeball-dashboard"`), waits for the matching response (tagged
 * `source: "scopeball-extension"` with the same `id`), and resolves with the
 * SW handler's `{ ok, data | error }` envelope.
 *
 * Why a thin module?
 * - Verdict / execution-report storage lives outside the
 *   policy-server and into `chrome.storage.local`. The dashboard talks to
 *   the SW for those reads — the bridge is the only available channel
 *   (`chrome.runtime.sendMessage` isn't reachable from a regular web page).
 * - Keeps the dashboard's React Query hooks unchanged: they still call
 *   `listAuditVerdicts(opts)` and get back a typed array; the implementation
 *   just changed from `fetch("/audit/verdicts?…")` to
 *   `sendToExtension({ type: "verdicts:list", opts })`.
 *
 * Protocol (matches `dashboard-bridge.ts`):
 *   request  → `{ source: "scopeball-dashboard", id, payload }`
 *   response ← `{ source: "scopeball-extension",  id, response }`
 *   broadcast ← `{ source: "scopeball-extension", id: "__broadcast__", … }`
 *
 * Broadcasts are filtered out; only matching `id`s resolve a pending call.
 */

const REQ_TAG = "scopeball-dashboard" as const;
const RES_TAG = "scopeball-extension" as const;
const BROADCAST_ID = "__broadcast__";

/** Surfaced when the SW returns `{ ok: false, error }`. */
export class ExtensionBridgeError extends Error {
  public readonly kind: string;
  constructor(kind: string, message: string) {
    super(message);
    this.name = "ExtensionBridgeError";
    this.kind = kind;
  }
}

/** Surfaced when the bridge times out — usually means the content script isn't
 *  injected (extension not installed / dashboard not on the matches origin). */
export class ExtensionBridgeTimeout extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ExtensionBridgeTimeout";
  }
}

interface BridgeResponseEnvelope {
  source: typeof RES_TAG;
  id: string;
  response: unknown;
}

function isBridgeResponse(value: unknown): value is BridgeResponseEnvelope {
  if (!value || typeof value !== "object") return false;
  const o = value as Record<string, unknown>;
  return o.source === RES_TAG && typeof o.id === "string" && "response" in o;
}

/** Default 10s. The SW evaluates locally so calls usually return in <50ms;
 *  the timeout exists to surface a missing extension, not slow handlers. */
const DEFAULT_TIMEOUT_MS = 10_000;

/**
 * Send `payload` to the SW, wait for the response.
 *
 * Resolves with `data` when the SW returns `{ ok: true, data }`.
 * Rejects with `ExtensionBridgeError` when the SW returns `{ ok: false, error }`.
 * Rejects with `ExtensionBridgeTimeout` if no response within `timeoutMs`.
 *
 * @param payload SW message payload (must include `type`)
 * @param timeoutMs override the default deadline
 */
export async function sendToExtension<T>(
  payload: unknown,
  timeoutMs: number = DEFAULT_TIMEOUT_MS,
): Promise<T> {
  // crypto.randomUUID() is in the browser baseline since 2022 — the dashboard
  // already targets modern Chrome (extension MV3). No fallback needed.
  const id = crypto.randomUUID();

  return new Promise<T>((resolve, reject) => {
    let settled = false;
    const onMessage = (event: MessageEvent): void => {
      if (event.source !== window) return;
      // The bridge always replies on `location.origin`; tighten the gate so a
      // foreign iframe can't impersonate a response.
      if (event.origin !== window.location.origin) return;
      if (!isBridgeResponse(event.data)) return;
      // Storage-change broadcasts share the response tag; ignore them.
      if (event.data.id === BROADCAST_ID) return;
      if (event.data.id !== id) return;
      settle();
      const r = event.data.response as
        | { ok: true; data: T }
        | { ok: false; error: { kind?: string; message?: string } }
        | undefined;
      if (!r) {
        reject(
          new ExtensionBridgeError(
            "bridge_failed",
            "empty response from extension",
          ),
        );
        return;
      }
      if (r.ok) {
        resolve(r.data);
      } else {
        reject(
          new ExtensionBridgeError(
            r.error?.kind ?? "bridge_failed",
            r.error?.message ?? "extension returned error",
          ),
        );
      }
    };
    const timer = window.setTimeout(() => {
      if (settled) return;
      settle();
      reject(
        new ExtensionBridgeTimeout(
          `extension did not respond within ${timeoutMs}ms`,
        ),
      );
    }, timeoutMs);
    const settle = (): void => {
      if (settled) return;
      settled = true;
      window.clearTimeout(timer);
      window.removeEventListener("message", onMessage);
    };
    window.addEventListener("message", onMessage);
    window.postMessage(
      { source: REQ_TAG, id, payload },
      window.location.origin,
    );
  });
}
