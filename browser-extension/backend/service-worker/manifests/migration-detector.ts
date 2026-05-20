// Migration auto-detection (Fix O).
//
// Up to Phase 7 the migration UI (`migration:list` handler + the
// dashboard banner) read from `chrome.storage.local["migration:pending"]`
// but nothing in production code ever wrote to that key. The result:
// post-Phase-5 a user with v0 policies in storage never saw the
// "Rewrite to context.custom.*" banner — `listPending()` always returned
// empty, the install path failed closed at runtime, and the user had no
// affordance to fix it.
//
// This module fixes that producer gap: it scans every managed policy
// text for `context.<knownEnrichmentField>` references and writes the
// matching ids onto the pending set, **merging** with whatever ids were
// already there. Re-running is a no-op (Set semantics).
//
// Wired into the boot path AFTER `hydrateManifests` so the banner sees
// the queue on the next dashboard load.

import Browser from "webextension-polyfill";
import { V0_KNOWN_FIELDS } from "../../../sdk/extension-client";
import { listManaged as defaultListManaged } from "../dashboard/storage";
import type { ManagedPolicy } from "../dashboard/storage";
import {
  KEY_ORIGINAL_ENABLED,
  KEY_PENDING_MIGRATION,
  getOriginalEnabled,
  listPending,
} from "./migration";

// Mirrors `policy-selection.ts` ENABLED_KEY. Declared here too so the
// detector doesn't have to take a runtime dependency on policy-selection
// (which would also drag in the catalog/listManaged graph at module
// init).
const KEY_ENABLED_IDS = "policy-selection:enabled-ids";

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
 *      `installFiltered` skips them — without this, every install retry
 *      keeps hitting the enriched schema's "no `context.<field>`" error
 *      and the orchestrator's reject path runs on every request (Fix R).
 *   3. Snapshot each id's prior enabled-state into
 *      `migration:original-enabled` so `migration:ack` can restore the
 *      preference after the user clicks Rewrite and the rewrite +
 *      put-raw lands. First-write-wins — a second detector pass
 *      observing the policy already-disabled does NOT clobber the
 *      original snapshot.
 *
 * Idempotent: re-running with the same inputs does not change pending,
 * does not re-strip an already-stripped id, and does not flip an
 * existing original-enabled value.
 *
 * The scan is intentionally conservative: it matches occurrences of
 * `context.<field>` where `<field>` is in [`V0_KNOWN_FIELDS`] and is
 * NOT preceded by `.custom` (those are already migrated). Identifiers
 * outside the known set are never flagged.
 *
 * Storage writes are batched into a single `chrome.storage.local.set`
 * call so an interrupt mid-call can never leave `migration:pending`
 * populated while the enabled-set still contains the id (the exact
 * failure mode Fix R closes).
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
  const r = (await Browser.storage.local.get(KEY_ENABLED_IDS)) as Record<
    string,
    unknown
  >;
  const raw = r[KEY_ENABLED_IDS];
  if (!Array.isArray(raw)) return [];
  return raw.filter((x): x is string => typeof x === "string");
}

/**
 * Single-`set()` write of all three keys so the storage state is
 * internally consistent even if the SW is suspended mid-flush. The
 * trio:
 *   - `migration:pending` — list of ids the banner shows. Removed when
 *     empty so storage stays tidy.
 *   - `policy-selection:enabled-ids` — the source of truth
 *     `installFiltered` reads. Force-disabled v0 ids are gone.
 *   - `migration:original-enabled` — `{id → wasEnabledPriorToDetection}`.
 *     Drives ack-side restore. Removed when empty.
 */
async function writeDetectorState(state: {
  pending: readonly string[];
  enabled: readonly string[];
  original: Record<string, boolean>;
}): Promise<void> {
  const toSet: Record<string, unknown> = {
    [KEY_ENABLED_IDS]: [...state.enabled],
  };
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
 * Return true when `text` references any known v0 enrichment field at
 * top-level `context.<field>` (NOT `context.custom.<field>`). The
 * regex tests both:
 *   - `context.<field>` not preceded by `.` or word char (so we don't
 *     match `bla.context.foo` or `xcontext.foo`)
 *   - excludes the v1 `context.custom.<field>` layout via the leading
 *     character class.
 */
function containsV0Reference(text: string): boolean {
  for (const field of V0_KNOWN_FIELDS) {
    if (!isAsciiIdent(field)) continue;
    // `(^|[^.\w])context.<field>(?![\w])`
    // The leading group excludes `.context.<field>` (would chain into
    // another property) and word-char prefixes. The trailing negative
    // lookahead excludes `context.<field>X` where X is an identifier
    // continuation. Crucially, this matches `context.<field>` but NOT
    // `context.custom.<field>` because the `.custom` segment sits
    // between `context` and `<field>` and won't be tested as the
    // leading boundary.
    const re = new RegExp(`(?:^|[^.\\w])context\\.${field}(?![\\w])`);
    if (re.test(text)) return true;
  }
  return false;
}

function isAsciiIdent(s: string): boolean {
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(s);
}
