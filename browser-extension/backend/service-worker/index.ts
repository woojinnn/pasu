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
import { applyEnabledIds, getCatalog, getEnabledIds } from "./policy-selection";
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
import {
  simulatePolicySequence,
  testPolicyText,
  validatePolicyText,
} from "./wasm-bridge";
import {
  clearExecutionReports,
  countExecutionReports,
  listExecutionReports,
  type ExecutionReportFilter,
} from "./execution-report-storage";
import {
  clearVerdicts,
  countVerdicts,
  exportVerdictsAsCsv,
  listVerdicts,
  setVerdictDecision as setStoredVerdictDecision,
  type VerdictFilter,
} from "./verdict-storage";

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

  // B4 cleanup (commits 6aa3cc0 / b6f3ac9) — v1 routing 의 `registry:adapter-bundles`
  // chrome.storage namespace 가 deprecated. v3 = 별 namespace
  // (`scopeball:declarative-v3-bundle:*`). 보존된 v1 key 가 storage 용량 차지하므로
  // boot 시 한 번 제거. SW restart 후 entry 부재 → 영향 0.
  try {
    await Browser.storage.local.remove("registry:adapter-bundles");
  } catch {
    // ignore — key 부재 또는 storage error 시 silent. boot 진행 영향 X.
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
/** apps/web Editor + Simulation pages route Cedar through the
 *  service worker rather than bundling wasm themselves. Three
 *  request variants map 1-1 to the new exports in
 *  `crates/policy-engine-wasm/src/cedar_exports.rs`. */
interface CedarValidateRequest {
  type: "cedar-validate";
  text: string;
}
interface CedarTestRequest {
  type: "cedar-test";
  text: string;
  // Pre-serialized JSON of `CedarRequestInput` so the wasm boundary
  // stays string-in / string-out and the FE doesn't have to know
  // the rust dto shape exactly.
  request_json: string;
}
interface CedarSimulateRequest {
  type: "cedar-simulate";
  steps_json: string;
  policies_json: string;
}
interface ExecutionReportsListRequest {
  type: "execution-reports:list";
  opts?: ExecutionReportFilter;
}
interface ExecutionReportsCountRequest {
  type: "execution-reports:count";
  opts?: ExecutionReportFilter;
}
interface ExecutionReportsClearRequest {
  type: "execution-reports:clear";
}
interface VerdictsListRequest {
  type: "verdicts:list";
  opts?: VerdictFilter;
}
interface VerdictsCountRequest {
  type: "verdicts:count";
  opts?: VerdictFilter;
}
interface VerdictsSetDecisionRequest {
  type: "verdicts:set-decision";
  id: string;
  decision: "trusted" | "cancelled";
}
interface VerdictsExportCsvRequest {
  type: "verdicts:export-csv";
  opts?: VerdictFilter;
}
interface VerdictsClearRequest {
  type: "verdicts:clear";
}
/** Read just the enabled-policy id list. The dashboard's policy list
 *  uses this for the checkbox state; the popup also uses it indirectly
 *  via `policy-catalog`. Keeping a dedicated `:get` lets the dashboard
 *  invalidate the lighter query on storage broadcasts. */
interface PolicySelectionGetRequest {
  type: "policy-selection:get";
}
type PopupRequest =
  | PolicyCatalogRequest
  | SetEnabledIdsRequest
  | PolicySelectionGetRequest
  | ScopeballAuthStatusRequest
  | ScopeballAuthSignInRequest
  | ScopeballAuthSignOutRequest
  | ScopeballListWalletsRequest
  | CedarValidateRequest
  | CedarTestRequest
  | CedarSimulateRequest
  | ExecutionReportsListRequest
  | ExecutionReportsCountRequest
  | ExecutionReportsClearRequest
  | VerdictsListRequest
  | VerdictsCountRequest
  | VerdictsSetDecisionRequest
  | VerdictsExportCsvRequest
  | VerdictsClearRequest;

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

    // apps/web Cedar editor / simulation. Three message types, all
    // forwarded to policy-engine-wasm cedar_exports. Return value is
    // the raw JSON string the wasm produces — the FE parses.
    if (req.type === "cedar-validate") {
      void validatePolicyText((req as CedarValidateRequest).text)
        .then((json) => sendResponse({ ok: true, data: json }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "cedar_validate_failed", message: String(err) },
          }),
        );
      return true;
    }
    if (req.type === "cedar-test") {
      const r = req as CedarTestRequest;
      void testPolicyText(r.text, r.request_json)
        .then((json) => sendResponse({ ok: true, data: json }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "cedar_test_failed", message: String(err) },
          }),
        );
      return true;
    }
    if (req.type === "cedar-simulate") {
      const r = req as CedarSimulateRequest;
      void simulatePolicySequence(r.steps_json, r.policies_json)
        .then((json) => sendResponse({ ok: true, data: json }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "cedar_simulate_failed", message: String(err) },
          }),
        );
      return true;
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

    if (req.type === "execution-reports:list") {
      void listExecutionReports((req as ExecutionReportsListRequest).opts)
        .then((data) => sendResponse({ ok: true, data }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: {
              kind: "execution_reports_list_failed",
              message: String(err),
            },
          }),
        );
      return true;
    }

    if (req.type === "execution-reports:count") {
      void countExecutionReports((req as ExecutionReportsCountRequest).opts)
        .then((data) => sendResponse({ ok: true, data }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: {
              kind: "execution_reports_count_failed",
              message: String(err),
            },
          }),
        );
      return true;
    }

    if (req.type === "execution-reports:clear") {
      void clearExecutionReports()
        .then(() => sendResponse({ ok: true, data: { cleared: true } }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: {
              kind: "execution_reports_clear_failed",
              message: String(err),
            },
          }),
        );
      return true;
    }

    if (req.type === "verdicts:list") {
      void listVerdicts((req as VerdictsListRequest).opts)
        .then((data) => sendResponse({ ok: true, data }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "verdicts_list_failed", message: String(err) },
          }),
        );
      return true;
    }

    if (req.type === "verdicts:count") {
      void countVerdicts((req as VerdictsCountRequest).opts)
        .then((data) => sendResponse({ ok: true, data }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "verdicts_count_failed", message: String(err) },
          }),
        );
      return true;
    }

    if (req.type === "verdicts:set-decision") {
      const r = req as VerdictsSetDecisionRequest;
      if (
        typeof r.id !== "string" ||
        (r.decision !== "trusted" && r.decision !== "cancelled")
      ) {
        sendResponse({
          ok: false,
          error: {
            kind: "invalid_request",
            message: "id and decision are required",
          },
        });
        return true;
      }
      void setStoredVerdictDecision(r.id, r.decision)
        .then((updated) => sendResponse({ ok: true, data: { updated } }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: {
              kind: "verdicts_set_decision_failed",
              message: String(err),
            },
          }),
        );
      return true;
    }

    if (req.type === "verdicts:export-csv") {
      void exportVerdictsAsCsv((req as VerdictsExportCsvRequest).opts)
        .then((csv) => sendResponse({ ok: true, data: { csv } }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "verdicts_export_failed", message: String(err) },
          }),
        );
      return true;
    }

    if (req.type === "verdicts:clear") {
      void clearVerdicts()
        .then(() => sendResponse({ ok: true, data: { cleared: true } }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "verdicts_clear_failed", message: String(err) },
          }),
        );
      return true;
    }

    if (req.type === "policy-selection:get") {
      void getEnabledIds()
        .then((ids) => sendResponse({ ok: true, data: ids }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "policy_selection_get_failed", message: String(err) },
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
