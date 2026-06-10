// Storage layer for the manifest-driven cedarschema feature.
//
// Three keys in `chrome.storage.local`:
// - `rpc:endpointUrl`        — string | null, the active policy-rpc URL.
// - `rpc:manifests`          — Record<action, PolicyManifest>, keyed by
//                              action snake_case (e.g. "swap").
// - `rpc:enrichedSchemaHash` — string | null, last enrichedSchemaHash
//                              returned by the WASM install path.
//
// Atomicity and WASM install ordering live in `atomic-install.ts`. Do not
// write `rpc:manifests` directly — use `atomicInstall` to keep storage
// consistent with the WASM engine.

import Browser from "webextension-polyfill";

export const KEY_ENDPOINT_URL = "rpc:endpointUrl";
export const KEY_MANIFESTS = "rpc:manifests";
export const KEY_HASH = "rpc:enrichedSchemaHash";

/**
 * Minimal mirror of `policy_engine::policy_rpc::PolicyManifest`. Field
 * names match the JSON wire shape produced by the dashboard SDK and
 * consumed by WASM `install_policies_json`.
 */
export interface PolicyManifest {
  id: string;
  schema_version: number;
  requires: unknown[];
  context_extensions?: Record<string, Record<string, string>>;
}

async function readKey<T>(key: string): Promise<T | null> {
  const record = (await Browser.storage.local.get(key)) as Record<string, unknown>;
  const value = record[key];
  return value === undefined ? null : (value as T);
}

export async function getEndpointUrl(): Promise<string | null> {
  return readKey<string>(KEY_ENDPOINT_URL);
}

export async function setEndpointUrl(url: string | null): Promise<void> {
  if (url === null) {
    await Browser.storage.local.remove(KEY_ENDPOINT_URL);
    return;
  }
  await Browser.storage.local.set({ [KEY_ENDPOINT_URL]: url });
}

export async function getHash(): Promise<string | null> {
  return readKey<string>(KEY_HASH);
}

export async function setHash(hash: string | null): Promise<void> {
  if (hash === null) {
    await Browser.storage.local.remove(KEY_HASH);
    return;
  }
  await Browser.storage.local.set({ [KEY_HASH]: hash });
}

export async function getAllManifests(): Promise<Record<string, PolicyManifest>> {
  const stored = await readKey<Record<string, PolicyManifest>>(KEY_MANIFESTS);
  return stored ?? {};
}

export async function getManifest(action: string): Promise<PolicyManifest | null> {
  const all = await getAllManifests();
  return all[action] ?? null;
}

export async function putManifestRaw(
  action: string,
  manifest: PolicyManifest,
): Promise<void> {
  const next = { ...(await getAllManifests()), [action]: manifest };
  await Browser.storage.local.set({ [KEY_MANIFESTS]: next });
}

/**
 * Overwrite the manifest map wholesale. Used by `atomic-install.ts`
 * after the WASM install succeeds — atomically replacing the map keeps
 * storage in lockstep with WASM rather than emitting one set() call per
 * action and risking torn state on a crash mid-write.
 */
export async function replaceAllManifests(
  next: Record<string, PolicyManifest>,
): Promise<void> {
  await Browser.storage.local.set({ [KEY_MANIFESTS]: next });
}

/**
 * Commit the manifest map and its enriched-schema hash in a single
 * `chrome.storage.local.set` call. `set` accepts a multi-key object and
 * applies it atomically, so callers (notably `atomicInstall`) avoid the
 * window where the map persists with a stale hash if the second write
 * throws.
 *
 * If `next` is empty AND `hash` is null the call is still issued so the
 * underlying storage reflects the cleared state; tests and dev-seed rely
 * on that being observable.
 */
export async function commitManifestsAndHash(
  next: Record<string, PolicyManifest>,
  hash: string,
): Promise<void> {
  await Browser.storage.local.set({
    [KEY_MANIFESTS]: next,
    [KEY_HASH]: hash,
  });
}

/**
 * Reset every key this module owns. Used by tests and by the dev-build
 * "wipe and reseed" path. Does NOT clear other extension storage.
 */
export async function clearAll(): Promise<void> {
  await Browser.storage.local.remove([KEY_ENDPOINT_URL, KEY_MANIFESTS, KEY_HASH]);
}
