// V0 → V1 policy migration helper (D10).
//
// Before Phase 5, enriched fields lived at the top level of `context`
// (e.g. `context.totalInputUsd.value`). The Phase-5 enriched schema
// moves them under `context.custom`, so any user policies authored
// against the v0 layout have to be rewritten.
//
// This module only does the string-level rewrite + tracks pending
// migrations; the actual atomic re-install is glued together by the
// SDK / SW handler that calls `rewritePolicyText`, loads the managed
// policy, and pushes the rewritten text through `dashboard:put-raw`.

import Browser from "webextension-polyfill";

export const KEY_PENDING_MIGRATION = "migration:pending";

/**
 * Sibling key to `migration:pending`. Stores `Record<policyId, boolean>`
 * captured at detection time: the policy's enabled-state in
 * `policy-selection:enabled-ids` BEFORE the detector force-disabled it.
 *
 * Fix R: detection alone isn't enough. v0 policies stay in
 * `installFiltered`'s payload until they're stripped from the enabled
 * set, and once we strip them we have to remember whether the user
 * actually wanted them on so a successful Rewrite + ack can restore the
 * preference. `migration:original-enabled[id] === false` means the user
 * had the policy off and ack must remove it from enabled-ids again
 * after `put-raw` re-added it.
 *
 * First-write-wins on re-runs (see `mergeOriginalEnabled`): a second
 * detector pass observing the policy already-disabled must NOT overwrite
 * the original `true` snapshot.
 */
export const KEY_ORIGINAL_ENABLED = "migration:original-enabled";

/**
 * Rewrite a Cedar policy text from the v0 `context.<field>` layout to
 * the v1 `context.custom.<field>` layout for the supplied
 * `knownFields`. For each known field that appears as a direct
 * `context.<field>` reference:
 *
 * 1. Every `context.<field>` occurrence becomes `context.custom.<field>`.
 * 2. The `when { ... }` body gets a single `context.custom has <field>`
 *    guard prepended (conservative — fail-open at runtime if the field
 *    is missing).
 *
 * Fields that don't appear in the source are left alone, so passing a
 * superset of known fields is safe. Identifiers not in `knownFields`
 * are never touched.
 *
 * The rewrite is idempotent: a policy already using
 * `context.custom.<field>` will not be modified, and a policy that
 * already has the `context.custom has <field>` guard will keep exactly
 * one guard.
 */
export function rewritePolicyText(
  text: string,
  knownFields: readonly string[],
): string {
  let out = text;
  for (const field of knownFields) {
    if (!isAsciiIdent(field)) continue;

    // Match `context.<field>` where `<field>` is followed by a non-
    // identifier character. Excludes `context.custom.<field>` (already
    // migrated) by requiring the preceding char to NOT be ".custom"
    // (we use a lookahead-style guard via a tiny tokenizer below).
    const directRef = new RegExp(
      `(^|[^.\\w])context\\.${field}(?![\\w])`,
      "g",
    );
    if (!directRef.test(out)) continue;
    // Reset the regex since we test()ed and want to replace().
    directRef.lastIndex = 0;
    out = out.replace(directRef, (_match, prefix: string) => {
      return `${prefix}context.custom.${field}`;
    });

    // Add exactly one `context.custom has <field> &&` guard inside each
    // `when { ... }` body that doesn't already declare the guard.
    out = addHasGuardForField(out, field);
  }
  return out;
}

/**
 * INVERSE of [`rewritePolicyText`] — used to migrate Phase-8 fields that
 * were promoted FROM `context.custom.<field>` BACK to base
 * `context.<field>`. The two fields currently in this category are
 * `inputAmountNano` and `outputAmountNano`; the engine now populates
 * them itself during lowering instead of via a manifest enrichment.
 *
 * For each named field:
 *  1. Every `context.custom.<field>` becomes `context.<field>`.
 *  2. The old `context.custom has <field>` (and the umbrella `context has
 *     custom` when no other custom field is referenced) becomes
 *     `context has <field>` — the v8 cedarschema declares the field as
 *     base-optional.
 *
 * Idempotent: a policy already using the new layout is returned
 * unchanged. The umbrella `context has custom` guard is preserved when
 * OTHER custom fields are still referenced in the same `when` block
 * (we only drop it when our amount-nano rewrite removes the last custom
 * reference).
 *
 * UI integration is deferred — the v0→v1 banner machinery (Fix O) can
 * register a parallel pending-list for these ids once we have a real
 * use case. The helper is exported now so policies installed before
 * Phase 8 keep working through a one-line dashboard upgrade later.
 */
export function rewritePolicyTextCustomToBase(
  text: string,
  knownFields: readonly string[],
): string {
  let out = text;
  let dropped_any_custom_guard = false;

  for (const field of knownFields) {
    if (!isAsciiIdent(field)) continue;

    // Strip `.custom.<field>` references back to `.<field>`. Tight regex
    // — only the exact custom-prefix shape, never partial matches that
    // would corrupt unrelated identifiers (`customer`, `customX.field`…).
    const customRef = new RegExp(
      `(^|[^.\\w])context\\.custom\\.${field}(?![\\w])`,
      "g",
    );
    if (!customRef.test(out)) continue;
    customRef.lastIndex = 0;
    out = out.replace(customRef, (_m, prefix: string) => {
      return `${prefix}context.${field}`;
    });

    // Replace the nested has-guard with the base-level one. We keep one
    // copy when multiple guards collapsed onto the same field name (the
    // `&&` join is preserved by the replace).
    const hasGuard = new RegExp(
      `context\\.custom has ${field}(?!\\w)`,
      "g",
    );
    out = out.replace(hasGuard, `context has ${field}`);

    dropped_any_custom_guard = true;
  }

  if (dropped_any_custom_guard) {
    out = dropUmbrellaCustomGuardIfUnused(out);
  }
  return out;
}

/**
 * Remove the `context has custom &&` guard from each `when { ... }` body
 * that no longer references any `context.custom.*` field. The rewrite
 * pass above doesn't track this per-body, so we re-scan after — cheap
 * because `when` bodies are short.
 */
function dropUmbrellaCustomGuardIfUnused(text: string): string {
  return text.replace(/when\s*\{([\s\S]*?)\}/g, (whole, bodyRaw: string) => {
    const body = String(bodyRaw);
    if (body.includes("context.custom.")) return whole;
    // Drop variants:
    //   `context has custom && `
    //   `&& context has custom`
    //   stand-alone `context has custom` (no other clauses left)
    let next = body
      .replace(/\bcontext has custom\b\s*&&\s*/g, "")
      .replace(/\s*&&\s*\bcontext has custom\b/g, "")
      .replace(/\bcontext has custom\b/g, "");
    return whole.replace(body, next);
  });
}

function isAsciiIdent(s: string): boolean {
  return /^[A-Za-z_][A-Za-z0-9_]*$/.test(s);
}

function addHasGuardForField(text: string, field: string): string {
  const guard = `context.custom has ${field}`;
  // For each `when {` opener, prepend the guard if the immediately-
  // following body doesn't already contain it.
  return text.replace(/when\s*\{/g, (match, offset) => {
    // Look at the body until the closing brace to decide if the guard
    // is already there.
    const rest = text.slice(offset);
    const closeIdx = findMatchingClose(rest);
    if (closeIdx === -1) return match;
    const body = rest.slice(0, closeIdx + 1);
    if (body.includes(guard)) return match;
    return `${match} ${guard} && `;
  });
}

function findMatchingClose(s: string): number {
  let depth = 0;
  for (let i = 0; i < s.length; i++) {
    if (s[i] === "{") depth++;
    else if (s[i] === "}") {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}

/**
 * Policy ids waiting to be migrated. Populated when the SW boots and
 * notices a managed-policy text that still uses the v0 layout. The
 * dashboard's migration banner reads this list to surface the action
 * to the user.
 */
export async function listPending(): Promise<string[]> {
  const r = (await Browser.storage.local.get(KEY_PENDING_MIGRATION)) as Record<
    string,
    unknown
  >;
  const raw = r[KEY_PENDING_MIGRATION];
  if (!Array.isArray(raw)) return [];
  return raw.filter((x): x is string => typeof x === "string");
}

export async function setPending(ids: readonly string[]): Promise<void> {
  if (ids.length === 0) {
    await Browser.storage.local.remove(KEY_PENDING_MIGRATION);
    return;
  }
  await Browser.storage.local.set({ [KEY_PENDING_MIGRATION]: [...ids] });
}

/**
 * Read the original-enabled snapshot. Returns `{}` when absent or
 * malformed; ids whose stored value isn't a boolean are dropped.
 */
export async function getOriginalEnabled(): Promise<Record<string, boolean>> {
  const r = (await Browser.storage.local.get(KEY_ORIGINAL_ENABLED)) as Record<
    string,
    unknown
  >;
  const raw = r[KEY_ORIGINAL_ENABLED];
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return {};
  const out: Record<string, boolean> = {};
  for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
    if (typeof v === "boolean") out[k] = v;
  }
  return out;
}

export async function setOriginalEnabled(
  snapshot: Record<string, boolean>,
): Promise<void> {
  if (Object.keys(snapshot).length === 0) {
    await Browser.storage.local.remove(KEY_ORIGINAL_ENABLED);
    return;
  }
  await Browser.storage.local.set({ [KEY_ORIGINAL_ENABLED]: { ...snapshot } });
}

/**
 * Pop one id off the original-enabled snapshot. Used by `migration:ack`
 * after the rewrite flow completes and the user's preference has been
 * restored (or doesn't need restoring). Removes the key entirely when
 * the last id is popped to keep storage tidy.
 */
export async function clearOriginalEnabled(id: string): Promise<void> {
  const cur = await getOriginalEnabled();
  if (!(id in cur)) return;
  delete cur[id];
  await setOriginalEnabled(cur);
}
