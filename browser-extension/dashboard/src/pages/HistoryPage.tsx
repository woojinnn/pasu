import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import {
  listHistoryVerdicts,
  type VerdictDto,
  type VerdictListOpts,
  type VerdictRangeAlias,
} from "../server-api";
import { Topbar } from "../shell/Topbar";

import "./verdicts.css";

const PAGE_SIZE = 50;

/**
 * History page — forensic ledger of verdicts.
 * - Newest first, second-precision timestamps + sequence column (id desc).
 * - Cursor pagination via `before` (id of last loaded row).
 * - "Load more" button (intentionally not auto-scroll — keeps it deterministic
 *   and avoids racy refetches).
 * - Range filter (1h / 24h / 7d / all).
 */
export function HistoryPage() {
  const [range, setRange] = useState<VerdictRangeAlias | "">("");
  const [pages, setPages] = useState<VerdictDto[][]>([]);
  // Cursor is now a unix-seconds timestamp (`before`) — the storage layer
  // paginates by `ts`, not by autoincrement id (which is now a UUID string).
  const [cursor, setCursor] = useState<number | undefined>(undefined);
  const [doneLoadingMore, setDoneLoadingMore] = useState(false);
  const seenIds = useRef(new Set<string>());

  const baseOpts = useMemo<VerdictListOpts>(
    () => ({ range: range || undefined, limit: PAGE_SIZE }),
    [range],
  );

  // First page (or reset when filters change).
  const firstQ = useQuery({
    queryKey: ["history", "first", baseOpts],
    queryFn: () => listHistoryVerdicts(baseOpts),
  });

  // Whenever the first query refreshes, reset the page stack.
  useEffect(() => {
    if (firstQ.data) {
      seenIds.current = new Set(firstQ.data.map((r) => r.id));
      setPages([firstQ.data]);
      // Cursor is the oldest `ts` in the page — the storage layer keeps
      // rows newest-first and filters by `ts < before` to paginate.
      setCursor(firstQ.data.at(-1)?.ts);
      setDoneLoadingMore(firstQ.data.length < PAGE_SIZE);
    }
  }, [firstQ.data]);

  const loadMoreQ = useQuery({
    queryKey: ["history", "more", baseOpts, cursor],
    queryFn: () => listHistoryVerdicts({ ...baseOpts, before: cursor }),
    enabled: false, // manually triggered
  });

  const onLoadMore = async () => {
    if (cursor === undefined) return;
    const result = await loadMoreQ.refetch();
    const rows = result.data ?? [];
    const newRows = rows.filter((r) => !seenIds.current.has(r.id));
    if (newRows.length === 0) {
      setDoneLoadingMore(true);
      return;
    }
    newRows.forEach((r) => seenIds.current.add(r.id));
    setPages((p) => [...p, newRows]);
    // Advance the cursor by `ts` (oldest row in the new page).
    setCursor(newRows.at(-1)?.ts);
    if (rows.length < PAGE_SIZE) setDoneLoadingMore(true);
  };

  const allRows = pages.flat();

  return (
    <>
      <Topbar
        here="History"
        subtitle={`${allRows.length}건 로드`}
      />
      <div className="v-toolbar">
        <label>
          기간
          <select value={range} onChange={(e) => setRange(e.target.value as VerdictRangeAlias | "")}>
            <option value="">전체</option>
            <option value="1h">1h</option>
            <option value="6h">6h</option>
            <option value="24h">24h</option>
            <option value="7d">7d</option>
          </select>
        </label>
        <span className="spacer" />
      </div>

      {firstQ.error && <div className="err-banner">불러오기 실패: {String(firstQ.error)}</div>}

      <div className="v-table-wrap">
        <table className="v-table">
          <thead>
            <tr>
              <th style={{ width: 70 }}>seq</th>
              <th style={{ width: 70 }}>판정</th>
              <th style={{ width: 130 }}>시각</th>
              <th>dApp / 함수</th>
              <th>지갑</th>
              <th>정책</th>
              <th>이유</th>
              <th style={{ width: 80 }}>처리</th>
            </tr>
          </thead>
          <tbody>
            {firstQ.isLoading && (
              <tr>
                <td colSpan={8} className="v-empty">불러오는 중…</td>
              </tr>
            )}
            {!firstQ.isLoading && allRows.length === 0 && (
              <tr>
                <td colSpan={8} className="v-empty">기록이 없습니다</td>
              </tr>
            )}
            {allRows.map((v) => <HistoryRow key={v.id} v={v} />)}
          </tbody>
        </table>
      </div>

      {!doneLoadingMore && allRows.length > 0 && (
        <button
          className="v-loadmore"
          onClick={onLoadMore}
          disabled={loadMoreQ.isFetching}
        >
          {loadMoreQ.isFetching ? "불러오는 중…" : "더 불러오기"}
        </button>
      )}
    </>
  );
}

function HistoryRow({ v }: { v: VerdictDto }) {
  const fn = v.decoded_fn ?? v.method ?? "—";
  const origin = v.dapp_origin ?? "—";
  const reason = v.reason?.ko ?? v.reason?.en ?? "—";
  const policyName = v.policy?.name ?? "—";
  // The id used to be a tiny autoincrement; now it's a UUID — show the first
  // 8 chars so the column stays narrow while remaining glanceably distinct.
  const shortId = v.id.length > 8 ? v.id.slice(0, 8) : v.id;
  return (
    <tr>
      <td className="seq" title={v.id}>#{shortId}</td>
      <td>
        <span className={`sev-pill ${v.verdict}`}><span className="pd" />{v.verdict}</span>
      </td>
      <td className="mono">{fmtTs(v.ts)}</td>
      <td>
        <div className="strong">{fn}</div>
        <div className="mono">{origin}</div>
      </td>
      <td className="mono">{shortAddr(v.wallet ?? "—")}</td>
      <td>
        <div className="strong">{policyName}</div>
        <div className="mono">{v.policy?.severity ?? ""}</div>
      </td>
      <td className="reason" title={reason}>{reason}</td>
      <td>
        {v.user_decision === "trusted" && <span className="deco-trusted">agree</span>}
        {v.user_decision === "cancelled" && <span className="deco-cancelled">deny</span>}
        {v.user_decision === null && <span className="deco-pending">선택중</span>}
      </td>
    </tr>
  );
}

function fmtTs(unixSec: number): string {
  const d = new Date(unixSec * 1000);
  const dt = d.toLocaleString("ko-KR", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
  return dt;
}

function shortAddr(addr: string): string {
  if (!addr || addr.length < 12) return addr;
  return `${addr.slice(0, 6)}···${addr.slice(-4)}`;
}
