import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import type { AuditEntry, VerdictKind } from "@scopeball/sdk";
import { useExtension } from "../sdk-context";
import "./AuditPage.css";

type KindFilter = "all" | VerdictKind;

const SINCE_OPTIONS: Array<{ value: number; label: string }> = [
  { value: 0, label: "전체 기간" },
  { value: 3600_000, label: "최근 1시간" },
  { value: 6 * 3600_000, label: "최근 6시간" },
  { value: 24 * 3600_000, label: "최근 24시간" },
  { value: 7 * 24 * 3600_000, label: "최근 7일" },
];

export function AuditPage() {
  const { client, managed, status } = useExtension();
  const navigate = useNavigate();
  const [entries, setEntries] = useState<AuditEntry[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [kindFilter, setKindFilter] = useState<KindFilter>("all");
  const [hostFilter, setHostFilter] = useState("");
  const [sinceMs, setSinceMs] = useState(0);

  const fetchLog = async () => {
    setLoading(true);
    setErr(null);
    try {
      const opts =
        sinceMs > 0 ? { since: Date.now() - sinceMs, limit: 200 } : { limit: 200 };
      const data = await client.getAuditLog(opts);
      setEntries(data);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
      setEntries(null);
    } finally {
      setLoading(false);
    }
  };

  // Initial load + refresh whenever the SW broadcasts an audit change.
  useEffect(() => {
    void fetchLog();
    return client.onChange((keys) => {
      if (keys.includes("requests:audit")) void fetchLog();
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client, sinceMs]);

  const filtered = useMemo(() => {
    if (!entries) return null;
    const host = hostFilter.trim().toLowerCase();
    return entries.filter((e) => {
      if (kindFilter !== "all" && e.verdict !== kindFilter) return false;
      if (host && !e.hostname.toLowerCase().includes(host)) return false;
      return true;
    });
  }, [entries, kindFilter, hostFilter]);

  const managedById = useMemo(() => {
    const map = new Map<string, true>();
    for (const m of managed ?? []) map.set(m.id, true);
    return map;
  }, [managed]);

  const openInEditor = (policyId: string) => {
    const target = (managed ?? []).find((p) => p.id === policyId);
    if (!target) return;
    navigate("/editor", {
      state: { id: target.id, text: target.text },
    });
  };

  if (status.kind === "error") {
    return (
      <div className="audit-page">
        <h1>Audit</h1>
        <div className="audit-err">Extension 연결 안 됨: {status.message}</div>
      </div>
    );
  }

  return (
    <div className="audit-page">
      <header className="audit-header">
        <div>
          <h1>Audit</h1>
          <div className="audit-meta">
            verdict 히스토리 · Pass / Warn / Fail
            {entries
              ? ` · ${entries.length}개${filtered && filtered.length !== entries.length ? ` (필터 ${filtered.length}개)` : ""}`
              : null}
          </div>
        </div>
        <div className="audit-toolbar">
          <select
            value={kindFilter}
            onChange={(e) => setKindFilter(e.target.value as KindFilter)}
            className="audit-select"
          >
            <option value="all">모든 verdict</option>
            <option value="pass">Pass 만</option>
            <option value="warn">Warn 만</option>
            <option value="fail">Fail 만</option>
          </select>
          <select
            value={sinceMs}
            onChange={(e) => setSinceMs(Number(e.target.value))}
            className="audit-select"
          >
            {SINCE_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
          <input
            type="search"
            placeholder="hostname 검색"
            value={hostFilter}
            onChange={(e) => setHostFilter(e.target.value)}
            className="audit-search"
          />
          <button
            type="button"
            className="audit-refresh"
            onClick={() => void fetchLog()}
            disabled={loading}
          >
            {loading ? "..." : "새로고침"}
          </button>
        </div>
      </header>

      {err ? <div className="audit-err">{err}</div> : null}

      {entries === null && !err ? (
        <div className="audit-loading">로딩 중…</div>
      ) : null}

      {entries && entries.length === 0 ? (
        <div className="audit-empty card">
          <strong>아직 기록된 verdict가 없습니다</strong>
          <span>
            확장프로그램이 dApp 트랜잭션을 가로채면 여기에 자동으로 쌓입니다.
            정책 평가 한 건마다 1줄.
          </span>
        </div>
      ) : null}

      {filtered && filtered.length === 0 && entries && entries.length > 0 ? (
        <div className="audit-empty card">
          <strong>필터 결과 없음</strong>
          <span>filter / since 조건을 완화해보세요.</span>
        </div>
      ) : null}

      {filtered && filtered.length > 0 ? (
        <ul className="audit-list">
          {filtered.map((entry) => (
            <AuditCard
              key={entry.requestId}
              entry={entry}
              managedById={managedById}
              onOpenPolicy={openInEditor}
            />
          ))}
        </ul>
      ) : null}
    </div>
  );
}

function AuditCard({
  entry,
  managedById,
  onOpenPolicy,
}: {
  entry: AuditEntry;
  managedById: Map<string, true>;
  onOpenPolicy: (id: string) => void;
}) {
  return (
    <li className={`audit-card card kind-${entry.verdict}`}>
      <div className="audit-card-head">
        <div className={`verdict-chip kind-${entry.verdict}`}>
          {entry.verdict}
        </div>
        <div className="audit-card-host">{entry.hostname}</div>
        <div className="audit-card-time">
          {new Date(entry.decidedAtMs).toLocaleString()}
        </div>
      </div>
      <div className="audit-card-meta">
        {entry.type}
        {entry.bypassed ? " · bypassed" : ""}
        {entry.policyRpc?.methods.length
          ? ` · ${entry.policyRpc.methods.join(", ")}`
          : ""}
      </div>
      {entry.matchedPolicies.length > 0 ? (
        <ul className="audit-matched">
          {entry.matchedPolicies.map((m, idx) => {
            const owned = managedById.has(m.id);
            return (
              <li
                key={`${m.id}-${idx}`}
                className={"audit-policy" + (owned ? " is-owned" : "")}
                onClick={owned ? () => onOpenPolicy(m.id) : undefined}
                role={owned ? "button" : undefined}
                tabIndex={owned ? 0 : undefined}
                title={owned ? "Editor에서 열기" : "이 정책은 대시보드 외부에서 관리"}
              >
                <span className="audit-policy-id">{m.id}</span>
                <span className={`audit-policy-sev sev-${m.severity}`}>
                  {m.severity}
                </span>
              </li>
            );
          })}
        </ul>
      ) : (
        <div className="audit-no-match">정책 매칭 없음 (orchestrator 통과)</div>
      )}
    </li>
  );
}
