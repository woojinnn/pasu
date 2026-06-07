// SDK for talking to the browser-extension from the dashboard SPA.
//
// Transport: window.postMessage → content-script (dashboard-bridge.ts) →
// chrome.runtime.sendMessage → SW dispatcher. Responses come back the same
// way, matched by request id. If the extension isn't installed or the bridge
// isn't injected (extension disabled, wrong origin), every call times out.
//
// Types here mirror the SW surface; they live in the extension code under
// src/background/dashboard. Keep them in sync until the workspaces are joined.

const REQ_TAG = "pasu-dashboard";
const RES_TAG = "pasu-extension";
const BROADCAST_ID = "__broadcast__";
const DEFAULT_TIMEOUT_MS = 5_000;

export type Severity = "deny" | "warn" | "unknown";

export interface CatalogPolicy {
  id: string;
  rules: { severity: Severity; reason: string }[];
  dominantSeverity: Severity;
  sourceLabel: string;
}

export interface Catalog {
  policies: CatalogPolicy[];
  enabled: string[];
  applied: string[];
}

export type ApplyResult =
  | { ok: true }
  | { ok: false; error: { kind: string; message: string } };

export type ParamSchema =
  | { type: "integer"; min: number; max: number; default?: number }
  | { type: "address"; default?: string }
  | { type: "enum"; values: readonly string[]; default?: string }
  | { type: "string"; maxLen: number; allowedChars: string; default?: string }
  | {
      type: "array";
      items: ParamSchema;
      maxItems: number;
      default?: unknown[];
    };

export type ParamsSchema = Record<string, ParamSchema>;
export type ParamValues = Record<string, unknown>;

export interface ManagedPolicy {
  id: string;
  kind: "raw" | "template";
  text: string;
  template?: {
    source: string;
    paramsSchema: ParamsSchema;
    paramValues: ParamValues;
  };
  manifest?: unknown;
  manifests?: readonly unknown[];
  updatedAtMs: number;
  schemaVersion: 1;
}

// Mirrors `AuditEntry` in backend/service-worker/storage.ts. Kept here so
// the dashboard doesn't have to import a path outside its workspace.
export type VerdictKind = "pass" | "warn" | "fail";

export interface AuditMatchedPolicy {
  id: string;
  severity: string;
  /**
   * Phase 6 / D9: present when the engine surfaces a runtime failure
   * (e.g. an unreachable policy-rpc endpoint). The matched entry's
   * `id` is the `__system__` sentinel and `reason` carries the WASM-
   * provided diagnostic (e.g. `"rpc-unavailable: <call-id>"`).
   * Ordinary policy matches don't include this field.
   */
  reason?: string;
}

export interface AuditPolicyRpc {
  request_id: string;
  manifest_set_hash: string;
  schema_hash: string;
  call_ids: string[];
  methods: string[];
}

export interface AuditEntry {
  requestId: string;
  hostname: string;
  type: "transaction" | "typed-signature" | "untyped-signature";
  bypassed: boolean;
  verdict: VerdictKind;
  matchedPolicies: AuditMatchedPolicy[];
  policyRpc?: AuditPolicyRpc;
  decidedAtMs: number;
}

export interface AuditQuery {
  /** Cap on number of entries returned. Server-side max is 200. */
  limit?: number;
  /** Only entries with `decidedAtMs >= since`. Unix ms. */
  since?: number;
}

export type Response<T> =
  | { ok: true; data: T }
  | { ok: false; error: { kind: string; message: string } };

// Phase 6 / Task 6.5: manifest-driven cedarschema SDK surface.
//
// Mirrors `PolicyManifest` on the WASM side. Lives in the SDK so the
// dashboard can construct manifest objects without importing types from
// the extension workspace.
export interface PolicyManifest {
  id: string;
  schema_version: number;
  requires: unknown[];
  context_extensions?: Record<string, Record<string, string>>;
}

export interface PreviewManifestOutput {
  customTypes: { name: string; fields: unknown[] }[];
  enrichedSchemaText: string;
  diff: { added: unknown[]; removed: unknown[]; changed: unknown[] };
  schemaHash: string;
}

/**
 * One manifest-contributed custom-context field, as returned by
 * `preview_installed_schema_json`. Field names are snake_case because the
 * engine's `CustomFieldSource` derives serde defaults without renaming.
 *
 * UIs typically care about `field` + `cedar_type`; the `source_*` columns
 * are provenance breadcrumbs (which requirement / RPC method introduced
 * the field), useful for debugging mismatches between the installed
 * manifest map and the visible schema.
 */
export interface CustomContextField {
  field: string;
  cedar_type: string;
  source_requirement_id: string;
  source_method: string;
  source_from: string;
  requirement_optional: boolean;
}

export interface EnrichedSchemaOutput {
  schema_text: string;
  schema_hash: string;
  added_fields: unknown[];
  customContexts: Record<string, CustomContextField[]>;
  schemaHash: string;
}

export interface AliasTableEntry {
  name: string;
  kind: "scalar" | "record";
  cedarSpelling: string;
}

export interface PingResult {
  reachable: boolean;
  url: string | null;
  status?: number;
  message?: string;
}

export interface ManifestPutResult {
  enrichedSchemaHash: string;
  addedCustomFields: Record<string, unknown[]>;
}

export interface MigrationRewriteResult {
  id: string;
  rewritten: string;
  applied: boolean;
}

/**
 * V0 (pre-Phase-5) enrichment field names that lived at top-level
 * `context.<field>` before the schema moved them under
 * `context.custom.<field>`. Both the dashboard's rewrite banner and the
 * SW-side migration detector must agree on this set, so it lives here
 * — the only module both build graphs reach (dashboard via the
 * `@pasu/sdk` path alias, SW via a relative import).
 *
 * Used by:
 *  - `dashboard/src/migration/rewrite-banner.tsx` — passes to
 *    `migration:rewrite` so `rewritePolicyText` only substitutes known
 *    fields.
 *  - `backend/service-worker/manifests/migration-detector.ts` — scans
 *    managed-policy texts for `context.<field>` references and queues
 *    matching ids into `migration:pending`.
 */
export const V0_KNOWN_FIELDS: readonly string[] = [
  "totalInputUsd",
  "totalMinOutputUsd",
  "effectiveRateVsOracleBps",
  "totalInputFractionOfPortfolioBps",
  "windowStats",
  "validityDeltaSec",
  "recipientIsContract",
];

// ── Method catalog (Phase 8.5) ────────────────────────────────────
//
// The dashboard's manifest editor reads the catalog to drive its
// method/param/output dropdowns. Two discovery paths:
//   1. Static bundle — `method-catalog.json` shipped in the
//      extension assets, fetched via `Browser.runtime.getURL`.
//   2. Dynamic from the configured policy-rpc daemon — `GET /v1/methods`,
//      which may add plugin/sidecar methods on top of the bundled set.
// `client.getMethodCatalog()` returns the merged view (dynamic wins on
// conflicts so a daemon that updates its bundled methods through a
// newer version still surfaces correctly).

/** Cedar-aligned type spellings the catalog declares for params/returns. */
export type CatalogScalarType =
  | "Long"
  | "String"
  | "Bool"
  | "decimal"
  | "Set<String>";

export type CatalogRecordType =
  | "UsdValuation"
  | "WindowStats"
  | "Validity"
  | "AssetRef"
  | "AmountConstraint"
  | "AssetRefWithAmountConstraint"
  | "TickRange"
  | "Pool";

export type CatalogType = CatalogScalarType | CatalogRecordType;

export interface MethodParamSpec {
  type: CatalogType;
  required: boolean;
  description?: string;
  defaultSelector?: string;
  enum_?: readonly string[];
  default?: string | number | boolean;
}

export type MethodReturnSpec =
  | { kind: "record"; type: CatalogRecordType }
  | { kind: "scalar"; type: CatalogScalarType; from: string };

export interface MethodCatalogEntry {
  name: string;
  description?: string;
  params: Record<string, MethodParamSpec>;
  returns: MethodReturnSpec;
  /**
   * `bundled` — shipped with the daemon binary.
   * `plugin`  — loaded from the daemon's `plugins/` dir (X-2).
   * `sidecar` — discovered via HTTP from a configured external daemon (X-1).
   * UI uses this to badge entries distinctly.
   */
  origin: "bundled" | "plugin" | "sidecar";
}

export interface MethodCatalog {
  methods: Record<string, MethodCatalogEntry>;
}

/**
 * Fields that moved FROM the optional `context.custom.<field>` extension
 * back DOWN to the base `context.<field>` shape during Phase 8. The
 * engine populates them itself during calldata lowering instead of via
 * a manifest enrichment — see
 * `policy-engine/src/lowering/dex/swap.rs::nano_amount`.
 *
 * Policies authored before Phase 8 may reference these under the old
 * `context.custom.*` path. Both shapes parse via the policy-builder
 * parser (the schema lookup canonicalizes the path on either side), but
 * the OLD STRING TEXT installed in the engine will fail strict cedar
 * schema validation after a re-install because `SwapCustomContext` no
 * longer carries these fields. A future banner can use
 * `migration::rewritePolicyTextCustomToBase` with this list to fix the
 * installed text.
 */
export const V1_TO_V2_AMOUNT_FIELDS: readonly string[] = [
  "inputAmountNano",
  "outputAmountNano",
];

export interface ExtensionClient {
  ping(): Promise<{ version: number }>;
  getCatalog(): Promise<Catalog>;
  listManaged(): Promise<ManagedPolicy[]>;
  putRaw(args: {
    id: string;
    text: string;
    manifest?: unknown;
    manifests?: readonly unknown[];
  }): Promise<{ policy: ManagedPolicy; catalog: Catalog }>;
  putTemplate(args: {
    id: string;
    templateText: string;
    paramsSchema: ParamsSchema;
    paramValues: ParamValues;
    manifest?: unknown;
    manifests?: readonly unknown[];
  }): Promise<{ policy: ManagedPolicy; catalog: Catalog }>;
  delete(id: string): Promise<{ catalog: Catalog }>;
  setEnabledIds(ids: string[]): Promise<{ catalog: Catalog }>;
  /** Recent verdict history (Pass/Warn/Fail). Ordered most-recent-first. */
  getAuditLog(opts?: AuditQuery): Promise<AuditEntry[]>;
  /** Subscribe to extension-side change broadcasts. Returns an unsubscribe fn. */
  onChange(cb: (keys: string[]) => void): () => void;

  // ── Phase 6 / Task 6.5: manifest-driven cedarschema surface ────────────
  /**
   * Compose the enriched cedarschema for one action's manifest without
   * installing it. Returns the per-action custom fields, the generated
   * cedarschema text, a diff against any currently-installed action,
   * and a `schemaHash` for the previewed schema.
   */
  previewManifest(
    action: string,
    manifest: PolicyManifest,
  ): Promise<PreviewManifestOutput>;
  /**
   * Install a manifest for `action` into the engine. The full map is
   * replaced atomically — other actions stay as-is.
   */
  putManifest(
    action: string,
    manifest: PolicyManifest,
  ): Promise<ManifestPutResult>;
  /** Read back one stored manifest (or `null` when absent). */
  getManifest(action: string): Promise<{ manifest: PolicyManifest | null }>;
  /**
   * Fetch the bundled "starter pack" manifest for `action` from the
   * extension's static assets (`public/default-manifests/`). Phase 8
   * stopped auto-seeding these into storage on first boot — the
   * `/manifests/<action>` page now uses this method to power the
   * "Install starter pack" button (explicit user opt-in).
   *
   * Returns `null` when no starter pack ships for the action (e.g. a
   * release build that didn't run `copy-default-manifests.js`).
   */
  getBundledManifest(
    action: string,
  ): Promise<{ manifest: PolicyManifest | null }>;
  /**
   * Hybrid catalog discovery (Phase 8.5):
   *   1. Read the bundled `method-catalog.json` from extension assets.
   *   2. If a policy-rpc endpoint URL is configured, fetch its
   *      `GET /v1/methods` and merge — dynamic wins on key conflicts.
   * Used by the manifest editor to drive method/param/output dropdowns.
   *
   * Returns an empty catalog (`{ methods: {} }`) on total failure so
   * the UI degrades gracefully (free-text mode) instead of crashing.
   */
  getMethodCatalog(): Promise<MethodCatalog>;
  /** Read back the currently-installed enriched cedarschema. */
  getEnrichedSchema(): Promise<EnrichedSchemaOutput>;
  /** Ping the configured policy-rpc endpoint's `/v1/healthz` URL. */
  pingRpcEndpoint(): Promise<PingResult>;
  /**
   * Set the policy-rpc endpoint URL on the SW storage layer. Pass `null`
   * to clear. The SW also validates the scheme; the dashboard rejects
   * non-`http(s)` schemes client-side before invoking this, but we keep
   * the SDK signature permissive so unit tests can exercise the SW path.
   */
  setEndpointUrl(url: string | null): Promise<{ url: string | null }>;
  /** Read the base alias table the engine ships with. */
  getAliasTable(): Promise<{ entries: AliasTableEntry[] }>;
  /** Ids of managed policies awaiting v0 → v1 migration. */
  listMigrationPending(): Promise<{ ids: string[] }>;
  /**
   * Rewrite a managed policy from `context.<x>` to `context.custom.<x>`.
   *
   * This does NOT pop the id off the pending-migration queue. After the
   * caller persists the rewritten text via `putRaw` and the engine
   * accepts the install, send `migrationAck(id)` to finish the
   * migration. Splitting the two avoids leaving the migration queue
   * empty while storage still holds v0 text (e.g. tab closed mid-flight).
   */
  rewritePolicyToCustom(args: {
    id: string;
    text: string;
    knownFields: readonly string[];
  }): Promise<MigrationRewriteResult>;
  /** Pop a migrated policy id off the pending queue. */
  migrationAck(id: string): Promise<{ id: string; remaining: string[] }>;
}

export interface ClientOptions {
  timeoutMs?: number;
}

interface PendingRequest {
  resolve: (value: unknown) => void;
  reject: (reason: unknown) => void;
  timeout: ReturnType<typeof setTimeout>;
}

function randomId(): string {
  if (
    typeof crypto !== "undefined" &&
    typeof crypto.randomUUID === "function"
  ) {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function createExtensionClient(
  opts: ClientOptions = {},
): ExtensionClient {
  const timeoutMs = opts.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const pending = new Map<string, PendingRequest>();
  const changeListeners = new Set<(keys: string[]) => void>();

  function onMessage(event: MessageEvent): void {
    if (event.source !== window) return;
    const data = event.data as
      | {
          source?: unknown;
          id?: unknown;
          response?: unknown;
          event?: unknown;
          keys?: unknown;
        }
      | undefined;
    if (!data || data.source !== RES_TAG) return;

    if (data.id === BROADCAST_ID && data.event === "changed") {
      const keys = Array.isArray(data.keys)
        ? (data.keys.filter((k) => typeof k === "string") as string[])
        : [];
      for (const cb of changeListeners) cb(keys);
      return;
    }

    if (typeof data.id !== "string") return;
    const slot = pending.get(data.id);
    if (!slot) return;
    pending.delete(data.id);
    clearTimeout(slot.timeout);
    slot.resolve(data.response);
  }

  window.addEventListener("message", onMessage);

  async function request<T>(payload: unknown): Promise<T> {
    const id = randomId();
    const response = await new Promise<Response<T>>((resolve, reject) => {
      const timeout = setTimeout(() => {
        pending.delete(id);
        reject(
          new Error(
            `extension_timeout: no response from extension within ${timeoutMs}ms`,
          ),
        );
      }, timeoutMs);
      pending.set(id, {
        resolve: resolve as (v: unknown) => void,
        reject,
        timeout,
      });
      window.postMessage({ source: REQ_TAG, id, payload }, location.origin);
    });
    if (!response.ok) {
      throw Object.assign(
        new Error(`${response.error.kind}: ${response.error.message}`),
        response.error,
      );
    }
    return response.data;
  }

  return {
    ping: () => request<{ version: number }>({ type: "dashboard:ping" }),
    getCatalog: () => request<Catalog>({ type: "dashboard:get-catalog" }),
    listManaged: () =>
      request<ManagedPolicy[]>({ type: "dashboard:list-managed" }),
    putRaw: ({ id, text, manifest, manifests }) =>
      request<{ policy: ManagedPolicy; catalog: Catalog }>({
        type: "dashboard:put-raw",
        id,
        text,
        ...(manifest !== undefined ? { manifest } : {}),
        ...(manifests !== undefined ? { manifests } : {}),
      }),
    putTemplate: ({
      id,
      templateText,
      paramsSchema,
      paramValues,
      manifest,
      manifests,
    }) =>
      request<{ policy: ManagedPolicy; catalog: Catalog }>({
        type: "dashboard:put-template",
        id,
        templateText,
        paramsSchema,
        paramValues,
        ...(manifest !== undefined ? { manifest } : {}),
        ...(manifests !== undefined ? { manifests } : {}),
      }),
    delete: (id) =>
      request<{ catalog: Catalog }>({ type: "dashboard:delete", id }),
    setEnabledIds: (ids) =>
      request<{ catalog: Catalog }>({
        type: "dashboard:set-enabled-ids",
        ids,
      }),
    getAuditLog: (opts) =>
      request<AuditEntry[]>({
        type: "dashboard:get-audit-log",
        ...(opts !== undefined ? { opts } : {}),
      }),
    onChange: (cb) => {
      changeListeners.add(cb);
      return () => {
        changeListeners.delete(cb);
      };
    },

    // ── Phase 6 / Task 6.5: manifest CRUD + schema preview + migration ───
    previewManifest: (action, manifest) =>
      request<PreviewManifestOutput>({
        type: "manifest:preview",
        action,
        manifest,
      }),
    putManifest: (action, manifest) =>
      request<ManifestPutResult>({
        type: "manifest:put",
        action,
        manifest,
      }),
    getManifest: (action) =>
      request<{ manifest: PolicyManifest | null }>({
        type: "manifest:get",
        action,
      }),
    getBundledManifest: (action) =>
      request<{ manifest: PolicyManifest | null }>({
        type: "manifest:get-bundled",
        action,
      }),
    getMethodCatalog: () =>
      request<MethodCatalog>({ type: "manifest:get-method-catalog" }),
    getEnrichedSchema: () =>
      request<EnrichedSchemaOutput>({ type: "manifest:get-enriched-schema" }),
    pingRpcEndpoint: () =>
      request<PingResult>({ type: "manifest:ping" }),
    setEndpointUrl: (url) =>
      request<{ url: string | null }>({
        type: "manifest:set-endpoint-url",
        url,
      }),
    getAliasTable: () =>
      request<{ entries: AliasTableEntry[] }>({
        type: "manifest:alias-table",
      }),
    listMigrationPending: () =>
      request<{ ids: string[] }>({ type: "migration:list" }),
    rewritePolicyToCustom: ({ id, text, knownFields }) =>
      request<MigrationRewriteResult>({
        type: "migration:rewrite",
        id,
        text,
        knownFields,
      }),
    migrationAck: (id) =>
      request<{ id: string; remaining: string[] }>({
        type: "migration:ack",
        id,
      }),
  };
}
