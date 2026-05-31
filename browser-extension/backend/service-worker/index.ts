import Browser from "webextension-polyfill";
import { Identifier } from "@lib/identifier";
import { handleDashboardRequest, isDashboardRequest } from "./dashboard/api";
import { handleManifestRequest, isManifestRequest } from "./manifests/handlers";
import { hydrateManifests } from "./manifests/hydrate";
import { migrateAdapterLoaderStorageKey } from "./manifests/adapter-loader-storage-migration";
import { detectPendingMigrations } from "./manifests/migration-detector";
import { decideMessage } from "./orchestrator";
import { reportExecutionOutcome } from "./execution-report";
import {
  ensureDefaultPoliciesInstalled,
  reinstallAllPolicies,
} from "./policies-loader";
import { loadDefaultPolicySetV2 } from "./policies-loader-v2";
import { applyEnabledIds, getCatalog } from "./policy-selection";
import {
  isExecutionReport,
  RequestType,
  type Message,
  type MessageResponse,
} from "@lib/types";
import {
  clearTokens,
  fetchMe,
  listWallets,
  startGoogleLogin,
  type Me,
  type WalletId,
} from "./scopeball-auth";

const WALLET_ACTION_TYPES = new Set<string>([
  RequestType.TRANSACTION,
  RequestType.TYPED_SIGNATURE,
  RequestType.UNTYPED_SIGNATURE,
  // Without this, the SW silently drops venue-order messages (no verdict ever
  // posts back) and the fetch hook times out → the order would slip through.
  RequestType.VENUE_ORDER,
]);

console.log("Scopeball SW alive at", new Date().toISOString());

// SW boot sequence (Phase 6, carry-over G):
//
// `ensureDefaultPoliciesInstalled()` and `hydrateManifests()` both end up
// calling `wasmInstallPolicies(...)` under the hood. Firing them in
// parallel created a last-writer-wins race — whichever install completed
// second would clobber the WASM engine state, leaving storage and the
// engine out of sync. We serialize them here: defaults first (they prime
// the engine in the common cold-start path), then declarative seed
// bundles, then hydrate stored manifests on top.
//
// Both stages stay best-effort: failures are logged so the engine still
// serves the legacy `policies-loader` install path on the first
// `decideMessage` retry. We do NOT block the runtime listeners below on
// this promise — they should be installed synchronously so the SW can
// queue messages while warmup is in flight.
void bootSequence().catch((err) => {
  console.warn("[Scopeball] boot sequence failed:", err);
});

async function bootSequence(): Promise<void> {
  // Fix R: run the migration detector BEFORE the install passes. The
  // detector strips v0 policy ids out of `policy-selection:enabled-ids`
  // and snapshots their prior enabled-state into
  // `migration:original-enabled`. If we ran the install first the
  // enriched-schema validation would reject every v0 policy and the
  // whole `installFiltered` call would error — orchestrator's reject
  // path would then fire on every request until the user opened the
  // dashboard and clicked Rewrite. By detecting + disabling first we
  // keep the rest of the enabled set installable and the engine green.
  //
  // Idempotent: re-running after a manual rewrite never appends
  // already-cleared ids and preserves the first-detection snapshot.
  try {
    await detectPendingMigrations();
  } catch (err) {
    console.warn("[Scopeball] migration auto-detect failed:", err);
  }

  // Adapter-loader storage key migration (one-time, idempotent).
  //
  // The `marketplace/` directory was renamed to `adapter-loader/` —
  // chrome.storage key `"marketplace:bundles"` (installed adapter bundle
  // cache) also moved to `"adapter-loader:bundles"`. Runs after the
  // policy-level migration detector and before any install path so the
  // bundle storage is at the new key when downstream code reads it. The
  // migration touches chrome.storage only (no WASM dependency), so its
  // placement is purely about ordering with other migrations.
  try {
    await migrateAdapterLoaderStorageKey();
  } catch (err) {
    console.warn("[Scopeball] adapter-loader storage migration failed:", err);
    // Non-fatal — first JIT fetch will populate the new key anyway.
  }

  // Cold-start prewarm: kick off WASM module load + default policy
  // install so the first dApp request doesn't pay the 4.77MB compile
  // cost inside the 3s lifecycle budget. We await this before hydrating
  // manifests — otherwise the two install paths would race on the
  // shared WASM engine state.
  try {
    await ensureDefaultPoliciesInstalled();
  } catch (err) {
    console.warn("[Scopeball] cold-start prewarm failed:", err);
  }

  // Phase 6 / Task 6.3: hydrate the manifest-driven schema on SW boot.
  //
  // Two paths share the same atomic-install plumbing:
  // 1. Prod cold-start restore — if storage already has manifests (the
  //    user installed them via the dashboard in a previous lifetime),
  //    push them back into WASM so the engine starts up with the right
  //    schema.
  // 2. Dev seeding — when `NODE_ENV !== "production"`, `devSeed()` fills
  //    in any missing default actions from `public/default-manifests/`.
  //    Prod builds short-circuit inside `devSeed`.
  try {
    await hydrateManifests();
  } catch (err) {
    console.warn("[Scopeball] manifest hydration failed:", err);
  }

  // Phase 1 / P2: warm the in-memory default v2 policy set so the first
  // decision doesn't pay the fetch. v2 evaluation is STATELESS — this is a
  // pure asset fetch + module-level cache, with NO WASM state to push, so
  // its ordering relative to the install stages above does not matter.
  // Best-effort like the surrounding stages: a failure here logs and leaves
  // the cache empty (the loader returns `[]`); it must never brick boot.
  try {
    const v2 = await loadDefaultPolicySetV2();
    // Visible boot proof: which v2 deny/warn bundles are actually loaded into
    // this SW. If this logs `[]`, the policy asset failed to fetch (check the
    // warning above) and nothing will be enforced.
    console.log(
      `[Scopeball] v2 default policies loaded (${v2.length}):`,
      v2.map((b) => b.id),
    );
  } catch (err) {
    console.warn("[Scopeball] v2 default policy load failed:", err);
  }
}

Browser.runtime.onConnect.addListener((port) => {
  if (port.name !== Identifier.CONTENT_SCRIPT) return;

  port.onMessage.addListener((message: Message) => {
    void handleMessage(message, port);
  });
});

async function handleMessage(
  message: Message,
  port: Browser.Runtime.Port,
): Promise<void> {
  // Raw / frozen advisories: log only (Plan 5 doesn't gate, but surfaces
  // them so the user can see something happened).
  if (message.data.type === "raw-transaction-advisory") {
    console.warn("[Scopeball] raw-tx advisory", message.data);
    return;
  }
  if (message.data.type === "provider-frozen-warning") {
    console.error("[Scopeball] provider frozen", message.data);
    return;
  }

  if (isExecutionReport(message)) {
    await reportExecutionOutcome(message.data);
    return;
  }

  // Skip messages that aren't wallet actions (transaction / typed sig /
  // untyped sig). The proxy is injected into every iframe (manifest
  // <all_urls> + all_frames), so probes from third-party widgets like
  // Cloudflare's bot challenge can deliver shapes the engine doesn't
  // know how to evaluate. Treating them as policy verdicts would pop a
  // "Blocked: __engine::unsupported" modal on every page that embeds
  // such a widget.
  if (!WALLET_ACTION_TYPES.has(message.data.type)) {
    return;
  }

  const { ok } = await decideMessage(message, {
    onAwaitingUser: () => {
      try {
        port.postMessage({
          requestId: message.requestId,
          kind: "awaiting-user",
        });
      } catch {
        /* dApp tab gone */
      }
    },
  });
  if (!message.data.bypassed) {
    const response: MessageResponse = {
      requestId: message.requestId,
      data: ok,
    };
    try {
      port.postMessage(response);
    } catch {
      /* dApp tab gone */
    }
  }
}

interface PolicyCatalogRequest {
  type: "policy-catalog";
}
interface SetEnabledIdsRequest {
  type: "set-enabled-ids";
  ids: string[];
}
interface ScopeballAuthStatusRequest {
  type: "scopeball-auth-status";
}
interface ScopeballAuthSignInRequest {
  type: "scopeball-auth-sign-in";
}
interface ScopeballAuthSignOutRequest {
  type: "scopeball-auth-sign-out";
}
interface ScopeballListWalletsRequest {
  type: "scopeball-list-wallets";
}
type PopupRequest =
  | PolicyCatalogRequest
  | SetEnabledIdsRequest
  | ScopeballAuthStatusRequest
  | ScopeballAuthSignInRequest
  | ScopeballAuthSignOutRequest
  | ScopeballListWalletsRequest;

// webextension-polyfill's listener type accepts `true | void | Promise<any>`,
// not `boolean`. Returning `undefined` (bare `return;`) closes the channel
// just like a literal `false` would — do not "fix" it back to `return false`.
Browser.runtime.onMessage.addListener(
  (message: unknown, _sender, sendResponse: (r: unknown) => void) => {
    const req = message as Partial<PopupRequest> | null;
    if (!req || typeof req !== "object") return;

    if (req.type === "policy-catalog") {
      void getCatalog()
        .then((cat) => sendResponse({ ok: true, data: cat }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "catalog_failed", message: String(err) },
          }),
        );
      return true; // keep the channel open for the async response
    }

    if (isDashboardRequest(req)) {
      void handleDashboardRequest(req)
        .then((response) => sendResponse(response))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "dashboard_failed", message: String(err) },
          }),
        );
      return true;
    }

    // Phase 6 / Task 6.5: manifest CRUD, schema preview, migration.
    if (isManifestRequest(req)) {
      void handleManifestRequest(req)
        .then((response) => sendResponse(response))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "manifest_failed", message: String(err) },
          }),
        );
      return true;
    }

    // Scopeball (Rust server) auth — separate from the legacy 8787 path.
    // Each handler returns `{ ok, data | error }` so the popup can match
    // uniformly.
    if (req.type === "scopeball-auth-status") {
      void fetchMe()
        .then((me: Me | null) => sendResponse({ ok: true, data: me }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "scopeball_auth_failed", message: String(err) },
          }),
        );
      return true;
    }

    if (req.type === "scopeball-auth-sign-in") {
      void startGoogleLogin()
        .then(async () => {
          const me = await fetchMe();
          sendResponse({ ok: true, data: me });
        })
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "scopeball_sign_in_failed", message: String(err) },
          }),
        );
      return true;
    }

    if (req.type === "scopeball-auth-sign-out") {
      void clearTokens()
        .then(() => sendResponse({ ok: true, data: null }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "scopeball_sign_out_failed", message: String(err) },
          }),
        );
      return true;
    }

    if (req.type === "scopeball-list-wallets") {
      void listWallets()
        .then((wallets: WalletId[]) =>
          sendResponse({ ok: true, data: wallets }),
        )
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "scopeball_list_wallets_failed", message: String(err) },
          }),
        );
      return true;
    }

    if (req.type === "set-enabled-ids") {
      // Reject malformed `ids` instead of silently coercing to []. A
      // non-array, or an array containing non-strings, would otherwise
      // disable all policies without telling the caller.
      if (
        !Array.isArray(req.ids) ||
        !req.ids.every((id) => typeof id === "string")
      ) {
        sendResponse({
          ok: false,
          error: { kind: "invalid_request", message: "ids must be string[]" },
        });
        return true;
      }
      const ids = req.ids;
      void applyEnabledIds(ids, reinstallAllPolicies)
        .then((result) => sendResponse(result))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "apply_failed", message: String(err) },
          }),
        );
      return true;
    }

    return;
  },
);
