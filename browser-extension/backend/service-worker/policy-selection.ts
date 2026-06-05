import Browser from 'webextension-polyfill';
import { parsePolicyMeta, type Severity } from '@lib/policy-meta';
import { listManaged } from './dashboard/storage';
import { getCurrentUserId } from './dashboard/current-user';
import { listInstalled } from './adapter-loader/storage';

const ENABLED_KEY_PREFIX = 'policy-selection:enabled-ids';
const APPLIED_KEY_PREFIX = 'policy-selection:applied-ids';
export const ENABLED_KEY_PREFIX_WITH_SEP = `${ENABLED_KEY_PREFIX}:`;
export const APPLIED_KEY_PREFIX_WITH_SEP = `${APPLIED_KEY_PREFIX}:`;
function enabledKey(uid: string): string {
  return `${ENABLED_KEY_PREFIX}:${uid}`;
}
function appliedKey(uid: string): string {
  return `${APPLIED_KEY_PREFIX}:${uid}`;
}

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

export type ReinstallFn = (ids: string[]) => Promise<void>;

async function readStringArray(key: string): Promise<string[]> {
  const raw = (await Browser.storage.local.get(key)) as Record<string, unknown>;
  const v = raw[key];
  return Array.isArray(v) ? (v.filter((x) => typeof x === 'string') as string[]) : [];
}

async function writeStringArray(key: string, ids: string[]): Promise<void> {
  await Browser.storage.local.set({ [key]: ids });
}

export async function getEnabledIds(): Promise<string[]> {
  const uid = await getCurrentUserId();
  if (!uid) return [];
  return readStringArray(enabledKey(uid));
}

export async function getAppliedIds(): Promise<string[]> {
  const uid = await getCurrentUserId();
  if (!uid) return [];
  return readStringArray(appliedKey(uid));
}

let inflight: Promise<ApplyResult> | null = null;
let queuedDesired: string[] | null = null;
const queuedResolvers: ((r: ApplyResult) => void)[] = [];

function classifyError(err: unknown): { kind: string; message: string } {
  if (err instanceof Error) {
    const m = err.message.match(/^([a-z_]+):\s*(.*)$/);
    if (m) return { kind: m[1], message: m[2] };
    return { kind: 'reinstall_failed', message: err.message };
  }
  return { kind: 'reinstall_failed', message: String(err) };
}

function normalizeIds(ids: string[]): string[] {
  return [...new Set(ids)].sort();
}

async function runApply(ids: string[], reinstall: ReinstallFn): Promise<ApplyResult> {
  const uid = await getCurrentUserId();
  if (!uid) {
    // No active user → no per-user enabled-ids namespace. Treat as a no-op
    // success: the engine has nothing dashboard-specific to install.
    try {
      await reinstall([]);
      return { ok: true };
    } catch (err) {
      return { ok: false, error: classifyError(err) };
    }
  }
  const sorted = normalizeIds(ids);
  try {
    await writeStringArray(enabledKey(uid), sorted);
    await reinstall(sorted);
    await writeStringArray(appliedKey(uid), sorted);
    return { ok: true };
  } catch (err) {
    return { ok: false, error: classifyError(err) };
  }
}

/**
 * Apply a desired enabled-ids set to the engine.
 *
 * Serialization: at most one in-flight reinstall + a single tail slot.
 * Rapid toggles collapse — newer calls overwrite the queued tail; ALL
 * queued resolvers (including the head's promise) settle with the
 * tail's result, so a caller can observe `{ok:false}` even when its own
 * runApply succeeded — the popup's UI needs the latest engine state,
 * which the tail represents.
 *
 * The IIFE captures the FIRST caller's `reinstall` callback; queued
 * calls' `reinstall` parameters are ignored. Pass a stable, idempotent
 * module-scoped reference (`reinstallAllPolicies`) at every call site.
 *
 * Storage semantics: ENABLED_KEY is written by `runApply` with the same
 * ids it passes to `reinstall(ids)`, so the loader receives ids verbatim
 * via the callback parameter (it MUST NOT re-read storage to decide
 * what to install — that would race with rapid toggles). APPLIED_KEY is
 * updated only after a successful reinstall, leaving the previous
 * applied set intact on failure.
 */
export async function applyEnabledIds(
  ids: string[],
  reinstall: ReinstallFn,
): Promise<ApplyResult> {
  if (inflight) {
    return new Promise<ApplyResult>((resolve) => {
      queuedDesired = [...ids];
      queuedResolvers.push(resolve);
    });
  }

  inflight = (async () => {
    try {
      let lastResult = await runApply(ids, reinstall);
      while (queuedDesired !== null) {
        const next = queuedDesired;
        queuedDesired = null;
        const resolvers = queuedResolvers.splice(0);
        lastResult = await runApply(next, reinstall);
        for (const r of resolvers) r(lastResult);
      }
      return lastResult;
    } finally {
      inflight = null;
    }
  })();

  return inflight;
}

interface DefaultPolicyEntry {
  id: string;
  text: string;
}

interface V2DefaultEntry {
  id: string;
  /** Cedar policy text with @id/@severity/@reason — same shape parsePolicyMeta
   * already parses for the v1 catalog. */
  policy: string;
}

async function loadDefaults(): Promise<DefaultPolicyEntry[]> {
  // The baked default set migrated to v2 (`policy-set-v2.json`); the old v1
  // `policy-set.json` is now an empty `[]`, which is why the popup catalog
  // showed nothing while v2 evaluation enforced the 9 shipped policies. Read
  // the v2 asset (baked set only — dashboard/managed policies are listed
  // separately via listManaged, so going through loadDefaultPolicySetV2 here
  // would double-count them) and project each `{id, policy}` onto the
  // v1-shaped `{id, text}` the catalog builder consumes.
  const url = Browser.runtime.getURL('default-policies/policy-set-v2.json');
  const res = await fetch(url);
  const v2 = (await res.json()) as V2DefaultEntry[];
  return v2.map((b) => ({ id: b.id, text: b.policy }));
}

function namespaceOf(id: string): string {
  // "default::dex/foo" → "default::dex"
  const slash = id.lastIndexOf('/');
  return slash >= 0 ? id.slice(0, slash) : id;
}

export async function getCatalog(): Promise<Catalog> {
  const [defaults, bundles, managed, enabledRaw, appliedRaw] = await Promise.all([
    loadDefaults(),
    listInstalled(),
    listManaged(),
    getEnabledIds(),
    getAppliedIds(),
  ]);

  const policies: CatalogPolicy[] = [];
  for (const entry of defaults) {
    const meta = parsePolicyMeta(entry.text);
    policies.push({
      id: entry.id,
      rules: meta.rules,
      dominantSeverity: meta.dominantSeverity,
      sourceLabel: namespaceOf(entry.id),
    });
  }
  for (const bundle of bundles) {
    const sourceLabel = `${bundle.bundle_id}@${bundle.version}`;
    for (const entry of bundle.renderedPolicySet) {
      const meta = parsePolicyMeta(entry.text);
      policies.push({
        id: entry.id,
        rules: meta.rules,
        dominantSeverity: meta.dominantSeverity,
        sourceLabel,
      });
    }
  }
  for (const entry of managed) {
    const meta = parsePolicyMeta(entry.text);
    policies.push({
      id: entry.id,
      rules: meta.rules,
      dominantSeverity: meta.dominantSeverity,
      sourceLabel: 'dashboard',
    });
  }

  const knownIds = new Set(policies.map((p) => p.id));
  const enabled = enabledRaw.filter((id) => knownIds.has(id));
  const applied = appliedRaw.filter((id) => knownIds.has(id));

  return { policies, enabled, applied };
}
