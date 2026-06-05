import Browser from "webextension-polyfill";
import { getCurrentUserId } from "./current-user";

const KEY_PREFIX = "dashboard:sets";
function setsKey(userId: string): string {
  return `${KEY_PREFIX}:${userId}`;
}
export const SETS_KEY_PREFIX = `${KEY_PREFIX}:`;
export const DASHBOARD_SET_ID_PREFIX = "dashboard-set::";
export const DASHBOARD_SET_ID_RE = /^dashboard-set::[A-Za-z0-9_./()-]{1,128}$/;
export const MAX_SET_NAME_BYTES = 256;
export const MAX_SET_DESCRIPTION_BYTES = 2_048;
export const MAX_SET_MEMBERS = 200;
export const MAX_SETS = 50;

export interface PolicySet {
  id: string;
  displayName: string;
  description?: string;
  /** Policy IDs that belong to this set. Many-to-many: a policy may
   *  appear in multiple sets. The SW does not validate that ids exist
   *  in `dashboard:policies` — callers may pre-add a set, then add the
   *  policy. Stale references are tolerated; the dashboard filters them
   *  when rendering. */
  memberIds: readonly string[];
  /** Provenance. Absent = `mine` (legacy). `market` sets are installed
   *  from the marketplace and treated as read-only in the list view. */
  source?: "mine" | "market";
  /** True when this set was installed from the marketplace. The list
   *  view shows a lock + disables in-place edits. */
  readOnly?: boolean;
  /** Domain category slug for the marketplace landing tiles. */
  cat?: string;
  /** When `source === 'market'`, the source listing id. */
  sourceListingId?: string;
  /** When `source === 'market'`, the installed version. */
  sourceVersion?: string;
  updatedAtMs: number;
  schemaVersion: 1;
}

function utf8ByteLength(s: string): number {
  return new TextEncoder().encode(s).length;
}

function assertValidId(id: string): void {
  if (!DASHBOARD_SET_ID_RE.test(id)) {
    throw new Error(
      `invalid_id: set id must match ${DASHBOARD_SET_ID_RE} (got "${id}")`,
    );
  }
}

function assertWithinCaps(set: PolicySet, listLengthAfter: number): void {
  const nameBytes = utf8ByteLength(set.displayName);
  if (nameBytes > MAX_SET_NAME_BYTES) {
    throw new Error(
      `name_too_large: set name is ${nameBytes} bytes, max ${MAX_SET_NAME_BYTES}`,
    );
  }
  if (set.description) {
    const descBytes = utf8ByteLength(set.description);
    if (descBytes > MAX_SET_DESCRIPTION_BYTES) {
      throw new Error(
        `description_too_large: ${descBytes} bytes, max ${MAX_SET_DESCRIPTION_BYTES}`,
      );
    }
  }
  if (set.memberIds.length > MAX_SET_MEMBERS) {
    throw new Error(
      `too_many_members: set has ${set.memberIds.length}, max ${MAX_SET_MEMBERS}`,
    );
  }
  if (listLengthAfter > MAX_SETS) {
    throw new Error(
      `too_many_sets: dashboard already stores ${MAX_SETS} sets; delete one before adding more`,
    );
  }
}

export async function listSets(): Promise<PolicySet[]> {
  const uid = await getCurrentUserId();
  if (!uid) return [];
  const key = setsKey(uid);
  const v = ((await Browser.storage.local.get(key)) as Record<string, unknown>)[
    key
  ] as PolicySet[] | undefined;
  return v ?? [];
}

export async function upsertSet(s: PolicySet): Promise<void> {
  assertValidId(s.id);
  const uid = await getCurrentUserId();
  if (!uid) {
    throw new Error("no_user: cannot save a set without an authenticated user");
  }
  const list = await listSets();
  const idx = list.findIndex((x) => x.id === s.id);
  const next = list.slice();
  if (idx >= 0) {
    next[idx] = s;
  } else {
    next.push(s);
  }
  assertWithinCaps(s, next.length);
  await Browser.storage.local.set({ [setsKey(uid)]: next });
}

export async function deleteSet(id: string): Promise<void> {
  const uid = await getCurrentUserId();
  if (!uid) return;
  const list = await listSets();
  await Browser.storage.local.set({
    [setsKey(uid)]: list.filter((s) => s.id !== id),
  });
}
