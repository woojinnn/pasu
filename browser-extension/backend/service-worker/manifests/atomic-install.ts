// Atomic install transition for manifest maps.
//
// The two side effects — pushing manifests into the WASM engine and
// persisting them to `chrome.storage.local` — must stay in lockstep,
// otherwise a crash mid-install (or a non-throwing WASM rejection) can
// leave storage with a manifest the engine doesn't have, or vice versa.
//
// Order:
//   1. Call WASM install. If it throws → return the error and leave
//      storage untouched.
//   2. Only after WASM acknowledges success, replace the manifest map
//      and the schema-hash key in one shot.
//
// Phase-6 contract: `wasmInstall` MUST be the Map-shape install path
// (see `wasm-bridge.ts::installPolicies`). A null return means the
// caller drifted onto the legacy Vec path; we surface that as an
// `install_legacy_envelope` error and leave storage untouched rather
// than silently committing a manifest set without an enriched-schema
// hash.

import * as store from "./store";

export interface AtomicInstallOk {
  ok: true;
  data: { enrichedSchemaHash: string; addedCustomFields: Record<string, unknown[]> };
}

export interface AtomicInstallErr {
  ok: false;
  error: { kind: string; message: string };
}

export type AtomicInstallResult = AtomicInstallOk | AtomicInstallErr;

export type WasmInstallFn = (
  manifests: Record<string, store.PolicyManifest>,
) => Promise<
  | { enrichedSchemaHash: string; addedCustomFields: Record<string, unknown[]> }
  | null
>;

export interface AtomicInstallDeps {
  wasmInstall: WasmInstallFn;
}

function asError(err: unknown): { kind: string; message: string } {
  if (err && typeof err === "object") {
    const e = err as { kind?: unknown; name?: unknown; message?: unknown };
    const kind =
      typeof e.kind === "string"
        ? e.kind
        : typeof e.name === "string"
        ? (e.name as string)
        : "install_failed";
    const message = typeof e.message === "string" ? e.message : String(err);
    return { kind, message };
  }
  return { kind: "install_failed", message: String(err) };
}

/**
 * Install `next` into WASM, then (and only then) persist it.
 *
 * On failure the storage layer is left exactly as it was before the
 * call — callers can retry the previous manifest map or surface the
 * error.
 */
export async function atomicInstall(
  next: Record<string, store.PolicyManifest>,
  deps: AtomicInstallDeps,
): Promise<AtomicInstallResult> {
  let installed:
    | { enrichedSchemaHash: string; addedCustomFields: Record<string, unknown[]> }
    | null;
  try {
    installed = await deps.wasmInstall(next);
  } catch (err) {
    return { ok: false, error: asError(err) };
  }

  if (installed === null) {
    return {
      ok: false,
      error: {
        kind: "install_legacy_envelope",
        message:
          "WASM install returned the legacy null envelope — caller drifted onto the deprecated Vec manifest shape",
      },
    };
  }

  // Atomic commit: replace the manifest map and the hash. The store's
  // `replaceAllManifests` swaps the value of the single
  // `rpc:manifests` key, so even if the runtime crashes between calls
  // the hash is the only key that could lag behind.
  await store.replaceAllManifests(next);
  await store.setHash(installed.enrichedSchemaHash);

  return { ok: true, data: installed };
}
