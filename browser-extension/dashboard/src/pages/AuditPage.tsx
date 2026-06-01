import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  exportAuditCsv,
  getAuditCounts,
  listAuditVerdicts,
  listPolicies,
  setVerdictDecision,
  type VerdictDto,
  type VerdictListOpts,
  type VerdictRangeAlias,
  type Verdict,
} from "../server-api";
import { Topbar } from "../shell/Topbar";

import "./verdicts.css";

/**
 * Audit page — filtered verdict log (minute-precision view).
 * - Time range (1h / 6h / 24h / 7d).
 * - Verdict filter (all / pass / warn / fail).
 * - Policy filter (extension-local placeholder until policy storage is wired).
 * - Free-text search.
 * - Decision PATCH (Trust / Cancel) for warn rows.
 * - CSV export — opens the export URL with the JWT in a query param
 *   (since <a download> can't set headers).
 */
export function AuditPage() {
  const [range, setRange] = useState<VerdictRangeAlias>("24h");
  const [verdict, setVerdict] = useState<Verdict | "">("");
  const [policyId, setPolicyId] = useState<number | "">("");
  const [search, setSearch] = useState("");

  const opts = useMemo<VerdictListOpts>(
    () => ({
      range,
      verdict: verdict || undefined,
      policy_id: policyId === "" ? undefined : Number(policyId),
      search: search.trim() || undefined,
      limit: 100,
    }),
    [range, verdict, policyId, search],
  );

  const listQ = useQuery({
    queryKey: ["audit", "list", opts],
    queryFn: () => listAuditVerdicts(opts),
  });
  const countsQ = useQuery({
    queryKey: ["audit", "counts", opts],
    queryFn: () => getAuditCounts(opts),
  });
  const policiesQ = useQuery({ queryKey: ["policies"], queryFn: listPolicies });

  const onExport = async () => {
    // SW returns the CSV body directly (storage-local has no server route to
    // open); wrap it in an object URL + anchor download. Revoked after
    // ~30s — long enough for browsers to dispatch the download dialog.
    try {
      const blob = await exportAuditCsv(opts);
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `audit-${Date.now()}.csv`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      setTimeout(() => URL.revokeObjectURL(url), 30_000);
    } catch (err) {
      console.error("CSV export failed", err);
    }
  };

  return (
    <>
      <Topbar
        here="Audit"
        subtitle={`최근 ${range}`}
        counts={countsQ.data}
      />
      <div className="v-toolbar">
        <label>
          기간
          <select value={range} onChange={(e) => setRange(e.target.value as VerdictRangeAlias)}>
            <option value="1h">1h</option>
            <option value="6h">6h</option>
            <option value="24h">24h</option>
            <option value="7d">7d</option>
          </select>
        </label>
        <label>
          판정
          <select value={verdict} onChange={(e) => setVerdict(e.target.value as Verdict | "")}>
            <option value="">전체</option>
            <option value="pass">pass</option>
            <option value="warn">warn</option>
            <option value="fail">fail</option>
          </select>
        </label>
        <label>
          정책
          <select
            value={policyId}
            onChange={(e) => setPolicyId(e.target.value === "" ? "" : Number(e.target.value))}
          >
            <option value="">전체</option>
            {policiesQ.data?.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          검색
          <input
            type="text"
            placeholder="reason / policy 이름"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </label>
        <span className="spacer" />
        {countsQ.data && (
          <span style={{ fontSize: 12, color: "var(--slate-500)", display: "flex", gap: 8 }}>
            <span>PASS {countsQ.data.pass}</span>
            <span>WARN {countsQ.data.warn}</span>
            <span>FAIL {countsQ.data.fail}</span>
          </span>
        )}
        <button className="icon-btn" onClick={onExport} title="CSV 내보내기" aria-label="export">
          CSV ↓
        </button>
      </div>

      {listQ.error && <div className="err-banner">불러오기 실패: {String(listQ.error)}</div>}

      <div className="v-table-wrap">
        <table className="v-table">
          <thead>
            <tr>
              <th style={{ width: 70 }}>판정</th>
              <th style={{ width: 80 }}>시각</th>
              <th>dApp / 함수</th>
              <th>지갑</th>
              <th>정책</th>
              <th>이유</th>
              <th style={{ width: 140 }}>처리</th>
            </tr>
          </thead>
          <tbody>
            {listQ.isLoading && (
              <tr>
                <td colSpan={7} className="v-empty">불러오는 중…</td>
              </tr>
            )}
            {!listQ.isLoading && listQ.data && listQ.data.length === 0 && (
              <tr>
                <td colSpan={7} className="v-empty">조건에 맞는 verdict가 없습니다</td>
              </tr>
            )}
            {listQ.data?.map((v) => <VerdictRow key={v.id} v={v} />)}
          </tbody>
        </table>
      </div>
    </>
  );
}

function VerdictRow({ v }: { v: VerdictDto }) {
  const qc = useQueryClient();
  const decideMut = useMutation({
    mutationFn: (decision: "trusted" | "cancelled") => setVerdictDecision(v.id, decision),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["audit"] }),
  });
  const fn = v.decoded_fn ?? v.method ?? "—";
  const origin = v.dapp_origin ?? "—";
  const reason = v.reason?.ko ?? v.reason?.en ?? "—";
  const policyName = v.policy?.name ?? "—";
  return (
    <tr>
      <td>
        <span className={`sev-pill ${v.verdict}`}><span className="pd" />{v.verdict}</span>
      </td>
      <td className="mono">{fmtTime(v.ts)}</td>
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
      <td className="actions-cell">
        {v.user_decision === "trusted" && <span className="deco-trusted">신뢰됨</span>}
        {v.user_decision === "cancelled" && <span className="deco-cancelled">무시됨</span>}
        {v.user_decision === null && v.verdict !== "pass" && (
          <div className="pill-row">
            <button
              className="btn primary"
              disabled={decideMut.isPending}
              onClick={() => decideMut.mutate("trusted")}
            >
              신뢰
            </button>
            <button
              className="btn"
              disabled={decideMut.isPending}
              onClick={() => decideMut.mutate("cancelled")}
            >
              무시
            </button>
          </div>
        )}
      </td>
    </tr>
  );
}

function fmtTime(unixSec: number): string {
  const d = new Date(unixSec * 1000);
  return d.toLocaleTimeString("ko-KR", { hour: "2-digit", minute: "2-digit" });
}

function shortAddr(addr: string): string {
  if (!addr || addr.length < 12) return addr;
  return `${addr.slice(0, 6)}···${addr.slice(-4)}`;
}
