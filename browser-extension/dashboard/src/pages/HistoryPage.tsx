import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";

import {
  getDiagnosisContextRow,
  getStateDeltaRow,
  listHistoryVerdicts,
  listManagedPolicies,
  type StateDeltaRow,
  type VerdictDto,
  type VerdictListOpts,
  type VerdictRangeAlias,
} from "../server-api";
import { PolicyDiagnosisByText } from "../cedar/diagram/PolicyDiagnosisByText";
import { Topbar } from "../shell/Topbar";
import {
  formatBalance,
  formatSignedDelta,
  isStateDeltaEmpty,
  parseStateDelta,
} from "./simulation/state-view";

import "./verdicts.css";

const PAGE_SIZE = 50;
type Verdict = VerdictDto["verdict"];
type GroupMode = "time" | "activity" | "verdict" | "origin" | "rule";

interface RangeOption {
  id: VerdictRangeAlias | "all";
  label: string;
}
const RANGE_OPTIONS: readonly RangeOption[] = [
  { id: "all", label: "전체" },
  { id: "1h", label: "1h" },
  { id: "6h", label: "6h" },
  { id: "24h", label: "24h" },
  { id: "7d", label: "7일" },
];

const GROUP_OPTIONS: readonly { id: GroupMode; label: string }[] = [
  { id: "time", label: "시간순" },
  { id: "activity", label: "활동별" },
  { id: "verdict", label: "verdict별" },
  { id: "origin", label: "dApp별" },
  { id: "rule", label: "rule별" },
];

/** Adjacent-row time gap (seconds) under which two txs from the same
 *  (wallet, dApp origin) are treated as one user activity — e.g. the
 *  approve + swap a dApp fires for a single intent. dApps expose no
 *  correlation id, so this proximity heuristic is the grouping signal. */
const ACTIVITY_WINDOW_SEC = 3;

const VERDICT_ORDER: readonly Verdict[] = ["fail", "warn", "pass"];

/**
 * History page — forensic ledger of verdicts.
 * - Newest first, second-precision timestamps + sequence column (id desc).
 * - Cursor pagination via `before` (id of last loaded row).
 * - "Load more" button (intentionally not auto-scroll — keeps it deterministic
 *   and avoids racy refetches).
 * - Range filter (1h / 24h / 7d / all) — drives the server query.
 * - Local search, verdict-pill toggles, and grouping mode (time / verdict /
 *   dApp / rule). All four operate on the rows already fetched so they're
 *   instant; the server-side range filter is the only one that triggers a
 *   refetch.
 * - Row click → inline detail panel with the fields not shown in the summary
 *   row (RPC method, contract address + selector, full reason text). Mirrors
 *   the original v3 "why panel".
 */
export function HistoryPage() {
  const [rangeId, setRangeId] = useState<VerdictRangeAlias | "all">("1h");
  const [pages, setPages] = useState<VerdictDto[][]>([]);
  // Cursor is a unix-seconds timestamp (`before`) — the storage layer
  // paginates by `ts`, not by autoincrement id (which is now a UUID string).
  const [cursor, setCursor] = useState<number | undefined>(undefined);
  const [doneLoadingMore, setDoneLoadingMore] = useState(false);
  const [openId, setOpenId] = useState<string | null>(null);
  const [q, setQ] = useState("");
  const [verdictFilter, setVerdictFilter] = useState<Set<Verdict>>(new Set());
  const [groupMode, setGroupMode] = useState<GroupMode>("time");
  // Keys of expanded groups. Empty = all collapsed (the default for any grouped
  // mode); the flat "time" view has no headers so it's unaffected. Reset
  // whenever the grouping changes so a new layout starts collapsed.
  const [openGroups, setOpenGroups] = useState<Set<string>>(new Set());
  useEffect(() => {
    setOpenGroups(new Set());
  }, [groupMode]);
  const toggleGroup = (key: string) =>
    setOpenGroups((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  const seenIds = useRef(new Set<string>());

  const baseOpts = useMemo<VerdictListOpts>(
    () => ({
      range: rangeId === "all" ? undefined : rangeId,
      limit: PAGE_SIZE,
    }),
    [rangeId],
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

  const filteredRows = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return allRows.filter((v) => {
      if (verdictFilter.size > 0 && !verdictFilter.has(v.verdict)) return false;
      if (needle) {
        const haystack = [
          v.wallet,
          v.dapp_origin,
          v.decoded_fn,
          v.method,
          v.policy?.name,
          v.reason?.ko,
          v.reason?.en,
          v.contract?.addr,
          v.selector?.sig,
          v.selector?.decoded,
        ]
          .filter(Boolean)
          .join(" ")
          .toLowerCase();
        if (!haystack.includes(needle)) return false;
      }
      return true;
    });
  }, [allRows, q, verdictFilter]);

  // Managed policies → Cedar `@id` → text, so a deny row can resolve its
  // policy back to source for the structure diagram + diagnosis.
  const managedQ = useQuery({
    queryKey: ["history-managed-policies"],
    queryFn: listManagedPolicies,
    staleTime: 60_000,
  });
  const managedPolicies = useMemo<ManagedPolicyEntry[]>(
    () =>
      (managedQ.data ?? []).map((p) => ({
        dashId: p.id,
        cedarId: p.text.match(/@id\("([^"]+)"\)/)?.[1] ?? null,
        text: p.text,
        manifest:
          p.manifest && typeof p.manifest === "object"
            ? p.manifest
            : { id: p.id, schema_version: 2 },
      })),
    [managedQ.data],
  );

  const counts = useMemo(() => {
    let pass = 0;
    let warn = 0;
    let fail = 0;
    for (const v of filteredRows) {
      if (v.verdict === "pass") pass += 1;
      else if (v.verdict === "warn") warn += 1;
      else if (v.verdict === "fail") fail += 1;
    }
    return { total: filteredRows.length, pass, warn, fail };
  }, [filteredRows]);

  const groups = useMemo(
    () => buildGroups(filteredRows, groupMode),
    [filteredRows, groupMode],
  );

  const toggleVerdict = (v: Verdict) => {
    setVerdictFilter((prev) => {
      const next = new Set(prev);
      if (next.has(v)) next.delete(v);
      else next.add(v);
      return next;
    });
  };

  const anyClientFilter = q.trim() !== "" || verdictFilter.size > 0;
  const onResetClient = () => {
    setQ("");
    setVerdictFilter(new Set());
  };

  return (
    <>
      <Topbar
        here="History"
        subtitle={`${allRows.length}건 로드`}
      />

      <FilterBar
        rangeId={rangeId}
        setRangeId={setRangeId}
        q={q}
        setQ={setQ}
        verdictFilter={verdictFilter}
        toggleVerdict={toggleVerdict}
        groupMode={groupMode}
        setGroupMode={setGroupMode}
        counts={counts}
        anyClientFilter={anyClientFilter}
        onResetClient={onResetClient}
      />

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
            {!firstQ.isLoading && allRows.length > 0 && filteredRows.length === 0 && (
              <tr>
                <td colSpan={9} className="v-empty">
                  조건에 맞는 verdict가 없어요 — 검색어/필터를 조정해 보세요
                </td>
              </tr>
            )}
            {groups.map((g) => {
              // Grouped modes (label present) collapse; the flat "time" view
              // (label null) always shows its rows.
              const collapsible = g.label !== null;
              const isOpen = !collapsible || openGroups.has(g.key);
              return (
                <Fragment key={g.key}>
                  {g.label && (
                    <GroupHeaderRow
                      group={g}
                      open={isOpen}
                      onToggle={() => toggleGroup(g.key)}
                    />
                  )}
                  {isOpen &&
                    g.rows.map((v) => (
                      <HistoryRow
                        key={v.id}
                        v={v}
                        open={openId === v.id}
                        onToggle={() => setOpenId(openId === v.id ? null : v.id)}
                        managedPolicies={managedPolicies}
                      />
                    ))}
                </Fragment>
              );
            })}
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

// ── Filter bar ──────────────────────────────────────────────────────────

function FilterBar({
  rangeId,
  setRangeId,
  q,
  setQ,
  verdictFilter,
  toggleVerdict,
  groupMode,
  setGroupMode,
  counts,
  anyClientFilter,
  onResetClient,
}: {
  rangeId: VerdictRangeAlias | "all";
  setRangeId: (id: VerdictRangeAlias | "all") => void;
  q: string;
  setQ: (q: string) => void;
  verdictFilter: Set<Verdict>;
  toggleVerdict: (v: Verdict) => void;
  groupMode: GroupMode;
  setGroupMode: (g: GroupMode) => void;
  counts: { total: number; pass: number; warn: number; fail: number };
  anyClientFilter: boolean;
  onResetClient: () => void;
}) {
  return (
    <div className="filter-bar">
      <div className="filter-row range-row">
        <span className="range-label">
          <ClockIcon /> 전체기간
        </span>
        <div className="seg-group" role="tablist" aria-label="time range">
          {RANGE_OPTIONS.filter((r) => r.id !== "all").map((r) => (
            <button
              key={r.id}
              role="tab"
              aria-selected={rangeId === r.id}
              className={`seg-btn${rangeId === r.id ? " on" : ""}`}
              onClick={() => setRangeId(r.id)}
            >
              {r.label}
            </button>
          ))}
        </div>
        <button
          className={`range-all${rangeId === "all" ? " on" : ""}`}
          onClick={() => setRangeId("all")}
        >
          전체
        </button>
        {rangeId !== "all" && (
          <span className="range-hint">현재 기준 롤링 윈도우</span>
        )}
        <div className="counts">
          <span className="count-total">{counts.total}건</span>
          {counts.warn > 0 && (
            <span className="count-chip warn">{counts.warn} warn</span>
          )}
          {counts.fail > 0 && (
            <span className="count-chip fail">{counts.fail} fail</span>
          )}
          {counts.pass > 0 && (
            <span className="count-chip pass">{counts.pass} pass</span>
          )}
        </div>
      </div>

      <div className="filter-row tool-row">
        <div className="search-wrap-hist">
          <SearchIcon />
          <input
            type="text"
            placeholder="주소 · dApp 출처 · 함수명 · 정책명 검색"
            value={q}
            onChange={(e) => setQ(e.target.value)}
          />
          {q && (
            <button className="search-clear" onClick={() => setQ("")} aria-label="clear">
              <XIcon />
            </button>
          )}
        </div>

        <div className="verdict-toggles" role="group" aria-label="verdict filter">
          {VERDICT_ORDER.map((v) => {
            const on = verdictFilter.has(v);
            return (
              <button
                key={v}
                type="button"
                className={`vtoggle ${v}${on ? " on" : ""}`}
                aria-pressed={on}
                onClick={() => toggleVerdict(v)}
              >
                <VerdictIcon kind={v} /> {v.toUpperCase()}
              </button>
            );
          })}
        </div>

        <span className="filter-sep" />

        <span className="group-label">
          <LayersIcon /> 그룹
        </span>
        <div className="seg-group" role="tablist" aria-label="grouping">
          {GROUP_OPTIONS.map((g) => (
            <button
              key={g.id}
              role="tab"
              aria-selected={groupMode === g.id}
              className={`seg-btn${groupMode === g.id ? " on" : ""}`}
              onClick={() => setGroupMode(g.id)}
            >
              {g.label}
            </button>
          ))}
        </div>

        {anyClientFilter && (
          <button className="filter-reset" onClick={onResetClient}>
            초기화
          </button>
        )}
      </div>
    </div>
  );
}

// ── Group section header ────────────────────────────────────────────────

function GroupHeaderRow({
  group,
  open,
  onToggle,
}: {
  group: RenderGroup;
  open: boolean;
  onToggle: () => void;
}) {
  const c = group.byVerdict ?? { pass: 0, warn: 0, fail: 0 };
  // Any fail in the group turns the whole header red, even when the group
  // isn't a verdict-typed bucket (e.g. an activity cluster).
  const hasFail = c.fail > 0;
  return (
    <tr
      className={`v-group-head${group.verdictKind ? ` gh-${group.verdictKind}` : ""}${
        hasFail ? " gh-has-fail" : ""
      }`}
      onClick={onToggle}
      aria-expanded={open}
    >
      <td colSpan={9}>
        <div className="gh-row">
          <span className="gh-caret" aria-hidden="true">
            {open ? "▾" : "▸"}
          </span>
          <span className="gh-title">{group.label}</span>
          <span className="gh-n">{group.rows.length}건</span>
          {!group.verdictKind && (
            <span className="gh-mini">
              {c.fail > 0 && <span className="mini-fail">{c.fail} fail</span>}
              {c.warn > 0 && <span className="mini-warn">{c.warn} warn</span>}
              {c.pass > 0 && <span className="mini-pass">{c.pass} pass</span>}
            </span>
          )}
        </div>
      </td>
    </tr>
  );
}

// ── Build groups ────────────────────────────────────────────────────────

interface RenderGroup {
  key: string;
  label: string | null;
  verdictKind?: Verdict;
  byVerdict?: { pass: number; warn: number; fail: number };
  rows: VerdictDto[];
}

function buildGroups(rows: VerdictDto[], mode: GroupMode): RenderGroup[] {
  if (mode === "time") {
    return [{ key: "time", label: null, rows }];
  }
  if (mode === "activity") {
    // One "activity" = the txs a dApp fires for a single user intent (e.g.
    // approve + swap). dApps give no correlation id, so cluster consecutive
    // rows from the same (wallet, dApp origin) whose adjacent time gap is
    // ≤ ACTIVITY_WINDOW_SEC. Rows sharing a `delta_id` (the SAME tx evaluated
    // against N policies) always stay together regardless of the window.
    const sorted = [...rows].sort((a, b) => b.ts - a.ts); // newest first
    const sameActor = (a: VerdictDto, b: VerdictDto) =>
      (a.wallet ?? "") === (b.wallet ?? "") &&
      (a.dapp_origin ?? "") === (b.dapp_origin ?? "");
    const groups: RenderGroup[] = [];
    let cur: VerdictDto[] = [];
    const flush = () => {
      if (cur.length === 0) return;
      const c = { pass: 0, warn: 0, fail: 0 };
      for (const r of cur) c[r.verdict] += 1;
      const head = cur[0];
      groups.push({
        key: `activity-${head.id}`,
        label: `${head.dapp_origin ?? "(unknown origin)"} · ${fmtTs(head.ts)}`,
        byVerdict: c,
        rows: cur,
      });
      cur = [];
    };
    for (const r of sorted) {
      if (cur.length === 0) {
        cur = [r];
        continue;
      }
      const prev = cur[cur.length - 1]; // oldest-so-far in the cluster
      const sameTx = r.delta_id != null && r.delta_id === prev.delta_id;
      const adjacent =
        sameActor(r, prev) && prev.ts - r.ts <= ACTIVITY_WINDOW_SEC;
      if (sameTx || adjacent) {
        cur.push(r);
      } else {
        flush();
        cur = [r];
      }
    }
    flush();
    return groups;
  }
  if (mode === "verdict") {
    return VERDICT_ORDER.map((v) => ({
      key: `verdict-${v}`,
      label: v.toUpperCase(),
      verdictKind: v,
      rows: rows.filter((r) => r.verdict === v),
    })).filter((g) => g.rows.length > 0);
  }
  // origin / rule — bucket by string key, sort buckets by worst severity then size.
  const keyFn =
    mode === "origin"
      ? (r: VerdictDto) => r.dapp_origin ?? "(unknown origin)"
      : (r: VerdictDto) => r.policy?.name ?? "(no policy)";
  const map = new Map<string, VerdictDto[]>();
  for (const r of rows) {
    const k = keyFn(r);
    if (!map.has(k)) map.set(k, []);
    map.get(k)!.push(r);
  }
  const groups: RenderGroup[] = [...map.entries()].map(([label, rs]) => {
    const c = { pass: 0, warn: 0, fail: 0 };
    for (const r of rs) c[r.verdict] += 1;
    return {
      key: `${mode}-${label}`,
      label,
      byVerdict: c,
      rows: rs,
    };
  });
  const rank = (g: RenderGroup) => {
    const c = g.byVerdict!;
    if (c.fail > 0) return 0;
    if (c.warn > 0) return 1;
    return 2;
  };
  groups.sort((a, b) => rank(a) - rank(b) || b.rows.length - a.rows.length);
  return groups;
}

// ── Row + detail ────────────────────────────────────────────────────────

/** A managed policy keyed by BOTH its unique dashboard id and its (possibly
 *  duplicated) Cedar `@id`, so a deny row can resolve the policy that actually
 *  fired — via the manifest id in the captured results — even when two policies
 *  share an `@id`. */
interface ManagedPolicyEntry {
  dashId: string;
  cedarId: string | null;
  text: string;
  manifest: unknown;
}

function HistoryRow({
  v,
  open,
  onToggle,
  managedPolicies,
}: {
  v: VerdictDto;
  open: boolean;
  onToggle: () => void;
  managedPolicies: ManagedPolicyEntry[];
}) {
  const fn = v.decoded_fn ?? v.method ?? "—";
  const origin = v.dapp_origin ?? "—";
  const reason = v.reason?.ko ?? v.reason?.en ?? "—";
  const policyName = v.policy?.name ?? "—";
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
            <HistoryDetail v={v} managedPolicies={managedPolicies} />
          </td>
        </tr>
      )}
    </Fragment>
  );
}

function HistoryDetail({
  v,
  managedPolicies,
}: {
  v: VerdictDto;
  managedPolicies: ManagedPolicyEntry[];
}) {
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

      {/* State-delta section: fetches the SW's `state-deltas:log` row by
          `delta_id` and renders the reducer-side delta + a re-simulate
          link. The fetch is lazy (only fires when the row is expanded). */}
      <StateDeltaSection v={v} />

      {/* Policy structure + denial diagnosis: for a deny whose policy we can
          resolve back to its Cedar source. */}
      {v.verdict === "fail" && v.policy?.name && (
        <PolicyStructureSection
          cedarId={v.policy.name}
          deltaId={v.delta_id}
          managedPolicies={managedPolicies}
        />
      )}
    </div>
  );
}

/** The manifest id embedded in a captured-results key (`<manifestId>::<callId>`),
 *  i.e. the dashboard id of the policy that produced that enrichment value. */
function manifestIdsOf(results: Record<string, unknown>): Set<string> {
  const out = new Set<string>();
  for (const k of Object.keys(results)) {
    const parts = k.split("::");
    if (parts.length >= 2) out.add(parts.slice(0, -1).join("::"));
  }
  return out;
}

/** Collapsible policy structure diagram + "where it's blocked" diagnosis for a
 *  history deny row. When the live deny's context was captured (keyed by
 *  delta_id), the diagnosis auto-runs against that REAL context — reproducing
 *  the actual blocked clause; otherwise it falls back to the structure only.
 *
 *  Resolving the policy by its (possibly duplicated) Cedar `@id` alone is
 *  ambiguous — two policies can share an `@id`. So when context exists we first
 *  pick the policy whose DASHBOARD id appears in the captured results (the one
 *  that actually enriched this deny), falling back to the `@id` match. This is
 *  why a USD deny no longer renders a same-`@id` nano policy's diagram. */
function PolicyStructureSection({
  cedarId,
  deltaId,
  managedPolicies,
}: {
  cedarId: string;
  deltaId: string | null;
  managedPolicies: ManagedPolicyEntry[];
}) {
  const [open, setOpen] = useState(false);
  const ctxQ = useQuery({
    queryKey: ["diagnosis-context", deltaId],
    queryFn: () =>
      deltaId ? getDiagnosisContextRow(deltaId) : Promise.resolve(null),
    enabled: open && !!deltaId,
    retry: false,
  });
  const ctx = ctxQ.data ?? null;

  const byCedarId = managedPolicies.filter((p) => p.cedarId === cedarId);
  // Prefer the policy that actually enriched this deny (its dashboard id is in
  // the captured results), disambiguating a shared @id; else the @id match.
  const resolved =
    (ctx &&
      byCedarId.find((p) => manifestIdsOf(ctx.results).has(p.dashId))) ||
    byCedarId[0] ||
    null;

  const request =
    ctx && resolved
      ? {
          action: ctx.action,
          meta: ctx.meta,
          tx: ctx.tx,
          bundles: [{ policy: resolved.text, manifest: resolved.manifest }],
          results: ctx.results,
        }
      : undefined;

  if (!resolved && !ctxQ.isLoading) {
    return null; // can't resolve the policy back to source — nothing to draw
  }

  return (
    <div className="v-struct-section">
      <button
        type="button"
        className="v-struct-toggle"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
      >
        {open ? "정책 구조 숨기기 ▲" : "정책 구조 보기 ▼"}
      </button>
      {open && (
        <>
          {!ctxQ.isLoading && !ctx && (
            <div className="v-struct-note">
              이전 거래라 진단 컨텍스트가 저장되지 않아 실제 막힌 조건은 표시할 수
              없어요. 구조만 보여드립니다 — 이후 거래의 deny부터 실제 차단 조건이
              빨갛게 표시됩니다.
            </div>
          )}
          <div className="v-struct-body">
            {ctxQ.isLoading || !resolved ? (
              <div className="pdiagram-empty">불러오는 중…</div>
            ) : ctx ? (
              <PolicyDiagnosisByText
                cedarText={resolved.text}
                compact
                request={request}
                autoRun
              />
            ) : (
              <PolicyDiagnosisByText
                cedarText={resolved.text}
                compact
                structureOnly
              />
            )}
          </div>
        </>
      )}
    </div>
  );
}

function StateDeltaSection({ v }: { v: VerdictDto }) {
  // Skip the whole section for legacy rows (delta_id stamped before the
  // schema migration carried a numeric placeholder we lost; new rows
  // either have a UUID or null).
  const deltaId =
    typeof v.delta_id === "string" && v.delta_id.length > 0
      ? v.delta_id
      : null;

  const q = useQuery({
    queryKey: ["state-delta", deltaId],
    queryFn: () => (deltaId ? getStateDeltaRow(deltaId) : Promise.resolve(null)),
    enabled: deltaId !== null,
  });

  if (!deltaId) {
    return (
      <div className="v-delta-section">
        <header className="v-delta-head">
          <strong>State-diff</strong>
          <span className="v-empty-inline">
            기록 없음 (마이그레이션 이전 verdict)
          </span>
        </header>
      </div>
    );
  }

  if (q.isLoading) {
    return (
      <div className="v-delta-section">
        <header className="v-delta-head">
          <strong>State-diff</strong>
          <span className="v-empty-inline">불러오는 중…</span>
        </header>
      </div>
    );
  }

  const row = q.data;
  if (!row) {
    return (
      <div className="v-delta-section">
        <header className="v-delta-head">
          <strong>State-diff</strong>
          <span className="v-empty-inline">
            서버 기록 없음 (로그아웃 상태이거나 서버 통신 실패)
          </span>
        </header>
      </div>
    );
  }

  return (
    <div className="v-delta-section">
      <header className="v-delta-head">
        <strong>State-diff</strong>
        <ReSimLink row={row} />
      </header>
      <DeltaRows row={row} />
    </div>
  );
}

/** Render the typed projection of `row.delta` — token / position / pending
 *  changes + gas. Mirrors the per-step rendering the simulator uses so
 *  history and live sim look consistent. */
function DeltaRows({ row }: { row: StateDeltaRow }) {
  const view = useMemo(() => parseStateDelta(row.delta as Record<string, unknown>), [
    row.delta,
  ]);

  if (isStateDeltaEmpty(view)) {
    return <div className="v-delta-empty">no state change</div>;
  }

  return (
    <ul className="v-delta-rows">
      {view.tokenChanges.map((t, i) => {
        if (t.kind === "balance_delta") {
          return (
            <li key={`tc-${i}`} className="v-delta-row">
              <span className="v-delta-tag">balance</span>
              <code>{shortAddr(t.key.address)}</code>
              <span className="v-delta-chain">{t.key.chain}</span>
              <span className="v-delta-amt">
                {/* The delta string is signed decimal at raw precision —
                    we don't know the token's decimals here without a
                    catalog lookup, so render the raw signed value. */}
                {formatSignedDelta(t.delta, 0)}
              </span>
            </li>
          );
        }
        if (t.kind === "approval_set") {
          return (
            <li key={`tc-${i}`} className="v-delta-row">
              <span className="v-delta-tag">approve</span>
              <code>{shortAddr(t.key.address)}</code>
              <span className="v-delta-arrow">→</span>
              <code>{shortAddr(t.spender)}</code>
            </li>
          );
        }
        return (
          <li key={`tc-${i}`} className="v-delta-row">
            <span className="v-delta-tag">revoke</span>
            <code>{shortAddr(t.key.address)}</code>
            <span className="v-delta-arrow">→</span>
            <code>{shortAddr(t.spender)}</code>
            <span className="v-delta-scope">{t.scope}</span>
          </li>
        );
      })}
      {view.positionChanges.map((p, i) => (
        <li key={`pc-${i}`} className="v-delta-row">
          <span className="v-delta-tag">position</span>
          <span className="v-delta-kind">{p.kind}</span>
          {p.id && <code>{shortAddr(p.id)}</code>}
        </li>
      ))}
      {view.pendingChanges.map((p, i) => (
        <li key={`pe-${i}`} className="v-delta-row">
          <span className="v-delta-tag">pending</span>
          <span className="v-delta-kind">{p.kind}</span>
        </li>
      ))}
      {view.gasPaid && (
        <li className="v-delta-row">
          <span className="v-delta-tag">gas</span>
          <code>{shortAddr(view.gasPaid.token.address)}</code>
          <span className="v-delta-amt neg">
            -{formatBalance(view.gasPaid.amount, 0)}
          </span>
        </li>
      )}
    </ul>
  );
}

/** "다시 시뮬" — feeds the row's tx fields back into SimulationPage as
 *  query params. SimulationPage's `?from=&to=&calldata=&value=&chain=`
 *  parser populates the first row on mount, so the user lands on a
 *  ready-to-run sim with the same calldata that produced this verdict. */
function ReSimLink({ row }: { row: StateDeltaRow }) {
  const qs = new URLSearchParams();
  qs.set("from", row.from);
  qs.set("to", row.to);
  qs.set("calldata", row.calldata || "0x");
  qs.set("value", row.value || "0");
  qs.set("chain", row.chain);
  return (
    <Link to={`/simulation?${qs.toString()}`} className="v-delta-resim">
      🧪 다시 시뮬
    </Link>
  );
}

// ── Icons ───────────────────────────────────────────────────────────────

function ClockIcon() {
  return (
    <svg viewBox="0 0 24 24" width={14} height={14} fill="none" stroke="currentColor" strokeWidth={1.9} strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3 2" />
    </svg>
  );
}
function SearchIcon() {
  return (
    <svg viewBox="0 0 24 24" width={14} height={14} fill="none" stroke="currentColor" strokeWidth={1.9} strokeLinecap="round" strokeLinejoin="round">
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-3.2-3.2" />
    </svg>
  );
}
function XIcon() {
  return (
    <svg viewBox="0 0 24 24" width={12} height={12} fill="none" stroke="currentColor" strokeWidth={2.2} strokeLinecap="round" strokeLinejoin="round">
      <path d="M6 6l12 12M18 6 6 18" />
    </svg>
  );
}
function LayersIcon() {
  return (
    <svg viewBox="0 0 24 24" width={14} height={14} fill="none" stroke="currentColor" strokeWidth={1.9} strokeLinecap="round" strokeLinejoin="round">
      <path d="m12 3 9 5-9 5-9-5 9-5Z" />
      <path d="m3 13 9 5 9-5" />
    </svg>
  );
}
function VerdictIcon({ kind }: { kind: Verdict }) {
  if (kind === "pass") {
    return (
      <svg viewBox="0 0 24 24" width={13} height={13} fill="none" stroke="currentColor" strokeWidth={2.4} strokeLinecap="round" strokeLinejoin="round">
        <path d="M20 6 9 17l-5-5" />
      </svg>
    );
  }
  if (kind === "warn") {
    return (
      <svg viewBox="0 0 24 24" width={13} height={13} fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
        <path d="M10.3 3.8 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.8a2 2 0 0 0-3.4 0Z" />
        <path d="M12 9v4M12 17h.01" />
      </svg>
    );
  }
  return (
    <svg viewBox="0 0 24 24" width={13} height={13} fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="9" />
      <path d="M5.6 5.6 18.4 18.4" />
    </svg>
  );
}

// ── helpers ─────────────────────────────────────────────────────────────

function fmtTs(unixSec: number): string {
  const d = new Date(unixSec * 1000);
  return d.toLocaleString("ko-KR", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function shortAddr(addr: string): string {
  if (!addr || addr.length < 12) return addr;
  return `${addr.slice(0, 6)}···${addr.slice(-4)}`;
}
