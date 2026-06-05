/**
 * Phase 1 / P2 — default v2 policy set loader (ADDITIVE).
 *
 * v2 policy evaluation (`evaluate_action_v2_json`) is STATELESS: each call
 * takes its policy `bundles` INLINE and composes their per-policy schema
 * internally. There is NO install step — unlike the v1 stateful path in
 * `policies-loader.ts` (`install_policies_json` / `installFiltered`, which
 * "REPLACE engine state"). So this module does NOT touch WASM at boot; it
 * just fetches the shipped `default-policies/policy-set-v2.json` asset once
 * and HOLDS it in a module-level cache that a future `evaluateActionV2`
 * orchestrator call site can read synchronously.
 *
 * Mirrors `loadDefaultPolicySet()` (the fetch) + `ensureSeedBundlesInstalled()`
 * (the cached boot-latch). Best-effort: a fetch failure logs + yields `[]` so
 * it can never brick SW boot.
 *
 * Do NOT route these bundles through `installPolicies`/`installFiltered` and
 * do NOT pass a `schema_text` — both belong to the v1 path and would clobber
 * or collide with v2's per-call schema composition.
 *
 * ## Dashboard policies (Option B)
 *
 * Beyond the shipped/baked set, the user-authored policies the dashboard
 * (editor-v7) saves to `chrome.storage.local["dashboard:policies"]` are merged
 * in here so they are actually ENFORCED on real verdicts — but only the ones
 * the user has ENABLED (`policy-selection:enabled-ids`); a policy toggled off in
 * the popup is excluded, mirroring the v1 `installFiltered` gate. Each enabled
 * `ManagedPolicy` is projected to a `V2Bundle` with a SYNTHESIZED minimal manifest
 * (`{ id, schema_version: 2 }`): an empty trigger matches every action, so the
 * Cedar policy body's own `action == <Domain>::Action::"<Tag>"` head is the
 * sole filter; no `policy_rpc` ⇒ no enrichment / `SystemFail` surface; no
 * `custom_context` ⇒ the full base schema only (proven by the engine's
 * `always_trigger_parses` test: empty trigger ⇒ core + all actions). The baked
 * set is fetched once and cached for the SW lifetime; the dashboard set is
 * mutable, so it is re-read from storage on every `loadDefaultPolicySetV2()`
 * and kept fresh between decisions by a `storage.onChanged` listener.
 */

import Browser from "webextension-polyfill";

import { type ManagedPolicy, listManaged } from "./dashboard/storage";
import { getEnabledIds } from "./policy-selection";

/** Storage key the dashboard writes managed policies under. Must stay in sync
 *  with `dashboard/storage.ts` `KEY`. */
const DASHBOARD_STORAGE_KEY = "dashboard:policies";

/** Storage key holding the enabled policy-id allow-list. Must stay in sync with
 *  `policy-selection.ts` `ENABLED_KEY`. Toggling a policy off in the popup
 *  rewrites THIS key (not `dashboard:policies`), so the cache must refresh on
 *  it too — otherwise a disabled policy keeps being enforced. */
const ENABLED_IDS_STORAGE_KEY = "policy-selection:enabled-ids";

/**
 * On-disk asset row (one element of `policy-set-v2.json`). `manifest` is left
 * opaque: it is validated by the Rust `default_policies_v2.rs` gate at
 * fixture-author time, not re-validated here (mirrors how
 * `loadDefaultPolicySet` treats `manifest?: unknown`).
 */
export interface V2Bundle {
  /**
   * Bundle directory name (== `manifest.id` by the `default_policies_v2.rs`
   * invariant). Kept for deterministic ordering and a future enable/disable
   * layer (the v2 analog of v1 `getEnabledIds`). NOT consumed by WASM.
   */
  id: string;
  /** Raw `policy.cedar` text, verbatim. */
  policy: string;
  /** Parsed `manifest.json`, verbatim. */
  manifest: unknown;
}

/**
 * Exact WASM `BundleInput` row for `evaluate_action_v2_json` — DROPS `id`.
 * `BundleInput` has no `id` field today and (currently) no
 * `deny_unknown_fields`, but we map to `{ policy, manifest }` so a later
 * `deny_unknown_fields` addition on the Rust side cannot break the call.
 */
export interface EngineBundleInput {
  policy: string;
  manifest: unknown;
}

/** Baked set — immutable, fetched once per SW lifetime. */
let cachedV2Bundles: V2Bundle[] | null = null;
let inflight: Promise<V2Bundle[]> | null = null;

/** Dashboard set — mutable, re-read from `chrome.storage.local`. */
let cachedDashboardBundles: V2Bundle[] = [];
let dashboardListenerRegistered = false;

/**
 * Fetch the baked `default-policies/policy-set-v2.json` asset and hold it in a
 * module-level cache. Idempotent within a single SW lifetime; concurrent
 * callers share one in-flight fetch. Best-effort: on any fetch/parse failure
 * this logs a warning and caches `[]` (mirrors the v1 `[]` fallback) so it can
 * never throw out of the boot sequence.
 */
async function loadBakedSetV2(): Promise<V2Bundle[]> {
  if (cachedV2Bundles) return cachedV2Bundles;
  if (inflight) return inflight;

  inflight = (async () => {
    try {
      const setUrl = Browser.runtime.getURL(
        "default-policies/policy-set-v2.json",
      );
      const response = await fetch(setUrl);
      if (!response.ok) {
        throw new Error(`HTTP ${response.status} for ${setUrl}`);
      }
      const parsed = JSON.parse(await response.text()) as V2Bundle[];
      cachedV2Bundles = parsed;
      return parsed;
    } catch (err) {
      console.warn(
        "[Scopeball] v2 default policy set load failed:",
        err instanceof Error ? err.message : err,
      );
      // Cache the empty result so a transient failure doesn't re-fetch on
      // every read; the boot latch retries on the next SW lifetime.
      cachedV2Bundles = [];
      return cachedV2Bundles;
    } finally {
      inflight = null;
    }
  })();

  return inflight;
}

/**
 * Project one dashboard `ManagedPolicy` to a `V2Bundle` with a synthesized
 * minimal manifest. The empty trigger (no `where`) matches every action, so the
 * Cedar body's `action == ...` head is the sole filter; the engine composes the
 * full base schema (no `custom_context`), which is exactly what a base-context
 * policy like the dashboard authors needs to compile.
 */
function managedToV2Bundle(p: ManagedPolicy): V2Bundle {
  return {
    id: p.id,
    policy: p.text,
    manifest: { id: p.id, schema_version: 2 },
  };
}

/**
 * Re-read the dashboard's managed policies from `chrome.storage.local` and
 * refresh `cachedDashboardBundles`. Best-effort: when `Browser.storage` is
 * absent (non-SW/test env) treat it as "no dashboard policies"; on a transient
 * read error keep the prior cache so a flaky read doesn't silently drop the
 * user's enforced policies.
 */
async function refreshDashboardBundles(): Promise<void> {
  if (!Browser.storage?.local) return;
  try {
    // Respect the enabled-id allow-list: a managed policy toggled OFF in the
    // popup is removed from `policy-selection:enabled-ids` (NOT deleted from
    // `dashboard:policies`), so include only ids the user has enabled. This
    // mirrors the v1 `installFiltered(getEnabledIds())` gate.
    const [list, enabledIds] = await Promise.all([listManaged(), getEnabledIds()]);
    const enabled = new Set(enabledIds);
    cachedDashboardBundles = list
      .filter((p) => enabled.has(p.id))
      .map(managedToV2Bundle);
  } catch (err) {
    console.warn(
      "[Scopeball] v2 dashboard policy load failed:",
      err instanceof Error ? err.message : err,
    );
  }
}

/**
 * Register a one-shot `storage.onChanged` listener that refreshes the dashboard
 * cache whenever the user saves/deletes a policy. This keeps the SYNCHRONOUS
 * `getDefaultPolicyBundlesV2()` read fresh for the transaction path, which (un-
 * like the venue / typed-sig paths) does not `await loadDefaultPolicySetV2()`
 * immediately before reading.
 */
function ensureDashboardListener(): void {
  if (dashboardListenerRegistered) return;
  if (!Browser.storage?.onChanged) return; // non-SW/test env
  Browser.storage.onChanged.addListener((changes, areaName) => {
    if (areaName !== "local") return;
    // Refresh on a managed-policy edit (`dashboard:policies`) OR an enable/
    // disable toggle (`policy-selection:enabled-ids`) — the latter is what the
    // popup rewrites when a policy is switched off.
    if (DASHBOARD_STORAGE_KEY in changes || ENABLED_IDS_STORAGE_KEY in changes) {
      void refreshDashboardBundles();
    }
  });
  dashboardListenerRegistered = true;
}

/**
 * Load the full v2 policy set = baked (shipped) ∪ dashboard (user-authored).
 * The baked fetch is cached for the SW lifetime; the dashboard set is re-read
 * from storage on every call (cheap local read) and a `storage.onChanged`
 * listener is armed so the synchronous getter stays fresh between decisions.
 *
 * Returns a fresh array each call (callers may mutate it without poisoning the
 * caches). Best-effort throughout — a dashboard or baked failure degrades to
 * the other source rather than throwing out of the boot/decision path.
 */
export async function loadDefaultPolicySetV2(): Promise<V2Bundle[]> {
  ensureDashboardListener();
  const baked = await loadBakedSetV2();
  await refreshDashboardBundles();
  return [...baked, ...cachedDashboardBundles];
}

/**
 * Return the held v2 set (baked ∪ dashboard) mapped to the WASM `bundles` arg
 * shape (`{ policy, manifest }`, `id` dropped). Synchronous — the orchestrator
 * reads this on the decision path after `loadDefaultPolicySetV2()` warmed the
 * baked cache at boot and the `onChanged` listener kept the dashboard cache
 * fresh. Returns `[]` (baked) when the cache hasn't been warmed yet.
 */
export function getDefaultPolicyBundlesV2(): EngineBundleInput[] {
  const baked = cachedV2Bundles ?? [];
  return [...baked, ...cachedDashboardBundles].map(({ policy, manifest }) => ({
    policy,
    manifest,
  }));
}

/**
 * Test helper — drop both cached sets so successive vitest cases re-fetch from
 * a cold slate. Mirrors `__resetSeedBundlesForTest` for the seed-bundle path.
 */
export function __resetV2BundlesForTest(): void {
  cachedV2Bundles = null;
  inflight = null;
  cachedDashboardBundles = [];
  dashboardListenerRegistered = false;
}
