import { useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import type { ManagedPolicy } from "@scopeball/sdk";
import { useExtension } from "../sdk-context";
import { RewriteBanner } from "../migration/rewrite-banner";
import {
  detectConflicts,
  describeKind,
  type ConflictHint,
} from "../policy/conflict-detector";
import "./LibraryPage.css";

type SortKey = "updated-desc" | "id-asc";

interface ImportEntry {
  id: string;
  text: string;
}

export function LibraryPage() {
  const { client, catalog, managed, status, refresh } = useExtension();
  const navigate = useNavigate();
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [sortKey, setSortKey] = useState<SortKey>("updated-desc");
  const [conflictModal, setConflictModal] = useState<{
    policy: ManagedPolicy;
    hints: ConflictHint[];
  } | null>(null);

  const conflicts = useMemo(
    () => (managed ? detectConflicts(managed) : new Map<string, ConflictHint[]>()),
    [managed],
  );

  const enabledSet = useMemo(
    () => new Set(catalog?.enabled ?? []),
    [catalog],
  );

  const filtered = useMemo(() => {
    if (!managed) return null;
    const term = search.trim().toLowerCase();
    const matchesTerm = (p: ManagedPolicy) => {
      if (!term) return true;
      if (p.id.toLowerCase().includes(term)) return true;
      // Reason is embedded in Cedar `@reason("…")` text; cheap substring
      // match is good enough for the common case (no escapes searched).
      if (p.text.toLowerCase().includes(term)) return true;
      return false;
    };
    const arr = managed.filter(matchesTerm);
    arr.sort((a, b) => {
      if (sortKey === "id-asc") return a.id.localeCompare(b.id);
      return b.updatedAtMs - a.updatedAtMs;
    });
    return arr;
  }, [managed, search, sortKey]);

  const toggleEnabled = async (id: string) => {
    setBusy(id);
    setErr(null);
    setInfo(null);
    try {
      const next = new Set(enabledSet);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      await client.setEnabledIds([...next]);
      void refresh();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const enableAll = async () => {
    if (!managed) return;
    setBusy("__bulk__");
    setErr(null);
    setInfo(null);
    try {
      const ids = new Set(enabledSet);
      for (const p of managed) ids.add(p.id);
      await client.setEnabledIds([...ids]);
      void refresh();
      setInfo(`${managed.length}개 정책 모두 활성화`);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const disableMine = async () => {
    if (!managed) return;
    setBusy("__bulk__");
    setErr(null);
    setInfo(null);
    try {
      const ids = new Set(enabledSet);
      for (const p of managed) ids.delete(p.id);
      await client.setEnabledIds([...ids]);
      void refresh();
      setInfo("내 정책 모두 비활성");
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const deletePolicy = async (id: string) => {
    if (!confirm(`정말 삭제할까요?\n\n${id}`)) return;
    setBusy(id);
    setErr(null);
    setInfo(null);
    try {
      await client.delete(id);
      void refresh();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  };

  const exportJson = () => {
    if (!managed) return;
    const payload = {
      exportedAtMs: Date.now(),
      schemaVersion: 1,
      policies: managed.map((p) => ({
        id: p.id,
        text: p.text,
      })),
    };
    const blob = new Blob([JSON.stringify(payload, null, 2)], {
      type: "application/json",
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `scopeball-policies-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
    setInfo(`${managed.length}개 정책 내보냄`);
  };

  const importJson = async (file: File) => {
    setBusy("__import__");
    setErr(null);
    setInfo(null);
    try {
      const text = await file.text();
      const parsed = JSON.parse(text) as {
        policies?: ImportEntry[];
      } | ImportEntry[];
      const entries: ImportEntry[] = Array.isArray(parsed)
        ? parsed
        : (parsed.policies ?? []);
      if (!Array.isArray(entries) || entries.length === 0) {
        throw new Error("정책이 포함되지 않은 파일입니다");
      }
      let ok = 0;
      const errors: string[] = [];
      for (const entry of entries) {
        if (typeof entry?.id !== "string" || typeof entry?.text !== "string") {
          errors.push(`malformed entry: ${JSON.stringify(entry).slice(0, 60)}`);
          continue;
        }
        try {
          await client.putRaw({ id: entry.id, text: entry.text });
          ok += 1;
        } catch (e) {
          errors.push(
            `${entry.id}: ${e instanceof Error ? e.message : String(e)}`,
          );
        }
      }
      void refresh();
      if (errors.length > 0) {
        setErr(
          `${ok}/${entries.length} 가져옴. 실패 ${errors.length}건:\n${errors.slice(0, 3).join("\n")}${errors.length > 3 ? "\n…" : ""}`,
        );
      } else {
        setInfo(`${ok}개 정책 가져옴`);
      }
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
  };

  if (status.kind === "error") {
    return (
      <div className="library-page">
        <h1>Library</h1>
        <div className="library-err">
          Extension 연결 안 됨: {status.message}
        </div>
      </div>
    );
  }

  if (status.kind === "connecting" || managed === null) {
    return (
      <div className="library-page">
        <h1>Library</h1>
        <div className="library-loading">로딩 중...</div>
      </div>
    );
  }

  return (
    <div className="library-page">
      <RewriteBanner />
      <header className="library-header">
        <div>
          <h1>Library</h1>
          <div className="library-meta">
            내가 만든 정책 {managed.length}개 · 전체 카탈로그{" "}
            {catalog?.policies.length ?? 0}개
            {filtered && filtered.length !== managed.length
              ? ` · 필터 결과 ${filtered.length}개`
              : null}
          </div>
        </div>
        <div className="library-toolbar">
          <input
            type="search"
            placeholder="id 또는 본문 검색"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="library-search"
          />
          <select
            value={sortKey}
            onChange={(e) => setSortKey(e.target.value as SortKey)}
            className="library-sort"
          >
            <option value="updated-desc">최근 수정 ↓</option>
            <option value="id-asc">ID (가나다)</option>
          </select>
        </div>
      </header>

      <div className="library-actions">
        <button
          type="button"
          className="bulk-btn"
          onClick={() => void enableAll()}
          disabled={busy === "__bulk__" || managed.length === 0}
        >
          전체 활성화
        </button>
        <button
          type="button"
          className="bulk-btn"
          onClick={() => void disableMine()}
          disabled={busy === "__bulk__" || managed.length === 0}
        >
          전체 비활성
        </button>
        <button
          type="button"
          className="bulk-btn"
          onClick={exportJson}
          disabled={managed.length === 0}
        >
          JSON 내보내기
        </button>
        <button
          type="button"
          className="bulk-btn"
          onClick={() => fileInputRef.current?.click()}
          disabled={busy === "__import__"}
        >
          JSON 가져오기
        </button>
        <input
          ref={fileInputRef}
          type="file"
          accept="application/json,.json"
          hidden
          onChange={(e) => {
            const f = e.target.files?.[0];
            if (f) void importJson(f);
          }}
        />
      </div>

      {err ? <div className="library-err">{err}</div> : null}
      {info ? <div className="library-info">{info}</div> : null}

      {managed.length === 0 ? (
        <div className="library-empty card">
          <strong>아직 등록한 정책이 없습니다</strong>
          <span>
            Editor에서 새 정책을 만들고 "정책 저장"을 누르면 여기에 표시됩니다.
            정책 id는 <code>dashboard::</code>로 시작해야 합니다.
          </span>
        </div>
      ) : filtered && filtered.length === 0 ? (
        <div className="library-empty card">
          <strong>검색 결과 없음</strong>
          <span>"{search}" 와 매칭되는 정책이 없습니다.</span>
        </div>
      ) : (
        <ul className="policy-list">
          {(filtered ?? []).map((p) => {
            const enabled = enabledSet.has(p.id);
            const rowBusy = busy === p.id;
            const hints = conflicts.get(p.id) ?? [];
            return (
              <li key={p.id} className="policy-card card">
                <div className="policy-card-head">
                  <div className="policy-id">{p.id}</div>
                  <span className={`policy-kind k-${p.kind}`}>{p.kind}</span>
                </div>
                <div className="policy-meta">
                  업데이트{" "}
                  {new Date(p.updatedAtMs).toLocaleString()}
                </div>
                {hints.length > 0 ? (
                  <button
                    type="button"
                    className="conflict-badge"
                    onClick={() =>
                      setConflictModal({ policy: p, hints })
                    }
                    title="다른 정책과의 잠재 충돌 보기"
                  >
                    충돌 가능성 {hints.length}건
                  </button>
                ) : null}
                <pre className="policy-preview">
                  <code>{p.text.slice(0, 240)}{p.text.length > 240 ? "…" : ""}</code>
                </pre>
                <div className="policy-actions">
                  <label className="toggle">
                    <input
                      type="checkbox"
                      checked={enabled}
                      disabled={rowBusy}
                      onChange={() => void toggleEnabled(p.id)}
                    />
                    <span>{enabled ? "활성화됨" : "비활성"}</span>
                  </label>
                  <div className="policy-actions-right">
                    <button
                      type="button"
                      className="btn-edit"
                      disabled={rowBusy}
                      onClick={() =>
                        navigate("/editor", {
                          state: { id: p.id, text: p.text },
                        })
                      }
                    >
                      Editor에서 열기
                    </button>
                    <button
                      type="button"
                      className="btn-delete"
                      disabled={rowBusy}
                      onClick={() => void deletePolicy(p.id)}
                    >
                      삭제
                    </button>
                  </div>
                </div>
              </li>
            );
          })}
        </ul>
      )}

      {conflictModal ? (
        <ConflictModal
          policy={conflictModal.policy}
          hints={conflictModal.hints}
          allManaged={managed}
          onClose={() => setConflictModal(null)}
          onOpenOther={(otherId) => {
            const target = managed.find((m) => m.id === otherId);
            if (!target) return;
            setConflictModal(null);
            navigate("/editor", {
              state: { id: target.id, text: target.text },
            });
          }}
        />
      ) : null}
    </div>
  );
}

function ConflictModal({
  policy,
  hints,
  allManaged,
  onClose,
  onOpenOther,
}: {
  policy: ManagedPolicy;
  hints: ConflictHint[];
  allManaged: ManagedPolicy[];
  onClose: () => void;
  onOpenOther: (id: string) => void;
}) {
  const byId = new Map(allManaged.map((m) => [m.id, m]));
  return (
    <div
      className="conflict-backdrop"
      role="dialog"
      aria-modal="true"
      aria-labelledby="conflict-title"
      onClick={onClose}
    >
      <div className="conflict-modal" onClick={(e) => e.stopPropagation()}>
        <header className="conflict-modal-head">
          <h2 id="conflict-title">잠재 충돌</h2>
          <div className="conflict-modal-sub">
            <code>{policy.id}</code> 의 조건/severity가 아래 정책과 겹칩니다.
            정적 분석이므로 false positive 가능 — 실제 verdict는 Policy Test에서 확인하세요.
          </div>
        </header>
        <ul className="conflict-list">
          {hints.map((h, idx) => {
            const other = byId.get(h.otherId);
            return (
              <li key={`${h.otherId}-${idx}`} className="conflict-row">
                <div className="conflict-row-head">
                  <span className={`conflict-kind k-${h.kind}`}>
                    {describeKind(h.kind)}
                  </span>
                  <button
                    type="button"
                    className="conflict-other-id"
                    onClick={() => onOpenOther(h.otherId)}
                    disabled={!other}
                  >
                    {h.otherId}
                  </button>
                </div>
                {h.sharedConjuncts.length > 0 ? (
                  <div className="conflict-shared">
                    <span className="conflict-shared-label">공통 조건:</span>
                    <ul>
                      {h.sharedConjuncts.map((c, i) => (
                        <li key={i}>
                          <code>{c}</code>
                        </li>
                      ))}
                    </ul>
                  </div>
                ) : null}
              </li>
            );
          })}
        </ul>
        <footer className="conflict-modal-foot">
          <button
            type="button"
            className="conflict-close"
            onClick={onClose}
          >
            닫기
          </button>
        </footer>
      </div>
    </div>
  );
}
