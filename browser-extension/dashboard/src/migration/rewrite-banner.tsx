// Migration rewrite banner (Phase 7.5, spec D10 + "/policies UX").
//
// Renders at the top of the policies list page. Reads
// `listMigrationPending()` on mount; if any ids are pending, shows a
// banner with one "Rewrite" button per id. Clicking Rewrite walks the
// three-step flow:
//
//   1. `rewritePolicyToCustom({id, text, knownFields})` — returns the
//      rewritten text but does NOT persist (per SDK comment around
//      lines 224-232 of extension-client.ts).
//   2. `putRaw({id, text: rewritten})` — persists the v1 text. Skipped
//      when the rewrite returns `applied: false` (no substitution
//      needed; the SW auto-acks in that case).
//   3. `migrationAck(id)` — pops the id off the pending queue. Splitting
//      the rewrite and ack avoids leaving the pending set empty while
//      storage still holds v0 text (see handlers.ts:196).
//
// On any step failing the id keeps its error row and the pending set
// stays intact — the user can retry.

import { useCallback, useEffect, useMemo, useState } from "react";
import type { ManagedPolicy } from "@scopeball/sdk";
import { useExtension } from "../sdk-context";
import "./rewrite-banner.css";

// The v0 known-field set. The Phase-6 SW handler's `rewritePolicyText`
// only substitutes occurrences of `context.<name>` for names listed
// here, so passing the wrong list would either leave fields un-rewritten
// or rewrite tokens that shouldn't be touched. This set is the
// pre-v1 base alias-table names from spec §"Migration of existing
// user-installed policies (D10)".
// How long the "No rewrite needed" toast stays visible after an
// `applied: false` migration. Short enough to feel ephemeral, long
// enough for the user to read it.
const INFO_AUTO_CLEAR_MS = 3_000;

const V0_KNOWN_FIELDS: readonly string[] = [
  "totalInputUsd",
  "totalMinOutputUsd",
  "effectiveRateVsOracleBps",
  "totalInputFractionOfPortfolioBps",
  "windowStats",
  "validityDeltaSec",
  "recipientIsContract",
];

export function RewriteBanner(): JSX.Element | null {
  const { client } = useExtension();
  const [pendingIds, setPendingIds] = useState<string[] | null>(null);
  const [managed, setManaged] = useState<ManagedPolicy[]>([]);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});
  // Phase 7 carry-over K: when `migration:rewrite` returns
  // `applied: false` the SW auto-acks and the row disappears with no
  // user feedback. We surface a brief inline "No rewrite needed" notice
  // keyed by id; it auto-clears after `INFO_AUTO_CLEAR_MS`.
  const [info, setInfo] = useState<{ id: string; text: string } | null>(null);

  const managedById = useMemo(() => {
    const m = new Map<string, ManagedPolicy>();
    for (const p of managed) m.set(p.id, p);
    return m;
  }, [managed]);

  const refresh = useCallback(async () => {
    try {
      const [{ ids }, mgd] = await Promise.all([
        client.listMigrationPending(),
        client.listManaged(),
      ]);
      setPendingIds(ids);
      setManaged(mgd);
    } catch (err) {
      // Stay quiet on the read path — the banner only needs to surface
      // action-time errors. A failed list call just leaves the banner
      // hidden; the next refresh will pick up.
      console.warn("[RewriteBanner] refresh failed:", err);
      setPendingIds([]);
    }
  }, [client]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Auto-clear the "No rewrite needed" toast after a short delay.
  useEffect(() => {
    if (!info) return;
    const handle = setTimeout(() => setInfo(null), INFO_AUTO_CLEAR_MS);
    return () => clearTimeout(handle);
  }, [info]);

  const onRewrite = useCallback(
    async (id: string) => {
      const policy = managedById.get(id);
      if (!policy) {
        setErrors((prev) => ({
          ...prev,
          [id]: "policy text not found in managed storage",
        }));
        return;
      }
      setBusyId(id);
      setErrors((prev) => {
        const next = { ...prev };
        delete next[id];
        return next;
      });
      try {
        const result = await client.rewritePolicyToCustom({
          id,
          text: policy.text,
          knownFields: V0_KNOWN_FIELDS,
        });
        if (result.applied) {
          // Persist the rewritten text, THEN pop the migration entry.
          // If putRaw throws the id stays on the pending queue so the
          // user can retry.
          await client.putRaw({ id, text: result.rewritten });
          await client.migrationAck(id);
        } else {
          // Phase 7 carry-over K: SW auto-acked (the rewriter found
          // nothing to substitute). Surface that explicitly so the
          // user knows the row didn't disappear because of a hidden
          // error.
          setInfo({
            id,
            text: "No rewrite needed — this policy is already on the v1 layout.",
          });
        }
        await refresh();
      } catch (err) {
        const message =
          err instanceof Error ? err.message : String(err);
        setErrors((prev) => ({ ...prev, [id]: message }));
      } finally {
        setBusyId(null);
      }
    },
    [client, managedById, refresh],
  );

  if (pendingIds === null) return null;
  // When the pending list is empty we may still have an outstanding
  // "No rewrite needed" toast from the last `applied: false` action.
  // Render a slim variant of the banner so the toast is visible.
  if (pendingIds.length === 0) {
    if (!info) return null;
    return (
      <div className="rewrite-banner rewrite-banner-info-only" role="status">
        <div
          className="rewrite-banner-info"
          data-testid="rewrite-banner-info"
        >
          {info.text}
        </div>
      </div>
    );
  }

  const noun = pendingIds.length === 1 ? "policy needs" : "policies need";

  return (
    <div className="rewrite-banner" role="alert">
      <div className="rewrite-banner-head">
        <strong>
          {pendingIds.length} {noun} migration to <code>context.custom.*</code>
        </strong>
        <button
          type="button"
          className="rewrite-banner-refresh"
          onClick={() => void refresh()}
        >
          새로고침
        </button>
      </div>
      {info ? (
        <div
          className="rewrite-banner-info"
          data-testid="rewrite-banner-info"
        >
          {info.text}
        </div>
      ) : null}
      <p className="rewrite-banner-sub">
        이전 버전에서 작성된 정책들이 새로운 스키마에 맞춰
        <code>context.custom.</code>으로 옮겨져야 합니다. 자동 변환을 시도하려면
        "Rewrite"를 누르세요. 알려진 필드만 변환되며 변환 실패는 그대로
        남습니다.
      </p>
      <ul className="rewrite-banner-list">
        {pendingIds.map((id) => {
          const hasText = managedById.has(id);
          const err = errors[id];
          return (
            <li key={id} className="rewrite-banner-row">
              <code className="rewrite-banner-id">{id}</code>
              {hasText ? (
                <button
                  type="button"
                  className="rewrite-banner-action"
                  onClick={() => void onRewrite(id)}
                  disabled={busyId === id}
                >
                  {busyId === id ? "Rewriting…" : "Rewrite"}
                </button>
              ) : (
                <span className="rewrite-banner-missing">
                  본문이 저장소에 없음
                </span>
              )}
              {err ? (
                <span className="rewrite-banner-err">{err}</span>
              ) : null}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
