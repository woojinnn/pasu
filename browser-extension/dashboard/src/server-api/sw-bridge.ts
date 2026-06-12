/**
 * Thin wrapper around `chrome.runtime.sendMessage` for the options page.
 *
 * Only callable inside the extension context (the page is served from
 * `chrome-extension://<id>/options.html`). Callers MUST gate the
 * invocation with `isExtensionContext()` from `../env`; we still guard
 * here as a defensive backstop.
 *
 * We do NOT pull in `@types/chrome` for the dashboard because the dev
 * server build runs in a plain web context where the chrome runtime is
 * undefined. A narrow ambient type is enough for the one call site.
 */

import { isExtensionContext } from "../env";

interface SwResponseEnvelope<T> {
  ok: boolean;
  data?: T;
  /** Present on `dambi-auth-sign-in` so the dashboard can mirror
   * the access token into localStorage. The popup ignores it. */
  tokens?: { access: string; refresh: string | null };
  error?: { kind: string; message: string };
}

interface ChromeRuntimeShim {
  runtime?: {
    sendMessage(message: unknown): Promise<unknown>;
  };
}

function getRuntime(): ChromeRuntimeShim["runtime"] | null {
  if (!isExtensionContext()) return null;
  const g = globalThis as unknown as { chrome?: ChromeRuntimeShim };
  return g.chrome?.runtime ?? null;
}

/** Send a typed message to the background service worker. Throws a
 * descriptive Error on transport failure or on `{ ok: false }`. */
export async function sendToSw<T>(
  type: string,
): Promise<SwResponseEnvelope<T>> {
  const runtime = getRuntime();
  if (!runtime) {
    throw new Error(
      "sendToSw called outside the extension — gate with isExtensionContext()",
    );
  }
  const res = (await runtime.sendMessage({ type })) as
    | SwResponseEnvelope<T>
    | undefined;
  if (!res) throw new Error("empty response from service worker");
  if (!res.ok) {
    throw new Error(
      `${res.error?.kind ?? "sw_error"}: ${res.error?.message ?? "unknown"}`,
    );
  }
  return res;
}
