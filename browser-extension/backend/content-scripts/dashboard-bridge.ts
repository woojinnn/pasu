import Browser from "webextension-polyfill";

// Tags used to label messages crossing window <-> content-script. Anyone can
// fire postMessage from any iframe, so the content script ignores anything
// without an exact source tag match. The manifest's `matches` (localhost/127.0.0.1)
// is the first gate; this is defense in depth so a non-dashboard localhost
// page can't hijack the channel.
const REQ_TAG = "dambi-dashboard";
const RES_TAG = "dambi-extension";
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
  // Audit log appends on every verdict — the dashboard's AuditPage uses this
  // to refresh live. Trimmed to AUDIT_MAX entries so payloads stay small.
  "requests:audit",
  // Manifest store + migration queue. The manifest editor and migration banner
  // subscribe so the UI mirrors installs from other tabs and the popup.
  "rpc:manifests",
  "rpc:endpointUrl",
  "rpc:enrichedSchemaHash",
  "migration:pending",
  // Active-user discriminator — when this flips, all per-user reads change.
  "dashboard:current-user-id",
  // 익스텐션 로그인 토큰 — 계정 전환/로그아웃을 대시보드가 실시간 감지하도록 broadcast.
  "dambi_jwt",
]);

/**
 * Prefix-match watchers for per-user namespaced keys. Broadcast on any user's
 * change so React Query invalidators react across account switches without
 * needing to know the current user id up front.
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
