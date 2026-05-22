import Browser from "webextension-polyfill";
import type { ParamsSchema, ParamValues } from "../marketplace/params-validator";
import type { RenderedPolicyEntry } from "../marketplace/storage";

// chrome.storage.local quota is 5–10 MB depending on the browser, but per-item
// 'large data' performance falls off a cliff well before that. Cap individual
// policy bodies at 32 KiB and total entries at 200 so a misbehaving dashboard
// can't drive the SW into quota errors. Beyond those caps the writer rejects.
const KEY = "dashboard:policies";
export const DASHBOARD_ID_PREFIX = "dashboard::";
export const DASHBOARD_ID_RE = /^dashboard::[A-Za-z0-9_./()-]{1,128}$/;
export const MAX_TEXT_BYTES = 32_768;
export const MAX_ENTRIES = 200;

export interface ManagedPolicyTemplateMeta {
  source: string;
  paramsSchema: ParamsSchema;
  paramValues: ParamValues;
}

export interface ManagedPolicy {
  id: string;
  kind: "raw" | "template";
  /** For 'raw': original text. For 'template': rendered text. */
  text: string;
  template?: ManagedPolicyTemplateMeta;
  manifest?: unknown;
  manifests?: readonly unknown[];
  updatedAtMs: number;
  schemaVersion: 1;
}

function utf8ByteLength(s: string): number {
  // TextEncoder is available in both SW and JSDOM test envs.
  return new TextEncoder().encode(s).length;
}

function assertValidId(id: string): void {
  if (!DASHBOARD_ID_RE.test(id)) {
    throw new Error(
      `invalid_id: dashboard policy id must match ${DASHBOARD_ID_RE} (got "${id}")`,
    );
  }
}

function assertWithinCaps(text: string, listLengthAfter: number): void {
  const bytes = utf8ByteLength(text);
  if (bytes > MAX_TEXT_BYTES) {
    throw new Error(
      `text_too_large: policy body is ${bytes} bytes, max ${MAX_TEXT_BYTES}`,
    );
  }
  if (listLengthAfter > MAX_ENTRIES) {
    throw new Error(
      `too_many_entries: dashboard already stores ${MAX_ENTRIES} policies; ` +
        `delete one before adding more`,
    );
  }
}

export async function listManaged(): Promise<ManagedPolicy[]> {
  const v = ((await Browser.storage.local.get(KEY)) as Record<string, unknown>)[
    KEY
  ] as ManagedPolicy[] | undefined;
  return v ?? [];
}

export async function upsertManaged(p: ManagedPolicy): Promise<void> {
  assertValidId(p.id);
  const list = await listManaged();
  const idx = list.findIndex((x) => x.id === p.id);
  const next = list.slice();
  if (idx >= 0) {
    next[idx] = p;
  } else {
    next.push(p);
  }
  assertWithinCaps(p.text, next.length);
  await Browser.storage.local.set({ [KEY]: next });
}

export async function deleteManaged(id: string): Promise<void> {
  const list = await listManaged();
  await Browser.storage.local.set({
    [KEY]: list.filter((p) => p.id !== id),
  });
}

/** Loader-facing projection. Mirrors the shape that
 *  marketplace `aggregatedPolicySet` returns so `policies-loader` can union
 *  defaults ∪ marketplace ∪ dashboard with one filter pass. */
export async function aggregatedManagedPolicySet(): Promise<
  RenderedPolicyEntry[]
> {
  const list = await listManaged();
  return list.map((p) => ({
    id: p.id,
    text: p.text,
    ...(p.manifest !== undefined ? { manifest: p.manifest } : {}),
    ...(p.manifests !== undefined ? { manifests: p.manifests } : {}),
  }));
}
