import { Fragment, useEffect, useMemo, useRef, useState } from "react";
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
 * - Row click → inline detail panel with the fields not shown in the summary
 *   row (RPC method, contract address + selector, full reason text). Mirrors
 *   the original v3 "why panel" — keeps the table dense but lets the user
 *   drill into any single verdict without leaving the page.
 */
export function HistoryPage() {
  const [range, setRange] = useState<VerdictRangeAlias | "">("");
  const [pages, setPages] = useState<VerdictDto[][]>([]);
  // Cursor is now a unix-seconds timestamp (`before`) — the storage layer
  // paginates by `ts`, not by autoincrement id (which is now a UUID string).
  const [cursor, setCursor] = useState<number | undefined>(undefined);
  const [doneLoadingMore, setDoneLoadingMore] = useState(false);
  const [openId, setOpenId] = useState<string | null>(null);
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
      // Filter change can wipe the row that was open, so collapse anything
      // we can't see anymore.
      setOpenId(null);
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
              <th style={{ width: 30 }} aria-label="expand" />
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
                <td colSpan={9} className="v-empty">불러오는 중…</td>
              </tr>
            )}
            {!firstQ.isLoading && allRows.length === 0 && (
              <tr>
                <td colSpan={9} className="v-empty">기록이 없습니다</td>
              </tr>
            )}
            {allRows.map((v) => (
              <HistoryRow
                key={v.id}
                v={v}
                open={openId === v.id}
                onToggle={() => setOpenId(openId === v.id ? null : v.id)}
              />
            ))}
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

function HistoryRow({
  v,
  open,
  onToggle,
}: {
  v: VerdictDto;
  open: boolean;
  onToggle: () => void;
}) {
  const fn = v.decoded_fn ?? v.method ?? "—";
  const origin = v.dapp_origin ?? "—";
  const reason = v.reason?.ko ?? v.reason?.en ?? "—";
  const policyName = v.policy?.name ?? "—";
  // The id used to be a tiny autoincrement; now it's a UUID — show the first
  // 8 chars so the column stays narrow while remaining glanceably distinct.
  const shortId = v.id.length > 8 ? v.id.slice(0, 8) : v.id;
  return (
    <Fragment>
      <tr
        className={`v-row${open ? " v-row-open" : ""}`}
        role="button"
        tabIndex={0}
        aria-expanded={open}
        onClick={onToggle}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onToggle();
          }
        }}
      >
        <td className="v-chev-cell" aria-hidden="true">
          <svg
            className={`v-chev${open ? " open" : ""}`}
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2.4}
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="m9 6 6 6-6 6" />
          </svg>
        </td>
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
          {/* PASS auto-passes and FAIL's popup is informational only — neither
              takes user input, so the decision column is left blank. Only WARN
              actually maps to agree/deny/선택중. */}
          {v.verdict === "warn" && v.user_decision === "trusted" && (
            <span className="deco-trusted">agree</span>
          )}
          {v.verdict === "warn" && v.user_decision === "cancelled" && (
            <span className="deco-cancelled">deny</span>
          )}
          {v.verdict === "warn" && v.user_decision === null && (
            <span className="deco-pending">선택중</span>
          )}
        </td>
      </tr>
      {open && (
        <tr className="v-detail-row">
          <td colSpan={9}>
            <HistoryDetail v={v} />
          </td>
        </tr>
      )}
    </Fragment>
  );
}

function HistoryDetail({ v }: { v: VerdictDto }) {
  const reason = v.reason?.ko ?? v.reason?.en ?? null;
  const contractAddr = v.contract?.addr ?? null;
  const contractSymbol = v.contract?.symbol ?? null;
  const selectorSig = v.selector?.sig ?? null;
  const selectorDecoded = v.selector?.decoded ?? null;
  const fullTs = new Date(v.ts * 1000).toLocaleString("ko-KR", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
  return (
    <div className="v-detail">
      <dl className="v-dprops">
        <dt>매칭 정책</dt>
        <dd>
          {v.policy ? (
            <span className={`v-tag-pol ${v.policy.severity ?? ""}`}>
              {v.policy.name ?? "(unnamed)"}
              <span className="v-tp-sev">{v.policy.severity ?? "—"}</span>
            </span>
          ) : (
            <span className="v-empty-inline">매칭된 정책 없음</span>
          )}
        </dd>

        <dt>RPC method</dt>
        <dd><span className="mono">{v.method ?? "—"}</span></dd>

        {v.decoded_fn && (
          <>
            <dt>디코딩된 함수</dt>
            <dd><span className="mono">{v.decoded_fn}</span></dd>
          </>
        )}

        <dt>대상 컨트랙트</dt>
        <dd>
          {contractAddr ? (
            <span className="v-addr-pill">
              <span className="mono">{contractAddr}</span>
              {contractSymbol && <span className="v-sym">{contractSymbol}</span>}
            </span>
          ) : (
            <span className="v-empty-inline">—</span>
          )}
        </dd>

        <dt>셀렉터</dt>
        <dd>
          {selectorSig ? (
            <>
              <span className="mono">{selectorSig}</span>
              {selectorDecoded && (
                <span className="mono v-sel-decoded"> · {selectorDecoded}</span>
              )}
            </>
          ) : (
            <span className="v-empty-inline">—</span>
          )}
        </dd>

        <dt>지갑</dt>
        <dd>
          <span className="mono">{v.wallet ?? "—"}</span>
        </dd>

        <dt>dApp 출처</dt>
        <dd>
          <span className="mono">{v.dapp_origin ?? "—"}</span>
        </dd>

        <dt>판정 시각</dt>
        <dd>
          <span className="mono">{fullTs}</span>
        </dd>

        <dt className="v-dpr-span">사유</dt>
        <dd className="v-dpr-span">
          {reason ? (
            <p className="v-reason-full">{reason}</p>
          ) : (
            <span className="v-empty-inline">기록된 사유 없음</span>
          )}
        </dd>
      </dl>
    </div>
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
