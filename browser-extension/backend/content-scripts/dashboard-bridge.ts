import Browser from "webextension-polyfill";

// Tags used to label messages crossing window <-> content-script. Anyone can
// fire postMessage from any iframe, so the content script ignores anything
// without an exact source tag match. The manifest's `matches` (localhost/127.0.0.1)
// is the first gate; this is defense in depth so a non-dashboard localhost
// page can't hijack the channel.
const REQ_TAG = "pasu-dashboard";
const RES_TAG = "pasu-extension";
const BROADCAST_ID = "__broadcast__";

// Origins that the page bridge accepts. The manifest already restricts where
// this script runs (matches: http://localhost:5173/*, http://127.0.0.1:5173/*),
// but we re-check at runtime so a future manifest change can't accidentally
// widen the bridge to arbitrary origins. Keep these two in sync.
const DASHBOARD_ORIGINS = new Set([
  "http://localhost:5173",
  "http://127.0.0.1:5173",
]);

function originAllowed(origin: string): boolean {
  return DASHBOARD_ORIGINS.has(origin);
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
  "adapter-loader:bundles",
  // Audit log appends on every verdict — dashboard's AuditPage uses this
  // to refresh on the fly. Storage layer trims to AUDIT_MAX (100), so the
  // change events fire at most once per decision and the payload stays small.
  "requests:audit",
  // Phase 6 / Task 6.5: manifest store + migration queue. The dashboard
  // manifest editor and migration banner subscribe so the UI mirrors
  // installs from other tabs and the popup.
  "rpc:manifests",
  "rpc:endpointUrl",
  "rpc:enrichedSchemaHash",
  "migration:pending",
  // Active-user discriminator — when this flips, all per-user reads change.
  "dashboard:current-user-id",
]);

/**
 * Prefix-match watchers for keys that are now namespaced per user. Pre-
 * namespacing they were flat strings (`dashboard:policies`); after
 * namespacing they include the user id (`dashboard:policies:<uid>`), and
 * we want to broadcast on ANY user's change so the dashboard's React
 * Query invalidators react across account switches without needing to
 * know the current user id up front.
 */
const WATCHED_KEY_PREFIXES = [
  "dashboard:policies:",
  "dashboard:sets:",
  "policy-selection:enabled-ids:",
  "policy-selection:applied-ids:",
];

Browser.storage.onChanged.addListener((changes, areaName) => {
  if (areaName !== "local") return;
  const touched = Object.keys(changes).filter(
    (k) => WATCHED_KEYS.has(k) || WATCHED_KEY_PREFIXES.some((p) => k.startsWith(p)),
  );
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
