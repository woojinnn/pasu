// SW-side message handlers for the manifest CRUD / schema preview /
// migration surface (Phase 6 / Task 6.5).
//
// The dashboard SDK builds messages with a `manifest:*` or `migration:*`
// `type` prefix and forwards them through the content-script bridge.
// `index.ts` routes those messages here.
//
// All handlers return the standard `{ ok, data | error }` envelope so
// the SDK can `throw` the error verbatim. The Map-shape WASM install
// path is the only one we use here — see `wasm-bridge.ts` for why.

import {
  type AliasTableEntry,
  getAliasTable,
  installPolicies,
  previewCustomSchema,
  fieldCatalog,
  previewInstalledSchema,
} from "../wasm-bridge";
import { fetchBundledDefaultManifests } from "./dev-seed";
import {
  loadCurrentEnabledPolicySet,
  reinstallAllPolicies,
} from "../policies-loader";
import {
  applyEnabledIds,
  getEnabledIds,
} from "../policy-selection";
import { atomicInstall, type AtomicInstallResult } from "./atomic-install";
import {
  KEY_PENDING_MIGRATION,
  clearOriginalEnabled,
  getOriginalEnabled,
  listPending,
  rewritePolicyText,
  setPending,
} from "./migration";
import * as store from "./store";

export type ManifestRequest =
  | { type: "manifest:preview"; action: string; manifest: unknown }
  | { type: "manifest:put"; action: string; manifest: store.PolicyManifest }
  | { type: "manifest:get"; action: string }
  | { type: "manifest:get-bundled"; action: string }
  | { type: "manifest:get-method-catalog" }
  | { type: "manifest:get-enriched-schema" }
  | { type: "manifest:get-field-catalog" }
  | { type: "manifest:ping" }
  | { type: "manifest:alias-table" }
  | { type: "manifest:set-endpoint-url"; url: string | null }
  | { type: "migration:list" }
  | {
      type: "migration:rewrite";
      id: string;
      knownFields: readonly string[];
      // The current `text` of the managed policy. The SW doesn't
      // touch storage here — the dashboard takes the rewritten text,
      // pushes it through `dashboard:put-raw`, and on success sends
      // `migration:ack` to pop the id off the pending set. Splitting
      // the two avoids a window where pending is empty but storage
      // still has v0 text.
      text: string;
    }
  | { type: "migration:ack"; id: string };

export type ManifestResponse<T = unknown> =
  | { ok: true; data: T }
  | { ok: false; error: { kind: string; message: string } };

export function isManifestRequest(value: unknown): value is ManifestRequest {
  if (!value || typeof value !== "object") return false;
  const t = (value as { type?: unknown }).type;
  return (
    typeof t === "string" &&
    (t.startsWith("manifest:") || t.startsWith("migration:"))
  );
}

function fail(kind: string, message: string): ManifestResponse {
  return { ok: false, error: { kind, message } };
}

function classify(err: unknown): { kind: string; message: string } {
  if (err && typeof err === "object") {
    const e = err as { kind?: unknown; message?: unknown; name?: unknown };
    const kind =
      typeof e.kind === "string"
        ? e.kind
        : typeof e.name === "string"
        ? (e.name as string)
        : "manifest_failed";
    const message =
      typeof e.message === "string" ? (e.message as string) : String(err);
    return { kind, message };
  }
  return { kind: "manifest_failed", message: String(err) };
}

async function callWasmInstallMap(
  manifests: Record<string, store.PolicyManifest>,
): Promise<
  | { enrichedSchemaHash: string; addedCustomFields: Record<string, unknown[]> }
  | null
> {
  // Phase 7 codex carry-over H follow-up: the previous implementation
  // passed `policy_set: []`, which silently wiped every installed
  // Cedar policy on each manifest:put. `install_policies_json`
  // *replaces* engine state — so handing it an empty policy set is the
  // same as `engine.clear_policies()`. We mirror `hydrate.ts`'s pattern
  // and forward the currently-enabled policy set so the engine keeps
  // serving real verdicts while the manifest map turns over.
  const policySet = await loadCurrentEnabledPolicySet();
  return installPolicies({
    schema_text: "",
    policy_set: policySet,
    manifests,
  });
}

async function installWith(
  next: Record<string, store.PolicyManifest>,
): Promise<AtomicInstallResult> {
  return atomicInstall(next, { wasmInstall: callWasmInstallMap });
}

/**
 * Hybrid method catalog discovery for the manifest editor.
 *
 * Reads the bundled `method-catalog.json` and merges any catalog the
 * configured policy-rpc daemon exposes over `GET /v1/methods`. The
 * dynamic catalog wins on key collision so:
 *  - A newer daemon catalog (post-extension-build update) surfaces
 *    correctly.
 *  - Plugin and sidecar methods added at daemon startup show up
 *    alongside the bundled set.
 *  - Existing bundled-only callers never break: empty merge is a
 *    no-op.
 *
 * Returns `{ methods: {} }` on total failure (bundle missing AND
 * daemon unreachable) so the manifest editor degrades to free-text
 * mode instead of crashing.
 */
async function fetchHybridMethodCatalog(): Promise<{
  methods: Record<string, unknown>;
}> {
  const Browser = (await import("webextension-polyfill")).default;
  let bundled: { methods: Record<string, unknown> } = { methods: {} };

  // 1) Bundled catalog from extension assets.
  try {
    const url = Browser.runtime.getURL("method-catalog.json");
    const response = await fetch(url);
    if (response.ok) {
      const raw = (await response.json()) as { methods?: Record<string, unknown> };
      if (raw && raw.methods && typeof raw.methods === "object") {
        bundled = { methods: raw.methods };
      }
    }
  } catch {
    // No bundled catalog (release build skipped copy, dev forgot to run
    // copy-method-catalog.js, etc.) — proceed with empty so the daemon
    // catalog can still seed the UI.
  }

  // 2) Optional dynamic catalog from the configured daemon.
  let dynamic: { methods: Record<string, unknown> } = { methods: {} };
  const endpointUrl = await store.getEndpointUrl();
  if (endpointUrl) {
    try {
      const url = `${endpointUrl.replace(/\/+$/, "")}/v1/methods`;
      const response = await fetch(url);
      if (response.ok) {
        const raw = (await response.json()) as {
          catalog?: { methods?: Record<string, unknown> };
          methods?: Record<string, unknown> | string[];
        };
        // Newer daemon shape: `{ methods: [...], catalog: { methods: {...} } }`.
        // Older daemon shape (pre-Phase-8.5): `{ methods: [...] }` — no
        // catalog map, treat as empty.
        if (raw.catalog && raw.catalog.methods && typeof raw.catalog.methods === "object") {
          dynamic = { methods: raw.catalog.methods };
        } else if (
          raw.methods &&
          typeof raw.methods === "object" &&
          !Array.isArray(raw.methods)
        ) {
          dynamic = { methods: raw.methods };
        }
      }
    } catch {
      // Daemon down / network error — silent, dashboard already exposes
      // a separate "endpoint health" indicator (manifest:ping).
    }
  }

  return { methods: { ...bundled.methods, ...dynamic.methods } };
}

async function pingEndpoint(): Promise<ManifestResponse> {
  const url = await store.getEndpointUrl();
  if (!url) {
    return { ok: true, data: { reachable: false, url: null } };
  }
  try {
    const response = await fetch(`${url.replace(/\/+$/, "")}/v1/healthz`, {
      method: "GET",
    });
    return {
      ok: true,
      data: { reachable: response.ok, url, status: response.status },
    };
  } catch (err) {
    return {
      ok: true,
      data: {
        reachable: false,
        url,
        message: err instanceof Error ? err.message : String(err),
      },
    };
  }
}

export async function handleManifestRequest(
  req: ManifestRequest,
): Promise<ManifestResponse> {
  try {
    switch (req.type) {
      case "manifest:preview": {
        const out = await previewCustomSchema({
          action: req.action,
          manifest: req.manifest,
        });
        return { ok: true, data: out };
      }

      case "manifest:put": {
        if (typeof req.action !== "string" || !req.manifest) {
          return fail("invalid_request", "action and manifest required");
        }
        const next = {
          ...(await store.getAllManifests()),
          [req.action]: req.manifest,
        };
        return await installWith(next);
      }

      case "manifest:get": {
        return {
          ok: true,
          data: { manifest: await store.getManifest(req.action) },
        };
      }

      case "manifest:get-bundled": {
        // Reads from the static asset bundle (`public/default-manifests/`)
        // rather than chrome.storage — the bundled set is the "starter
        // pack" we ship in the extension binary, never user state. The
        // helper returns `{}` when no bundle was copied for this build,
        // in which case there's no starter pack for the caller to install.
        const bundled = await fetchBundledDefaultManifests();
        return {
          ok: true,
          data: { manifest: bundled[req.action] ?? null },
        };
      }

      case "manifest:get-method-catalog": {
        // Hybrid catalog discovery (Phase 8.5):
        //   1. Bundled `method-catalog.json` from extension assets — always
        //      available, ships with the extension build.
        //   2. Optional dynamic `GET /v1/methods` from the configured
        //      policy-rpc daemon. When present, its entries OVERRIDE
        //      the bundled set on key collision — that's how a daemon
        //      built from newer source than the extension surfaces its
        //      latest catalog without an extension reinstall, and how
        //      plugin/sidecar methods reach the dashboard's editor.
        const catalog = await fetchHybridMethodCatalog();
        return { ok: true, data: catalog };
      }

      case "manifest:get-enriched-schema": {
        return { ok: true, data: await previewInstalledSchema() };
      }

      case "manifest:get-field-catalog": {
        return { ok: true, data: await fieldCatalog() };
      }

      case "manifest:ping": {
        return await pingEndpoint();
      }

      case "manifest:alias-table": {
        return {
          ok: true,
          data: (await getAliasTable()) as { entries: AliasTableEntry[] },
        };
      }

      case "manifest:set-endpoint-url": {
        // Phase 7 codex carry-over M: server-side URL scheme check.
        // The dashboard already validates this client-side, but the SDK
        // is reachable from any content script / Vite test and we
        // cannot rely on the caller honouring the contract — only
        // `http(s)://...` (or `null` to clear) should land in storage.
        const url = typeof req.url === "string" ? req.url.trim() : null;
        if (url !== null && url !== "") {
          if (!/^https?:\/\/[^\s]+/i.test(url)) {
            return fail(
              "invalid_endpoint_url",
              `endpoint URL must use http:// or https://, got: ${url.slice(0, 40)}`,
            );
          }
        }
        await store.setEndpointUrl(url && url.length > 0 ? url : null);
        return { ok: true, data: { url: await store.getEndpointUrl() } };
      }

      case "migration:list": {
        return { ok: true, data: { ids: await listPending() } };
      }

      case "migration:rewrite": {
        if (typeof req.id !== "string" || typeof req.text !== "string") {
          return fail("invalid_request", "id and text required");
        }
        const rewritten = rewritePolicyText(req.text, req.knownFields ?? []);
        // `rewritten === req.text` means there was nothing to rewrite.
        // Auto-ack in that case since there's no follow-up put-raw to
        // wait for — the policy is already on the v1 layout.
        if (rewritten === req.text) {
          const pending = await listPending();
          await setPending(pending.filter((p) => p !== req.id));
          return { ok: true, data: { id: req.id, rewritten, applied: false } };
        }
        // `applied: true` only signals the rewrite produced a different
        // string. The id stays on the pending set until the dashboard
        // confirms with `migration:ack` after `dashboard:put-raw`
        // succeeds. Splitting the two avoids a window where pending is
        // empty but the managed-policy entry still has v0 text.
        return { ok: true, data: { id: req.id, rewritten, applied: true } };
      }

      case "migration:ack": {
        if (typeof req.id !== "string") {
          return fail("invalid_request", "id required");
        }
        // Fix R, ack-side restore. The dashboard's rewrite flow runs
        // `migration:rewrite` → `dashboard:put-raw` → `migration:ack`.
        // The put-raw step always re-adds the id to `enabled-ids` (see
        // `persistThenApply` → `autoApplyEnabled`). For users whose
        // original preference was DISABLED, that's wrong — we strip
        // the id off again here and reinstall via the apply-queue so
        // BOTH `enabled-ids` AND `applied-ids` move in lockstep and
        // serialize against any concurrent popup toggle. For users
        // whose original preference was ENABLED (or who never had a
        // snapshot because the detector didn't run for this id),
        // put-raw's add is exactly right and we leave enabled-ids alone.
        const original = await getOriginalEnabled();
        const wasEnabled = original[req.id];
        if (wasEnabled === false) {
          const current = await getEnabledIds();
          if (current.includes(req.id)) {
            const next = current.filter((x) => x !== req.id);
            const result = await applyEnabledIds(next, reinstallAllPolicies);
            if (!result.ok) {
              // Surface the apply-queue failure verbatim. Leaving the
              // snapshot + pending entry in place lets the next ack
              // retry pick up where we stopped.
              return { ok: false, error: result.error };
            }
          }
        }
        await clearOriginalEnabled(req.id);
        const pending = await listPending();
        await setPending(pending.filter((p) => p !== req.id));
        return { ok: true, data: { id: req.id, remaining: await listPending() } };
      }

      default: {
        const _exhaustive: never = req;
        void _exhaustive;
        return fail("unknown_request", `unrecognized manifest request`);
      }
    }
  } catch (err) {
    return { ok: false, error: classify(err) };
  }
}

// Re-exported so `index.ts` can route raw `Browser.runtime.onMessage`
// payloads here without re-importing the pending key.
export { KEY_PENDING_MIGRATION };
