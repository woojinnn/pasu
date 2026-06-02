/**
 * JSON snapshot ↔ `Doc` round-trip for persistence.
 *
 * The server stores the v7 builder tree as an opaque JSON string in
 * `user_policies.policy_tree`. We don't share the typescript schema
 * directly — instead, every persistence round-trip funnels through
 * `serializeTree` / `deserializeTree` so the on-disk format has a
 * single version-stamped envelope:
 *
 *   { "v": 1, "doc": <Doc> }
 *
 * If we ever rev the doc shape, `parseTree` can branch on `v` and
 * adapt. The runtime path stays untouched — Cedar text is the actual
 * compile target.
 */

import type { Doc } from "./types";

const CURRENT_VERSION = 1;

export interface TreeEnvelope {
  v: number;
  doc: Doc;
}

export function serializeTree(doc: Doc): string {
  const env: TreeEnvelope = { v: CURRENT_VERSION, doc };
  return JSON.stringify(env);
}

export function parseTree(raw: string | null | undefined): Doc | null {
  if (!raw) return null;
  try {
    const env = JSON.parse(raw) as Partial<TreeEnvelope>;
    if (!env || typeof env !== "object") return null;
    if (env.v !== CURRENT_VERSION) {
      // Future: branch on env.v and migrate. For now, treat older
      // envelopes as unreadable — the caller falls back to Code mode.
      return null;
    }
    if (!env.doc) return null;
    return env.doc;
  } catch {
    return null;
  }
}
