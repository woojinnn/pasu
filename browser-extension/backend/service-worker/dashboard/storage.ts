import Browser from "webextension-polyfill";
import type { ParamsSchema, ParamValues } from "../adapter-loader/params-validator";
import type { RenderedPolicyEntry } from "../adapter-loader/storage";
import { getCurrentUserId } from "./current-user";

// chrome.storage.local quota is 5–10 MB depending on the browser, but per-item
// 'large data' performance falls off a cliff well before that. Cap individual
// policy bodies at 32 KiB and total entries at 200 so a misbehaving dashboard
// can't drive the SW into quota errors. Beyond those caps the writer rejects.

/**
 * Per-user storage key. The discriminator is the policy-server's `user_id`
 * (mirrored via the dashboard auth flow). When no user is active, reads
 * fall back to an empty list and writes throw `no_user` — same Chrome
 * profile with two different dashboard accounts therefore see two
 * disjoint policy spaces.
 */
const KEY_PREFIX = "dashboard:policies";
export function policiesStorageKey(userId: string): string {
  return `${KEY_PREFIX}:${userId}`;
}
/** Prefix-match guard for the storage.onChanged listeners that have to
 *  fan out to multiple user namespaces. */
export const POLICIES_KEY_PREFIX = `${KEY_PREFIX}:`;
export const DASHBOARD_ID_PREFIX = "dashboard::";
export const DASHBOARD_ID_RE = /^dashboard::[A-Za-z0-9_./()-]{1,128}$/;
export const MAX_TEXT_BYTES = 32_768;
export const MAX_ENTRIES = 200;

export interface ManagedPolicyTemplateMeta {
  source: string;
  paramsSchema: ParamsSchema;
  paramValues: ParamValues;
}

/** Lifecycle stage. `draft` = author still working, hidden from
 *  enforced set when the draft-gate flag is on. `publish` = finalised
 *  and eligible for enforcement. Absent on legacy entries — treated
 *  as `publish` so behaviour is unchanged when the flag is off. */
export type PolicyLife = "draft" | "publish";

/** Provenance. `mine` = authored locally. `market` = installed from
 *  the marketplace; `sourceListingId` + `sourceVersion` carry the
 *  outbound link so the list view can detect upstream updates. */
export type PolicySource = "mine" | "market";

/** Authoring surface chosen at create time. `cedar` = raw text only,
 *  `block` = Blockly canvas (still produces Cedar), `form` = guided
 *  wizard (reserved; stub-disabled until the form mode ships). */
export type PolicyMethod = "form" | "block" | "cedar";

export interface ManagedPolicy {
  id: string;
  kind: "raw" | "template";
  /** For 'raw': original text. For 'template': rendered text. */
  text: string;
  template?: ManagedPolicyTemplateMeta;
  manifest?: unknown;
  manifests?: readonly unknown[];
  /** Optional v7 builder tree snapshot (JSON-encoded `Doc`).
   *  Present when the policy was authored in Builder mode; absent for
   *  Code-only policies. The runtime ignores this; it's metadata so the
   *  dashboard can reopen the canvas exactly as the user left it. */
  policyTree?: string;
  /** Human-readable display name. Falls back to the `@id` annotation
   *  parsed from `text` when absent. */
  displayName?: string;
  /** Draft/publish lifecycle. Absent = `publish` (legacy compatible). */
  life?: PolicyLife;
  /** Provenance. Absent = `mine` (legacy compatible). */
  source?: PolicySource;
  /** Domain category slug (e.g. `defi`, `nft`). Free-form; the dashboard
   *  uses it for the category chip row and download leaderboard buckets. */
  cat?: string;
  /** Authoring surface chosen at create time. Drives the default view
   *  tab when the new editor opens. */
  method?: PolicyMethod;
  /** Dedup hint surfaced by the list view when two policies in the same
   *  package collide. Currently advisory; future work parses it from the
   *  manifest's action selector. */
  dupKey?: string;
  /** Free-form author note. Shown above the editor tabs. */
  memo?: string;
  /** When `source === 'market'`, the listing id this copy came from. */
  sourceListingId?: string;
  /** When `source === 'market'`, the listing version installed. The list
   *  view compares this to the current upstream version to badge stale
   *  installs. */
  sourceVersion?: string;
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
  const uid = await getCurrentUserId();
  if (!uid) return [];
  const key = policiesStorageKey(uid);
  const v = ((await Browser.storage.local.get(key)) as Record<string, unknown>)[
    key
  ] as ManagedPolicy[] | undefined;
  return v ?? [];
}

export async function upsertManaged(p: ManagedPolicy): Promise<void> {
  assertValidId(p.id);
  const uid = await getCurrentUserId();
  if (!uid) {
    throw new Error("no_user: cannot save a policy without an authenticated user");
  }
  const list = await listManaged();
  const idx = list.findIndex((x) => x.id === p.id);
  const next = list.slice();
  if (idx >= 0) {
    next[idx] = p;
  } else {
    next.push(p);
  }
  assertWithinCaps(p.text, next.length);
  await Browser.storage.local.set({ [policiesStorageKey(uid)]: next });
}

export async function deleteManaged(id: string): Promise<void> {
  const uid = await getCurrentUserId();
  if (!uid) {
    // No user → no per-user storage exists. Nothing to delete, no error.
    return;
  }
  const list = await listManaged();
  await Browser.storage.local.set({
    [policiesStorageKey(uid)]: list.filter((p) => p.id !== id),
  });
}

/** Loader-facing projection. Mirrors the shape that
 *  adapter-loader `aggregatedPolicySet` returns so `policies-loader` can union
 *  defaults ∪ adapter-loader ∪ dashboard with one filter pass. */
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
