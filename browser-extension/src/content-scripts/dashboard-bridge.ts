import Browser from "webextension-polyfill";

// Tags used to label messages crossing window <-> content-script. Anyone can
// fire postMessage from any iframe, so the content script ignores anything
// without an exact source tag match. The manifest's `matches` (localhost/127.0.0.1)
// is the first gate; this is defense in depth so a non-dashboard localhost
// page can't hijack the channel.
const REQ_TAG = "scopeball-dashboard";
const RES_TAG = "scopeball-extension";
const BROADCAST_ID = "__broadcast__";

// Origins that the page bridge accepts. The manifest already restricts where
// this script runs, but if Chrome ever loads it on an unintended page (e.g.
// a localhost dev server proxied through https), we still want to refuse.
function originAllowed(origin: string): boolean {
  return (
    origin.startsWith("http://localhost") ||
    origin.startsWith("http://127.0.0.1")
  );
}

interface BridgeRequest {
  source: typeof REQ_TAG;
  id: string;
  payload: unknown;
}

function isBridgeRequest(value: unknown): value is BridgeRequest {
  if (!value || typeof value !== "object") return false;
  const o = value as Record<string, unknown>;
  return (
    o.source === REQ_TAG && typeof o.id === "string" && "payload" in o
  );
}

window.addEventListener("message", (event) => {
  if (event.source !== window) return;
  if (!originAllowed(event.origin)) return;
  if (!isBridgeRequest(event.data)) return;
  const { id, payload } = event.data;
  void forward(id, payload, event.origin);
});

async function forward(
  id: string,
  payload: unknown,
  origin: string,
): Promise<void> {
  try {
    const response = await Browser.runtime.sendMessage(payload);
    window.postMessage({ source: RES_TAG, id, response }, origin);
  } catch (err) {
    window.postMessage(
      {
        source: RES_TAG,
        id,
        response: {
          ok: false,
          error: { kind: "bridge_failed", message: String(err) },
        },
      },
      origin,
    );
  }
}

// Broadcast extension storage changes to the dashboard page so SDKs can
// invalidate caches / refetch when the popup or another tab mutates state.
// Keys watched are deliberately narrow so unrelated storage churn doesn't
// fan out to the page.
const WATCHED_KEYS = new Set([
  "dashboard:policies",
  "marketplace:bundles",
  "policy-selection:enabled-ids",
  "policy-selection:applied-ids",
]);

Browser.storage.onChanged.addListener((changes, areaName) => {
  if (areaName !== "local") return;
  const touched = Object.keys(changes).filter((k) => WATCHED_KEYS.has(k));
  if (touched.length === 0) return;
  window.postMessage(
    {
      source: RES_TAG,
      id: BROADCAST_ID,
      event: "changed",
      keys: touched,
    },
    location.origin,
  );
});
