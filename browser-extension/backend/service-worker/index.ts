import Browser from "webextension-polyfill";
import { Identifier } from "@lib/identifier";
import { handleDashboardRequest, isDashboardRequest } from "./dashboard/api";
import { handleManifestRequest, isManifestRequest } from "./manifests/handlers";
import { hydrateManifests } from "./manifests/hydrate";
import { migrateAdapterLoaderStorageKey } from "./manifests/adapter-loader-storage-migration";
import { migratePasuRenameStorageKeys } from "./manifests/pasu-rename-storage-migration";
import { detectPendingMigrations } from "./manifests/migration-detector";
import { cleanupLegacyKeys } from "./policy-store/seed";
import {
  handlePs2Request,
  isPs2Request,
  provisionFromWalletSync,
  type Ps2Request,
} from "./policy-store/api";
import { decideMessage } from "./orchestrator";
import { reportExecutionOutcome } from "./execution-report";
import {
  ensureDefaultV3BundlesInstalled,
  getInstalledV3BundleCount,
  v3BundleBootCompleted,
} from "./v3-bundle-loader";
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
  listWalletSummaries,
  addWallet,
  updateWallet,
  deleteWallet,
  setTokens,
  startGoogleLogin,
  type Me,
  type WalletId,
  type WalletSummary,
  type AddWalletResp,
} from "./pasu-auth";
import {
  declarativeRouteRequestV3,
  estToPolicyText,
  evaluateActionV2,
  policyTextToEst,
  runDiagnosisProbesV2,
  simulatePolicySequence,
  simulateStep,
  testPolicyText,
  validatePolicyText,
  type DeclarativeRouteRequestV3Input,
  type DeclarativeRouteRequestV3Result,
  type EvaluateActionV2InputDto,
  type VerdictDto,
  type SimulateStepInput,
  type SimulateStepOutput,
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
import {
  clearStateDeltas,
  getStateDelta,
  type StateDeltaRow,
} from "./state-delta-storage";
import {
  getDiagnosisContext,
  type DiagnosisContextRow,
} from "./diagnosis-context-storage";

const WALLET_ACTION_TYPES = new Set<string>([
  RequestType.TRANSACTION,
  RequestType.TYPED_SIGNATURE,
  RequestType.UNTYPED_SIGNATURE,
  // Without this, the SW silently drops venue-order messages (no verdict ever
  // posts back) and the fetch hook times out → the order would slip through.
  RequestType.VENUE_ORDER,
]);

console.log("Pasu SW alive at", new Date().toISOString());

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
//
// `bootReady` exposes boot completion so auth handlers can `await` it
// before reading tokens. In MV3 the SW is woken BY a message and the
// message listeners are installed synchronously — without this gate a
// token read could run before the pasu-rename migration's `set` lands,
// read an absent `pasu_jwt`, and show the user logged out. The `.catch`
// keeps the promise non-rejecting, so awaiting it never throws (boot is
// best-effort; a stalled stage must not brick the auth handlers).
export const bootReady: Promise<void> = bootSequence().catch((err) => {
  console.warn("[Pasu] boot sequence failed:", err);
});

async function bootSequence(): Promise<void> {
  // Pasu rename storage-key migration (one-time, idempotent). Runs FIRST so
  // the renamed auth/server-url/diagnostics keys (scopeball_* → pasu_*) are
  // populated before any boot step — or any incoming message handler — reads
  // the new keys. Without it the rename would silently log existing users out
  // and drop their chosen server URL. Touches chrome.storage only (no WASM).
  try {
    await migratePasuRenameStorageKeys();
  } catch (err) {
    console.warn("[Pasu] rename storage migration failed:", err);
    // Non-fatal — a logged-out user can simply sign in again.
  }

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
    console.warn("[Pasu] migration auto-detect failed:", err);
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
    console.warn("[Pasu] adapter-loader storage migration failed:", err);
    // Non-fatal — first JIT fetch will populate the new key anyway.
  }

  // B4 cleanup (commits 6aa3cc0 / b6f3ac9) — v1 routing 의 `registry:adapter-bundles`
  // chrome.storage namespace 가 deprecated. v3 = 별 namespace
  // (`pasu:declarative-v3-bundle:*`). 보존된 v1 key 가 storage 용량 차지하므로
  // boot 시 한 번 제거. SW restart 후 entry 부재 → 영향 0.
  try {
    await Browser.storage.local.remove("registry:adapter-bundles");
  } catch {
    // ignore — key 부재 또는 storage error 시 silent. boot 진행 영향 X.
  }

  // 정책 스토리지 v2 — 구(v1) 정책 키 리셋(마이그레이션 없음, 스펙 합의).
  // `dashboard:policies/sets`, `policy-selection:*`, `migration:*` 네임스페이스를
  // 제거한다. ps2:* 시드는 uid가 필요하므로 여기가 아니라 첫 resolve/프로비저닝
  // 호출에서 lazy하게 일어난다.
  try {
    await cleanupLegacyKeys();
  } catch (err) {
    console.warn("[Pasu] legacy policy-storage cleanup failed:", err);
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
    console.warn("[Pasu] manifest hydration failed:", err);
  }

  // Default v3 decoder bundles — used by the simulation page so the
  // `declarative_route_request_v3_json` decoder has something to look up
  // (without this it falls through to `ActionBody::Unknown` for every
  // calldata, even canonical ERC20 transfer/approve). Production
  // enforcement still uses the registry-api JIT path; this is a
  // simulator-friendly cold-start seed. Best-effort like the other stages.
  // Runs AFTER hydrateManifests so any per-bundle install errors don't
  // leave the engine in a half-installed manifest state.
  try {
    const v3Count = await ensureDefaultV3BundlesInstalled();
    console.log(`[Pasu] v3 default bundles installed (${v3Count})`);
  } catch (err) {
    console.warn("[Pasu] v3 default bundle install failed:", err);
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
    console.warn("[Pasu] raw-tx advisory", message.data);
    return;
  }
  if (message.data.type === "provider-frozen-warning") {
    console.error("[Pasu] provider frozen", message.data);
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

interface PasuAuthStatusRequest {
  type: "pasu-auth-status";
}
interface PasuAuthSignInRequest {
  type: "pasu-auth-sign-in";
}
interface PasuAuthSignOutRequest {
  type: "pasu-auth-sign-out";
}
/** Dashboard → SW token mirror. The dashboard's OAuth flow lands tokens in
 *  page `localStorage`; the SW reads tokens from `chrome.storage.local`.
 *  Without this sync the SW thinks the user is signed out even after a
 *  successful dashboard sign-in, and `recordSimulationOnServer` returns
 *  silently at its `hasToken` guard — leaving the HistoryPage's state-diff
 *  panel permanently empty. The dashboard calls this after every
 *  `fetchMe()` that resolves to a real user, so the sync is idempotent. */
interface PasuAuthSyncTokensRequest {
  type: "pasu-auth-sync-tokens";
  access: string;
  refresh: string | null;
}
interface PasuListWalletsRequest {
  type: "pasu-list-wallets";
}
/** Wallet 관리 — 팝업이 서버(GET/POST/PATCH/DELETE /wallets)를 단일 소스로
 *  쓰도록 SW 가 대리한다. 대시보드도 같은 서버를 읽어 일관성 유지. */
interface PasuListWalletSummariesRequest {
  type: "pasu-list-wallet-summaries";
}
interface PasuAddWalletRequest {
  type: "pasu-add-wallet";
  address: string;
  label?: string;
}
interface PasuUpdateWalletRequest {
  type: "pasu-update-wallet";
  address: string;
  label?: string;
}
interface PasuDeleteWalletRequest {
  type: "pasu-delete-wallet";
  address: string;
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
interface RunDiagnosisProbesRequest {
  type: "run-diagnosis-probes";
  input_json: string;
}
interface CedarTextToEstRequest {
  type: "cedar-text-to-est";
  text: string;
}
interface CedarEstToTextRequest {
  type: "cedar-est-to-text";
  // Pre-serialized EST JSON (a single policy's EST object).
  est_json: string;
}
/** Simulation page: one (state, action, ctx) → (delta, next_state).
 *  Dashboard owns the per-tx loop; SW just forwards to the wasm bridge.
 *  Contract: `crates/policy-engine-wasm/src/sim_step_exports.rs`. */
interface SimStepRequest {
  type: "sim-step";
  input: SimulateStepInput;
}
/** Simulation page: decode a raw tx (chain_id, to, calldata, …) into the
 *  typed `Action[]` tree the v3 route engine emits. Same wasm entry the SW
 *  orchestrator uses for live wallet flows — exposed here so the dashboard
 *  can drive the same decode → simulate pipeline from user-pasted calldata. */
interface SimDecodeRequest {
  type: "sim-decode";
  input: DeclarativeRouteRequestV3Input;
}
/** Simulation page: evaluate one (action, meta, tx, bundles, results) →
 *  `VerdictDto`. Pairs with `sim-step` so the dashboard's per-tx loop can
 *  compute BOTH the post-state AND the policy verdict at every step.
 *  Contract: `crates/policy-engine-wasm/src/action_eval_exports.rs`. */
interface SimEvaluateRequest {
  type: "sim-evaluate";
  input: EvaluateActionV2InputDto;
}
/** Simulation page: how many default v3 decoder bundles did this SW
 *  lifetime manage to install at boot? The probe surfaces a warning when
 *  this returns 0 (the decoder will return `Unknown` for everything in
 *  that case). Returns `{count, bootCompleted}` — `bootCompleted = false`
 *  means the install pass is still in-flight; the probe shows "warming up"
 *  instead of "no bundles". */
interface SimV3BundleCountRequest {
  type: "sim-v3-bundle-count";
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
/** HistoryPage detail panel: fetch the state-delta row that a verdict's
 *  `delta_id` points at. Returns `null` for missing ids (legacy rows or
 *  decisions whose `recordSimulationOnServer` couldn't reach the policy
 *  server). */
interface StateDeltasGetRequest {
  type: "state-deltas:get";
  id: string;
}
interface StateDeltasClearRequest {
  type: "state-deltas:clear";
}
/** HistoryPage / confirm-popup denial diagnosis: fetch the captured context
 *  (action + materialized enrichment results) a deny's `delta_id` points at, so
 *  the dashboard can re-run "which clause blocked this" against the real
 *  context. `null` for non-deny / legacy rows. */
interface DiagnosisContextGetRequest {
  type: "diagnosis-context:get";
  id: string;
}
type PopupRequest =
  | PasuAuthStatusRequest
  | PasuAuthSignInRequest
  | PasuAuthSignOutRequest
  | PasuAuthSyncTokensRequest
  | PasuListWalletsRequest
  | PasuListWalletSummariesRequest
  | PasuAddWalletRequest
  | PasuUpdateWalletRequest
  | PasuDeleteWalletRequest
  | CedarValidateRequest
  | CedarTestRequest
  | CedarSimulateRequest
  | RunDiagnosisProbesRequest
  | CedarTextToEstRequest
  | CedarEstToTextRequest
  | SimStepRequest
  | SimDecodeRequest
  | SimEvaluateRequest
  | SimV3BundleCountRequest
  | ExecutionReportsListRequest
  | ExecutionReportsCountRequest
  | ExecutionReportsClearRequest
  | VerdictsListRequest
  | VerdictsCountRequest
  | VerdictsSetDecisionRequest
  | VerdictsExportCsvRequest
  | VerdictsClearRequest
  | StateDeltasGetRequest
  | StateDeltasClearRequest
  | DiagnosisContextGetRequest
  | Ps2Request;

// webextension-polyfill's listener type accepts `true | void | Promise<any>`,
// not `boolean`. Returning `undefined` (bare `return;`) closes the channel
// just like a literal `false` would — do not "fix" it back to `return false`.
Browser.runtime.onMessage.addListener(
  (message: unknown, _sender, sendResponse: (r: unknown) => void) => {
    const req = message as Partial<PopupRequest> | null;
    if (!req || typeof req !== "object") return;

    // 정책 스토리지 v2 — ps2:* 패밀리는 단일 디스패처로. 핸들러별 분기 대신
    // api.ts의 switch가 메시지 모양을 ops로 위임한다(쓰기는 전부 mutate 큐).
    if (isPs2Request(req)) {
      void bootReady
        .then(() => handlePs2Request(req))
        .then((data) => sendResponse({ ok: true, data: data ?? null }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "ps2_failed", message: String(err) },
          }),
        );
      return true;
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
    if (req.type === "run-diagnosis-probes") {
      // Denial-diagnosis oracle. `input_json` is built by the dashboard's
      // `runDiagnosisProbes` and forwarded verbatim to WASM; `json` is the raw
      // WASM `{ ok, data }` envelope STRING, which the dashboard re-parses (see
      // dashboard `server-api/diagnosis.ts`). Guide: `cedar/diagnosis/README.md`.
      void runDiagnosisProbesV2((req as RunDiagnosisProbesRequest).input_json)
        .then((json) => sendResponse({ ok: true, data: json }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "run_diagnosis_probes_failed", message: String(err) },
          }),
        );
      return true;
    }
    if (req.type === "cedar-text-to-est") {
      void policyTextToEst((req as CedarTextToEstRequest).text)
        .then((json) => sendResponse({ ok: true, data: json }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "cedar_text_to_est_failed", message: String(err) },
          }),
        );
      return true;
    }
    if (req.type === "cedar-est-to-text") {
      void estToPolicyText((req as CedarEstToTextRequest).est_json)
        .then((json) => sendResponse({ ok: true, data: json }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "cedar_est_to_text_failed", message: String(err) },
          }),
        );
      return true;
    }
    if (req.type === "sim-step") {
      void simulateStep((req as SimStepRequest).input)
        .then((data: SimulateStepOutput) => sendResponse({ ok: true, data }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "sim_step_failed", message: String(err) },
          }),
        );
      return true;
    }
    if (req.type === "sim-decode") {
      void declarativeRouteRequestV3((req as SimDecodeRequest).input)
        .then((data: DeclarativeRouteRequestV3Result) =>
          sendResponse({ ok: true, data }),
        )
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "sim_decode_failed", message: String(err) },
          }),
        );
      return true;
    }
    if (req.type === "sim-evaluate") {
      void evaluateActionV2((req as SimEvaluateRequest).input)
        .then((data: VerdictDto) => sendResponse({ ok: true, data }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "sim_evaluate_failed", message: String(err) },
          }),
        );
      return true;
    }
    if (req.type === "sim-v3-bundle-count") {
      // Synchronous module-level counters — no await needed, but we keep
      // the async response shape for consistency with the other handlers.
      sendResponse({
        ok: true,
        data: {
          count: getInstalledV3BundleCount(),
          bootCompleted: v3BundleBootCompleted(),
        },
      });
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

    // Pasu (Rust server) auth — separate from the legacy 8787 path.
    // Each handler returns `{ ok, data | error }` so the popup can match
    // uniformly.
    if (req.type === "pasu-auth-status") {
      // Gate the token read on boot: the pasu-rename storage migration runs
      // inside `bootSequence()` and must finish copying `scopeball_jwt` →
      // `pasu_jwt` before we read the token, or a freshly-woken SW reports
      // the user logged out. `bootReady` never rejects (boot is best-effort).
      void bootReady
        .then(() => fetchMe())
        .then((me: Me | null) => sendResponse({ ok: true, data: me }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "pasu_auth_failed", message: String(err) },
          }),
        );
      return true;
    }

    if (req.type === "pasu-auth-sign-in") {
      // Await boot before the sign-in flow so its post-login `fetchMe()`
      // token read sees the migrated key (see `pasu-auth-status`).
      void bootReady
        // 새 로그인 전에 이전 계정 토큰을 먼저 비운다. 그래야 OAuth 진행 중
        // 같은 storage 를 공유하는 대시보드(options.html)가 옛 계정으로 잠깐
        // 인증되거나, 계정 전환 시 stale 토큰이 남는 race 를 막는다. 새 토큰은
        // startGoogleLogin 성공 시에만 기록된다.
        .then(() => clearTokens())
        .then(() => startGoogleLogin())
        .then(async () => {
          const me = await fetchMe();
          sendResponse({ ok: true, data: me });
        })
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "pasu_sign_in_failed", message: String(err) },
          }),
        );
      return true;
    }

    if (req.type === "pasu-auth-sign-out") {
      // Await boot so a sign-out can't race the migration re-populating
      // `pasu_jwt` from the stale `scopeball_jwt` after we clear it.
      void bootReady
        .then(() => clearTokens())
        .then(() => sendResponse({ ok: true, data: null }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "pasu_sign_out_failed", message: String(err) },
          }),
        );
      return true;
    }

    if (req.type === "pasu-auth-sync-tokens") {
      const r = req as PasuAuthSyncTokensRequest;
      // Await boot so the dashboard's token mirror can't be clobbered by
      // the migration's late `set` (both write `pasu_jwt`).
      void bootReady
        .then(() => setTokens(r.access, r.refresh))
        .then(() => sendResponse({ ok: true, data: null }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "pasu_sync_tokens_failed", message: String(err) },
          }),
        );
      return true;
    }

    if (req.type === "pasu-list-wallets") {
      // Await boot before the token read (see `pasu-auth-status`).
      void bootReady
        .then(() => listWallets())
        .then(async (wallets: WalletId[]) => {
          // 정책 스토리지 v2 프로비저닝 훅: 서버 지갑 목록과 동기화되는 이
          // 경로에서 새 지갑에 defaults를 바인딩한다(멱등). 실패해도 지갑
          // 목록 응답은 막지 않는다.
          try {
            await provisionFromWalletSync(wallets.map((w) => w.address));
          } catch (err) {
            console.warn("[Pasu] ps2 wallet provisioning failed:", err);
          }
          sendResponse({ ok: true, data: wallets });
        })
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "pasu_list_wallets_failed", message: String(err) },
          }),
        );
      return true;
    }

    // 지갑 요약(라벨+잔액) — 서버 GET /dashboard/summary. 팝업이 별칭(label)을
    // 서버 단일 소스에서 읽는 경로.
    if (req.type === "pasu-list-wallet-summaries") {
      void bootReady
        .then(() => listWalletSummaries())
        .then((wallets: WalletSummary[]) =>
          sendResponse({ ok: true, data: wallets }),
        )
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: {
              kind: "pasu_list_wallet_summaries_failed",
              message: String(err),
            },
          }),
        );
      return true;
    }

    // 지갑 등록(POST /wallets). `chains` 를 명시해 "no chains configured" 400 을
    // 우회한다 — 서버 pasu-sync.toml 에 RPC 가 설정된 체인만(eth/arbitrum/base).
    // 미설정 체인을 포함하면 그 체인 native 조회 실패가 디스커버리 전체를
    // 중단시켜 잔액이 0 으로 남는다.
    if (req.type === "pasu-add-wallet") {
      const r = req as PasuAddWalletRequest;
      const addBody: { address: string; chains: string[]; label?: string } = {
        address: r.address.toLowerCase(),
        chains: ["eip155:1", "eip155:42161", "eip155:8453"],
      };
      if (r.label) addBody.label = r.label;
      void bootReady
        .then(() => addWallet(addBody))
        .then((resp: AddWalletResp) => sendResponse({ ok: true, data: resp }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "pasu_add_wallet_failed", message: String(err) },
          }),
        );
      return true;
    }

    // 별칭 변경(PATCH /wallets/:addr) — 서버 라벨을 팝업과 동기화. 빈 문자열은
    // 라벨 제거(null)로 보낸다.
    if (req.type === "pasu-update-wallet") {
      const r = req as PasuUpdateWalletRequest;
      const patch: { label?: string | null } = {};
      if (r.label !== undefined) patch.label = r.label === "" ? null : r.label;
      void bootReady
        .then(() => updateWallet(r.address.toLowerCase(), patch))
        .then(() => sendResponse({ ok: true, data: null }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "pasu_update_wallet_failed", message: String(err) },
          }),
        );
      return true;
    }

    // 지갑 삭제(DELETE /wallets/:addr) — 서버에서 제거해 대시보드·팝업 일관성.
    if (req.type === "pasu-delete-wallet") {
      const r = req as PasuDeleteWalletRequest;
      void bootReady
        .then(() => deleteWallet(r.address.toLowerCase()))
        .then(() => sendResponse({ ok: true, data: null }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "pasu_delete_wallet_failed", message: String(err) },
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

    if (req.type === "state-deltas:get") {
      void getStateDelta((req as StateDeltasGetRequest).id)
        .then((row: StateDeltaRow | null) => sendResponse({ ok: true, data: row }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "state_deltas_get_failed", message: String(err) },
          }),
        );
      return true;
    }
    if (req.type === "diagnosis-context:get") {
      void getDiagnosisContext((req as DiagnosisContextGetRequest).id)
        .then((row: DiagnosisContextRow | null) =>
          sendResponse({ ok: true, data: row }),
        )
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "diagnosis_context_get_failed", message: String(err) },
          }),
        );
      return true;
    }

    if (req.type === "state-deltas:clear") {
      void clearStateDeltas()
        .then(() => sendResponse({ ok: true, data: { cleared: true } }))
        .catch((err: unknown) =>
          sendResponse({
            ok: false,
            error: { kind: "state_deltas_clear_failed", message: String(err) },
          }),
        );
      return true;
    }

    return;
  },
);
