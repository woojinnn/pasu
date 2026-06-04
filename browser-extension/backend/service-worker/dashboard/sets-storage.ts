import Browser from "webextension-polyfill";

const KEY = "dashboard:sets";
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
  const v = ((await Browser.storage.local.get(KEY)) as Record<string, unknown>)[
    KEY
  ] as PolicySet[] | undefined;
  return v ?? [];
}

export async function upsertSet(s: PolicySet): Promise<void> {
  assertValidId(s.id);
  const list = await listSets();
  const idx = list.findIndex((x) => x.id === s.id);
  const next = list.slice();
  if (idx >= 0) {
    next[idx] = s;
  } else {
    next.push(s);
  }
  assertWithinCaps(s, next.length);
  await Browser.storage.local.set({ [KEY]: next });
}

export async function deleteSet(id: string): Promise<void> {
  const list = await listSets();
  await Browser.storage.local.set({
    [KEY]: list.filter((s) => s.id !== id),
  });
}
