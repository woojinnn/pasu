/**
 * `extension-bridge` — page-side counterpart to
 * `browser-extension/backend/content-scripts/dashboard-bridge.ts`.
 *
 * Sends one request to the extension service worker and resolves with the SW
 * handler's `{ ok, data | error }` envelope.
 *
 * Why a thin module?
 * - Verdict / execution-report storage lives outside the policy-server and
 *   into `chrome.storage.local`. The dashboard talks to the SW for those
 *   reads. Extension pages can call `chrome.runtime.sendMessage` directly;
 *   localhost dev pages need the content-script `window.postMessage` bridge.
 * - Keeps the dashboard's React Query hooks unchanged: they still call
 *   `listAuditVerdicts(opts)` and get back a typed array; the implementation
 *   just changed from `fetch("/audit/verdicts?…")` to
 *   `sendToExtension({ type: "verdicts:list", opts })`.
 *
 * Protocol (matches `dashboard-bridge.ts`):
 *   request  → `{ source: "pasu-dashboard", id, payload }`
 *   response ← `{ source: "pasu-extension",  id, response }`
 *   broadcast ← `{ source: "pasu-extension", id: "__broadcast__", … }`
 *
 * Broadcasts are filtered out; only matching `id`s resolve a pending call.
 */

const REQ_TAG = "pasu-dashboard" as const;
const RES_TAG = "pasu-extension" as const;
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

type SwResponse<T> =
  | { ok: true; data: T }
  | { ok: false; error?: { kind?: string; message?: string } };

interface ChromeRuntimeShim {
  runtime?: {
    sendMessage(message: unknown): Promise<unknown>;
  };
}

function isBridgeResponse(value: unknown): value is BridgeResponseEnvelope {
  if (!value || typeof value !== "object") return false;
  const o = value as Record<string, unknown>;
  return o.source === RES_TAG && typeof o.id === "string" && "response" in o;
}

function getRuntime(): ChromeRuntimeShim["runtime"] | null {
  // Only the extension's OWN pages (chrome-extension://, e.g. the bundled
  // options.html) may call `chrome.runtime.sendMessage(message)` directly. On
  // http(s) dev pages (the Vite server at :5173) `chrome.runtime` can still be
  // exposed, but a 1-arg sendMessage there throws ("must specify an Extension
  // ID") — those pages MUST go through the content-script window.postMessage
  // bridge below. Gate on the page origin, not just sendMessage's presence.
  if (globalThis.location?.protocol !== "chrome-extension:") return null;
  const chrome = (globalThis as unknown as { chrome?: ChromeRuntimeShim })
    .chrome;
  const runtime = chrome?.runtime;
  if (typeof runtime?.sendMessage !== "function") return null;
  return runtime;
}

function unwrapSwResponse<T>(response: unknown): T {
  const r = response as SwResponse<T> | undefined;
  if (!r) {
    throw new ExtensionBridgeError(
      "bridge_failed",
      "empty response from extension",
    );
  }
  if (r.ok) return r.data;
  throw new ExtensionBridgeError(
    r.error?.kind ?? "bridge_failed",
    r.error?.message ?? "extension returned error",
  );
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
  const runtime = getRuntime();
  if (runtime) {
    let timer: number | undefined;
    try {
      const response = await Promise.race([
        runtime.sendMessage(payload),
        new Promise<never>((_, reject) => {
          timer = window.setTimeout(() => {
            reject(
              new ExtensionBridgeTimeout(
                `extension did not respond within ${timeoutMs}ms`,
              ),
            );
          }, timeoutMs);
        }),
      ]);
      return unwrapSwResponse<T>(response);
    } finally {
      if (timer !== undefined) window.clearTimeout(timer);
    }
  }

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
      try {
        resolve(unwrapSwResponse<T>(event.data.response));
      } catch (err) {
        reject(err);
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

// ─── broadcasts ──────────────────────────────────────────────────────

/** Shape of `chrome.storage.local` change broadcasts the content-script
 *  fans out via window.postMessage. */
interface BridgeBroadcastEnvelope {
  source: typeof RES_TAG;
  id: typeof BROADCAST_ID;
  event: "changed";
  keys: string[];
}

function isBridgeBroadcast(value: unknown): value is BridgeBroadcastEnvelope {
  if (!value || typeof value !== "object") return false;
  const o = value as Record<string, unknown>;
  return (
    o.source === RES_TAG &&
    o.id === BROADCAST_ID &&
    o.event === "changed" &&
    Array.isArray(o.keys)
  );
}

interface ChromeStorageShim {
  storage?: {
    onChanged: {
      addListener(
        cb: (
          changes: Record<string, unknown>,
          areaName: string,
        ) => void,
      ): void;
      removeListener(
        cb: (
          changes: Record<string, unknown>,
          areaName: string,
        ) => void,
      ): void;
    };
  };
}

/**
 * Subscribe to storage-change broadcasts.
 *
 * Two delivery paths cover the two hosts the dashboard ships in:
 *  1. Extension options page (chrome-extension://<id>/options.html) —
 *     `chrome.storage.onChanged` is available directly; the
 *     content-script bridge does NOT inject here (its manifest only
 *     matches localhost dev origins), so the page MUST subscribe to
 *     the storage API itself.
 *  2. Vite dev server (http://localhost:5173) — `chrome.storage` is
 *     undefined; the content-script bridge fans `onChanged` out via
 *     `window.postMessage` and we listen on that.
 *
 * We register whichever paths are available so the callback fires once
 * per change in either host.
 */
export function subscribeToBroadcast(
  callback: (keys: string[]) => void,
): () => void {
  const cleanups: Array<() => void> = [];

  const messageHandler = (event: MessageEvent): void => {
    if (event.source !== window) return;
    if (event.origin !== window.location.origin) return;
    if (!isBridgeBroadcast(event.data)) return;
    callback(event.data.keys);
  };
  window.addEventListener("message", messageHandler);
  cleanups.push(() => window.removeEventListener("message", messageHandler));

  const chrome = (globalThis as unknown as { chrome?: ChromeStorageShim })
    .chrome;
  const storageOnChanged = chrome?.storage?.onChanged;
  if (storageOnChanged) {
    const storageHandler = (
      changes: Record<string, unknown>,
      areaName: string,
    ): void => {
      if (areaName !== "local") return;
      const keys = Object.keys(changes);
      if (keys.length === 0) return;
      callback(keys);
    };
    storageOnChanged.addListener(storageHandler);
    cleanups.push(() => storageOnChanged.removeListener(storageHandler));
  }

  return () => {
    for (const fn of cleanups) fn();
  };
}
