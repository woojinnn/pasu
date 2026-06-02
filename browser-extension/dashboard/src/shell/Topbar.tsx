/**
 * Page topbar — title crumb, global search, verdict count chips.
 *
 * Search is client-side: it fans queries out to server wallets plus
 * extension-local policy/verdict readers, and renders matched entries.
 * Click a result → router push to the relevant page. The intent is "I
 * just want to jump", not "I want a perfect search engine".
 */
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

import {
  listAuditVerdicts,
  listPolicies,
  listWallets,
  type InstalledPolicy,
  type VerdictDto,
  type WalletId,
} from "../server-api";

interface TopbarProps {
  here: string;
  subtitle?: ReactNode;
  searchPlaceholder?: string;
  counts?: { pass: number; warn: number; fail: number };
  right?: ReactNode;
}

export function Topbar({ here, subtitle, searchPlaceholder, counts, right }: TopbarProps) {
  return (
    <div className="topbar">
      <div className="crumb">
        <span className="here">{here}</span>
        {subtitle && <span className="sep">/</span>}
        {subtitle && <span className="addr">{subtitle}</span>}
      </div>
      <GlobalSearch placeholder={searchPlaceholder} />
      <div className="dots">
        {counts && (
          <>
            <span className="dot-chip">
              <span className="dot pass" />PASS {counts.pass}
            </span>
            <span className="dot-chip">
              <span className="dot warn" />WARN {counts.warn}
            </span>
            <span className="dot-chip">
              <span className="dot fail" />FAIL {counts.fail}
            </span>
          </>
        )}
        <NotificationButton />
        {right}
      </div>
    </div>
  );
}

// ── Global search ───────────────────────────────────────────────────────

function GlobalSearch({ placeholder }: { placeholder?: string }) {
  const [q, setQ] = useState("");
  const [open, setOpen] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const navigate = useNavigate();

  // ⌘K / Ctrl+K to focus search.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        inputRef.current?.focus();
        setOpen(true);
      } else if (e.key === "Escape") {
        setOpen(false);
        inputRef.current?.blur();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  // Click outside closes the popdown.
  useEffect(() => {
    const onClick = (e: MouseEvent) => {
      if (!containerRef.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onClick);
    return () => document.removeEventListener("mousedown", onClick);
  }, []);

  // Underlying queries — only kick when the popdown is open. Cached
  // for 60s so opening repeatedly isn't expensive.
  const walletsQ = useQuery({
    queryKey: ["search-wallets"],
    queryFn: listWallets,
    enabled: open,
    staleTime: 60_000,
  });
  const policiesQ = useQuery({
    queryKey: ["search-policies"],
    queryFn: listPolicies,
    enabled: open,
    staleTime: 60_000,
  });
  const verdictsQ = useQuery({
    queryKey: ["search-verdicts"],
    queryFn: () => listAuditVerdicts({ range: "24h", limit: 100 }),
    enabled: open && q.trim().length >= 3,
    staleTime: 60_000,
  });

  const { walletHits, policyHits, verdictHits } = useMemo(() => {
    const needle = q.trim().toLowerCase();
    if (!needle) return { walletHits: [], policyHits: [], verdictHits: [] };
    const walletHits = (walletsQ.data ?? []).filter((w: WalletId) =>
      w.address.toLowerCase().includes(needle),
    );
    const policyHits = (policiesQ.data ?? []).filter(
      (p: InstalledPolicy) =>
        p.name.toLowerCase().includes(needle) ||
        (p.description ?? "").toLowerCase().includes(needle),
    );
    const verdictHits = (verdictsQ.data ?? []).filter(
      (v: VerdictDto) =>
        (v.policy?.name ?? "").toLowerCase().includes(needle) ||
        (v.dapp_origin ?? "").toLowerCase().includes(needle) ||
        (v.decoded_fn ?? "").toLowerCase().includes(needle) ||
        (v.reason?.ko ?? "").toLowerCase().includes(needle) ||
        (v.reason?.en ?? "").toLowerCase().includes(needle),
    );
    return { walletHits, policyHits, verdictHits };
  }, [q, walletsQ.data, policiesQ.data, verdictsQ.data]);

  const total = walletHits.length + policyHits.length + verdictHits.length;
  const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

  const onPick = (path: string) => {
    setOpen(false);
    setQ("");
    navigate(path);
  };

  return (
    <div className="search-wrap" ref={containerRef}>
      <div className="search">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" style={{ color: "var(--slate-300)" }}>
          <circle cx="11" cy="11" r="7" />
          <path d="m20 20-3.5-3.5" />
        </svg>
        <input
          ref={inputRef}
          type="text"
          placeholder={placeholder ?? "지갑 / 정책 / dApp / 함수 검색"}
          value={q}
          onChange={(e) => {
            setQ(e.target.value);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
        />
        <span className="kbd">{isMac ? "⌘ K" : "Ctrl K"}</span>
      </div>

      {open && q.trim().length > 0 && (
        <div className="search-pop">
          {total === 0 && (
            <div className="search-empty">매칭되는 항목이 없습니다</div>
          )}
          {walletHits.length > 0 && (
            <div className="search-group">
              <div className="search-group-head">지갑 · {walletHits.length}</div>
              {walletHits.slice(0, 5).map((w) => (
                <button
                  key={w.address}
                  className="search-item"
                  onClick={() => onPick(`/monitoring?wallet=${w.address}`)}
                  title={w.address}
                >
                  <span className="search-item-ico">W</span>
                  <span className="search-item-main">
                    <span className="search-item-title">{shortAddr(w.address)}</span>
                    <span className="search-item-sub">{w.chains.length} chains</span>
                  </span>
                  <span className="search-item-go">↗</span>
                </button>
              ))}
            </div>
          )}
          {policyHits.length > 0 && (
            <div className="search-group">
              <div className="search-group-head">정책 · {policyHits.length}</div>
              {policyHits.slice(0, 5).map((p) => (
                <button
                  key={p.id}
                  className="search-item"
                  onClick={() => onPick(`/editor?policy=${p.id}`)}
                >
                  <span className="search-item-ico">P</span>
                  <span className="search-item-main">
                    <span className="search-item-title">{p.name}</span>
                    <span className="search-item-sub">policy#{p.id} · {p.severity}</span>
                  </span>
                  <span className="search-item-go">↗</span>
                </button>
              ))}
            </div>
          )}
          {verdictHits.length > 0 && (
            <div className="search-group">
              <div className="search-group-head">최근 verdict · {verdictHits.length}</div>
              {verdictHits.slice(0, 5).map((v) => (
                <button key={v.id} className="search-item" onClick={() => onPick(`/history`)}>
                  <span className={`search-item-ico verdict ${v.verdict}`}>{v.verdict[0].toUpperCase()}</span>
                  <span className="search-item-main">
                    <span className="search-item-title">{v.policy?.name ?? v.decoded_fn ?? "(unnamed)"}</span>
                    <span className="search-item-sub">{v.dapp_origin ?? "—"} · #{v.id}</span>
                  </span>
                  <span className="search-item-go">↗</span>
                </button>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ── Notification button (C11) ───────────────────────────────────────────

function NotificationButton() {
  const navigate = useNavigate();
  const findingsQ = useQuery({
    queryKey: ["findings", "topbar"],
    queryFn: () => import("../server-api").then((m) => m.listFindings({ limit: 50 })),
    refetchInterval: 30_000,
  });
  const unread = findingsQ.data?.filter((f) => f.user_decision === null).length ?? 0;
  return (
    <button
      className="icon-btn"
      onClick={() => navigate("/history")}
      title={unread > 0 ? `${unread}건 미해결 finding` : "알림 없음"}
      aria-label="notifications"
    >
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round">
        <path d="M6 8a6 6 0 0 1 12 0c0 7 3 7 3 9H3c0-2 3-2 3-9" />
        <path d="M10.3 21a1.94 1.94 0 0 0 3.4 0" />
      </svg>
      {unread > 0 && <span className="unread-dot" />}
    </button>
  );
}

// ── helpers ─────────────────────────────────────────────────────────────

function shortAddr(addr: string): string {
  if (!addr || addr.length < 12) return addr;
  return `${addr.slice(0, 6)}···${addr.slice(-4)}`;
}
