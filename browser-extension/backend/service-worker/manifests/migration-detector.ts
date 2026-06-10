// Migration auto-detection.
//
// Scans every managed policy text for `context.<knownEnrichmentField>`
// references and writes matching ids onto the `migration:pending` set,
// merging with whatever ids were already there (re-running is a no-op).
//
// Wired into the boot path after `hydrateManifests` so the dashboard
// migration banner sees the queue on the next load.

import Browser from "webextension-polyfill";
import { V0_KNOWN_FIELDS } from "../../../sdk/extension-client";
import { getCurrentUserId } from "../dashboard/current-user";
import { listManaged as defaultListManaged } from "../dashboard/storage";
import type { ManagedPolicy } from "../dashboard/storage";
import {
  KEY_ORIGINAL_ENABLED,
  KEY_PENDING_MIGRATION,
  getOriginalEnabled,
  listPending,
} from "./migration";

// Mirrors `policy-selection.ts`'s per-user enabled-ids key. Built here
// (not imported) to keep the detector off the catalog/listManaged graph at
// module init. Must namespace by user ID — stripping from a stale base key
// would silently diverge from what `installFiltered` reads.
const ENABLED_IDS_KEY_PREFIX = "policy-selection:enabled-ids";
function enabledIdsKey(userId: string): string {
  return `${ENABLED_IDS_KEY_PREFIX}:${userId}`;
}

export interface MigrationDetectorDeps {
  listManaged: () => Promise<ManagedPolicy[]>;
}

const DEFAULT_DEPS: MigrationDetectorDeps = {
  listManaged: defaultListManaged,
};

/**
 * Detect v0 policy texts and:
 *   1. Push their ids onto `migration:pending` (set-merge with prior).
 *   2. Strip them from `policy-selection:enabled-ids` so the next
 *      `installFiltered` skips them — otherwise every install keeps hitting
 *      the enriched schema's "no `context.<field>`" error.
 *   3. Snapshot each id's prior enabled-state into
 *      `migration:original-enabled` so `migration:ack` can restore the
 *      preference. First-write-wins — a second detector pass must not
 *      overwrite the original snapshot.
 *
 * Idempotent: re-running with the same inputs is a no-op.
 *
 * Storage writes are batched into a single `chrome.storage.local.set` call
 * so an interrupt cannot leave `migration:pending` populated while the
 * enabled-set still contains the id.
 */
export async function detectPendingMigrations(
  overrides: Partial<MigrationDetectorDeps> = {},
): Promise<{ pending: readonly string[]; added: readonly string[] }> {
  const deps: MigrationDetectorDeps = { ...DEFAULT_DEPS, ...overrides };

  const [managed, currentPending, currentEnabledRaw, currentOriginal] =
    await Promise.all([
      deps.listManaged(),
      listPending(),
      readEnabledIds(),
      getOriginalEnabled(),
    ]);

  const pendingSet = new Set<string>(currentPending);
  const enabledSet = new Set<string>(currentEnabledRaw);
  const originalSnapshot: Record<string, boolean> = { ...currentOriginal };
  const added: string[] = [];
  let mutated = false;

  for (const policy of managed) {
    if (!policy?.text) continue;
    if (!containsV0Reference(policy.text)) continue;

    if (!pendingSet.has(policy.id)) {
      pendingSet.add(policy.id);
      added.push(policy.id);
      mutated = true;
    }
    // First-write-wins. If the snapshot already has this id, the user's
    // original preference (captured on the FIRST detection) is the
    // truth — we must not overwrite it just because the policy is now
    // disabled by a prior detector pass.
    if (!(policy.id in originalSnapshot)) {
      originalSnapshot[policy.id] = enabledSet.has(policy.id);
      mutated = true;
    }
    if (enabledSet.has(policy.id)) {
      enabledSet.delete(policy.id);
      mutated = true;
    }
  }

  const nextPending = Array.from(pendingSet);

  if (mutated) {
    await writeDetectorState({
      pending: nextPending,
      enabled: Array.from(enabledSet),
      original: originalSnapshot,
    });
  }

  return { pending: nextPending, added };
}

async function readEnabledIds(): Promise<string[]> {
  const uid = await getCurrentUserId();
  if (!uid) return [];
  const key = enabledIdsKey(uid);
  const r = (await Browser.storage.local.get(key)) as Record<string, unknown>;
  const raw = r[key];
  if (!Array.isArray(raw)) return [];
  return raw.filter((x): x is string => typeof x === "string");
}

/**
 * Single-`set()` write of all three keys so storage stays consistent even
 * if the SW is suspended mid-flush:
 *   - `migration:pending` — ids the banner shows; removed when empty.
 *   - `policy-selection:enabled-ids` — v0 ids stripped so `installFiltered`
 *     skips them.
 *   - `migration:original-enabled` — `{id → wasEnabled}` for ack-side
 *     restore; removed when empty.
 */
async function writeDetectorState(state: {
  pending: readonly string[];
  enabled: readonly string[];
  original: Record<string, boolean>;
}): Promise<void> {
  const uid = await getCurrentUserId();
  const toSet: Record<string, unknown> = {};
  // Only a signed-in user has a per-user enabled-ids set to rewrite.
  if (uid) {
    toSet[enabledIdsKey(uid)] = [...state.enabled];
  }
  const toRemove: string[] = [];
  if (state.pending.length > 0) {
    toSet[KEY_PENDING_MIGRATION] = [...state.pending];
  } else {
    toRemove.push(KEY_PENDING_MIGRATION);
  }
  if (Object.keys(state.original).length > 0) {
    toSet[KEY_ORIGINAL_ENABLED] = { ...state.original };
  } else {
    toRemove.push(KEY_ORIGINAL_ENABLED);
  }
  await Browser.storage.local.set(toSet);
  if (toRemove.length > 0) {
    await Browser.storage.local.remove(toRemove);
  }
}

/**
 * Return true when `text` references any known v0 enrichment field as a
 * top-level `context.<field>` (NOT `context.custom.<field>`). The leading
 * character class prevents matching chained properties or identifier
 * prefixes; `context.custom.<field>` is already migrated and excluded.
 */
function containsV0Reference(text: string): boolean {
  for (const field of V0_KNOWN_FIELDS) {
    if (!isAsciiIdent(field)) continue;
    // `(^|[^.\w])context.<field>(?![\w])` — excludes chained properties,
    // identifier prefixes, identifier continuations, and the already-migrated
    // `context.custom.<field>` shape.
    const re = new RegExp(`(?:^|[^.\\w])context\\.${field}(?![\\w])`);
    if (re.test(text)) return true;
  }
  return false;
}

function isAsciiIdent(s: string): boolean {
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(s);
}
