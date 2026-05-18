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
  previewInstalledSchema,
} from "../wasm-bridge";
import { atomicInstall, type AtomicInstallResult } from "./atomic-install";
import {
  KEY_PENDING_MIGRATION,
  listPending,
  rewritePolicyText,
  setPending,
} from "./migration";
import * as store from "./store";

export type ManifestRequest =
  | { type: "manifest:preview"; action: string; manifest: unknown }
  | { type: "manifest:put"; action: string; manifest: store.PolicyManifest }
  | { type: "manifest:get"; action: string }
  | { type: "manifest:get-enriched-schema" }
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
  return installPolicies({ schema_text: "", policy_set: [], manifests });
}

async function installWith(
  next: Record<string, store.PolicyManifest>,
): Promise<AtomicInstallResult> {
  return atomicInstall(next, { wasmInstall: callWasmInstallMap });
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

      case "manifest:get-enriched-schema": {
        return { ok: true, data: await previewInstalledSchema() };
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
