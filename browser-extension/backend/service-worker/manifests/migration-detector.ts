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

import { V0_KNOWN_FIELDS } from "../../../sdk/extension-client";
import { listManaged as defaultListManaged } from "../dashboard/storage";
import type { ManagedPolicy } from "../dashboard/storage";
import { listPending, setPending } from "./migration";

export interface MigrationDetectorDeps {
  listManaged: () => Promise<ManagedPolicy[]>;
}

const DEFAULT_DEPS: MigrationDetectorDeps = {
  listManaged: defaultListManaged,
};

/**
 * Detect v0 policy texts and push their ids onto `migration:pending`.
 *
 * Idempotent. The merge uses a Set so the same id added twice still
 * appears once.
 *
 * The scan is intentionally conservative: it matches occurrences of
 * `context.<field>` where `<field>` is in [`V0_KNOWN_FIELDS`] and is
 * NOT preceded by `.custom` (those are already migrated). Identifiers
 * outside the known set are never flagged.
 */
export async function detectPendingMigrations(
  overrides: Partial<MigrationDetectorDeps> = {},
): Promise<{ pending: readonly string[]; added: readonly string[] }> {
  const deps: MigrationDetectorDeps = { ...DEFAULT_DEPS, ...overrides };

  const managed = await deps.listManaged();
  const existing = new Set<string>(await listPending());
  const added: string[] = [];

  for (const policy of managed) {
    if (!policy?.text) continue;
    if (containsV0Reference(policy.text)) {
      if (!existing.has(policy.id)) {
        existing.add(policy.id);
        added.push(policy.id);
      }
    }
  }

  // Preserve insertion order (existing first, then newly-detected). Set
  // iteration order is insertion-order in JS so this is stable.
  const next = Array.from(existing);
  await setPending(next);
  return { pending: next, added };
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
