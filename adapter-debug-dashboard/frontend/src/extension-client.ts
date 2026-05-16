// SDK for talking to the browser-extension from the dashboard SPA.
//
// Transport: window.postMessage → content-script (dashboard-bridge.ts) →
// chrome.runtime.sendMessage → SW dispatcher. Responses come back the same
// way, matched by request id. If the extension isn't installed or the bridge
// isn't injected (extension disabled, wrong origin), every call times out.
//
// Types here mirror the SW surface; they live in the extension code under
// src/background/dashboard. Keep them in sync until the workspaces are joined.

const REQ_TAG = "scopeball-dashboard";
const RES_TAG = "scopeball-extension";
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

export type Response<T> =
  | { ok: true; data: T }
  | { ok: false; error: { kind: string; message: string } };

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
  /** Subscribe to extension-side change broadcasts. Returns an unsubscribe fn. */
  onChange(cb: (keys: string[]) => void): () => void;
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
    onChange: (cb) => {
      changeListeners.add(cb);
      return () => {
        changeListeners.delete(cb);
      };
    },
  };
}
