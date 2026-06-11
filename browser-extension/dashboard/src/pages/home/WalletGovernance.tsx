/**
 * Wallet-dial governance — the Home centerpiece.
 *
 *   ┌ WalletDial (left) ┐   ┌ WalletPanel (right) ──────────────┐
 *   │  vertical card     │   │ folder(package) ▸ policy ▸ param  │
 *   │  dial, drag/step    │   │ package on/off  → setPackageEnabled
 *   └────────────────────┘   │ param   on/off  → updateBinding   │
 *   click active card → WalletOverview (all-wallets grid)        │
 *
 * Ported from the Home.html prototype. The dial physics are imperative
 * (refs + rAF) like the prototype; everything else is React state.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import {
  setPackageEnabled,
  updateBinding,
  type StoreSnapshot,
} from "../../server-api/policy-store";
import {
  appliedCount,
  buildFolders,
  BASELINE_COUNT,
  totalPolicyCount,
  toggledParams,
  type FolderVM,
  type PolicyVM,
} from "./home-model";

import "./home-dial.css";

export interface DialWallet {
  address: string;
  label: string | null;
  balanceUsd: number;
  tone: "calm" | "warn" | "fail";
}

interface Props {
  wallets: DialWallet[];
  snap: StoreSnapshot | null;
  onSync: (address: string) => void;
  syncingAddress?: string | null;
  onRename: (w: DialWallet) => void;
  onDelete: (w: DialWallet) => void;
  onAddWallet: () => void;
}

// ── tiny icons ────────────────────────────────────────────────────────────
const Chevron = () => (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round"><path d="m6 9 6 6 6-6" /></svg>
);
const Folder = () => (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></svg>
);
const Shield = () => (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.9} strokeLinecap="round" strokeLinejoin="round"><path d="M12 3 4 6v5c0 5 3.5 7.5 8 9 4.5-1.5 8-4 8-9V6Z" /></svg>
);
const Edit = () => (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round"><path d="M12 20h9" /><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z" /></svg>
);
const Sync = () => (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round"><path d="M3 12a9 9 0 0 1 15-6.7L21 8" /><path d="M21 3v5h-5" /><path d="M21 12a9 9 0 0 1-15 6.7L3 16" /><path d="M3 21v-5h5" /></svg>
);
const Rename = () => (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round"><path d="M20.59 13.41 13.42 20.58a2 2 0 0 1-2.83 0L3 13V3h10l7.59 7.58a2 2 0 0 1 0 2.83z" /><path d="M7.5 7.5h.01" /></svg>
);
const Trash = () => (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round"><path d="M3 6h18" /><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" /><path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" /><path d="M10 11v6M14 11v6" /></svg>
);
const ArrowOut = () => (
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2.2} strokeLinecap="round" strokeLinejoin="round"><path d="M5 12h14M13 6l6 6-6 6" /></svg>
);

const initialOf = (w: DialWallet) => (w.label ?? w.address).slice(0, 1).toUpperCase();
const usd = (n: number) => "$" + n.toLocaleString("en-US", { maximumFractionDigits: 0 });

// ════════════════════════════════════════════════════════════════════════
export function WalletGovernance({ wallets, snap, onSync, syncingAddress, onRename, onDelete, onAddWallet }: Props) {
  const [active, setActive] = useState(0);
  const [overview, setOverview] = useState(false);
  const splitRef = useRef<HTMLDivElement>(null);

  // clamp active when wallets change
  useEffect(() => {
    if (active > wallets.length - 1) setActive(Math.max(0, wallets.length - 1));
  }, [wallets.length, active]);

  const activeWallet = wallets[active];

  if (wallets.length === 0) {
    return (
      <section className="dial-section">
        <DialHead />
        <div className="dp-empty" style={{ minHeight: 240, border: "1px solid var(--hairline)", borderRadius: "var(--r-md)", background: "var(--surface)" }}>
          <div className="et"><b>등록된 지갑이 없습니다</b><br />첫 지갑을 추가하세요.</div>
          <button className="btn primary" onClick={onAddWallet}>지갑 추가 +</button>
        </div>
      </section>
    );
  }

  return (
    <section className="dial-section">
      <DialHead />
      <div className="dial-split" ref={splitRef}>
        <WalletDial
          wallets={wallets}
          active={active}
          onSelect={(i) => { setOverview(false); setActive(i); }}
          onActiveClick={() => setOverview((v) => !v)}
        />
        <div className="dial-panel">
          {overview ? (
            <WalletOverview
              wallets={wallets}
              snap={snap}
              active={active}
              onPick={(i) => { setActive(i); setOverview(false); }}
              onAddWallet={onAddWallet}
            />
          ) : (
            <WalletPanel
              key={activeWallet.address}
              wallet={activeWallet}
              snap={snap}
              onOpenOverview={() => setOverview(true)}
              onSync={onSync}
              syncing={syncingAddress === activeWallet.address}
              onRename={() => onRename(activeWallet)}
              onDelete={() => onDelete(activeWallet)}
            />
          )}
        </div>
      </div>
      <ResizeGrip splitRef={splitRef} />
    </section>
  );
}

function DialHead() {
  return (
    <div className="dial-head">
      <div className="gov-title">
        <h2>지갑별 정책 <span className="scope-pill wallet">지갑 단위</span></h2>
        <span className="gov-sub">지갑을 고르면 그 지갑의 패키지가 열립니다. 카드를 한 번 더 누르면 전체 지갑을 한눈에 봅니다.</span>
      </div>
    </div>
  );
}

// ── LEFT: the dial ─────────────────────────────────────────────────────────
function WalletDial({
  wallets,
  active,
  onSelect,
  onActiveClick,
}: {
  wallets: DialWallet[];
  active: number;
  onSelect: (i: number) => void;
  onActiveClick: () => void;
}) {
  const stageRef = useRef<HTMLDivElement>(null);
  const cardRefs = useRef<HTMLDivElement[]>([]);
  const offset = useRef(active);
  const spacing = useRef(152);
  const drag = useRef<{ y: number; o: number; moved: boolean; vel: number; lastY: number; lastT: number } | null>(null);
  const n = wallets.length;

  const recalc = () => {
    const ch = cardRefs.current[0]?.offsetHeight ?? 176;
    const sh = stageRef.current?.clientHeight ?? 472;
    spacing.current = Math.max(140, Math.min(ch + 34, sh * 0.32));
  };
  const layout = () => {
    const sp = spacing.current;
    cardRefs.current.forEach((tk, i) => {
      if (!tk) return;
      let rel = i - offset.current;
      rel = rel - n * Math.round(rel / n);
      const dist = Math.abs(rel);
      const scale = Math.max(0.66, 1 - dist * 0.18);
      const op = dist > 1.7 ? 0 : Math.max(0, 1 - dist * 0.45);
      tk.style.transform = `translateY(${rel * sp}px) scale(${scale})`;
      tk.style.opacity = String(op);
      tk.style.zIndex = String(100 - Math.round(dist * 10));
      tk.style.pointerEvents = op < 0.25 ? "none" : "auto";
      tk.classList.toggle("is-active", dist < 0.5);
    });
  };
  const activeIndex = () => ((Math.round(offset.current) % n) + n) % n;

  // keep the dial centered on `active` when it changes externally
  useEffect(() => {
    const k = Math.round((offset.current - active) / n);
    offset.current = active + k * n;
    recalc();
    layout();
    const s = stageRef.current;
    if (s) { s.classList.remove("wd-ready"); requestAnimationFrame(() => s.classList.add("wd-ready")); }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [active, n]);

  useEffect(() => {
    recalc(); layout();
    const t = setTimeout(() => stageRef.current?.classList.add("wd-ready"), 60);
    const onResize = () => { recalc(); layout(); };
    window.addEventListener("resize", onResize);
    return () => { clearTimeout(t); window.removeEventListener("resize", onResize); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const commit = () => onSelect(activeIndex());
  const step = (d: number) => { offset.current = Math.round(offset.current) + d; layout(); commit(); };

  const onPointerDown = (e: React.PointerEvent) => {
    drag.current = { y: e.clientY, o: offset.current, moved: false, vel: 0, lastY: e.clientY, lastT: e.timeStamp };
    stageRef.current?.classList.add("dragging");
    stageRef.current?.setPointerCapture(e.pointerId);
  };
  const onPointerMove = (e: React.PointerEvent) => {
    const d = drag.current; if (!d) return;
    const dy = e.clientY - d.y;
    if (Math.abs(dy) > 4) d.moved = true;
    const dt = e.timeStamp - d.lastT;
    if (dt > 0) d.vel = 0.7 * d.vel + 0.3 * ((e.clientY - d.lastY) / dt);
    d.lastY = e.clientY; d.lastT = e.timeStamp;
    offset.current = d.o - dy / spacing.current;
    layout();
  };
  const onPointerUp = (e: React.PointerEvent) => {
    const d = drag.current; if (!d) return;
    drag.current = null;
    stageRef.current?.classList.remove("dragging");
    try { stageRef.current?.releasePointerCapture(e.pointerId); } catch { /* noop */ }
    const base = Math.round(offset.current);
    let target = Math.round(offset.current - (d.vel * 130) / spacing.current);
    target = Math.max(base - 2, Math.min(base + 2, target));
    offset.current = target;
    layout(); commit();
  };
  const onClick = (e: React.MouseEvent) => {
    const d = drag.current;
    if (d?.moved) return;
    const card = (e.target as HTMLElement).closest(".wcard");
    if (card && card.classList.contains("is-active")) { onActiveClick(); return; }
    const r = stageRef.current!.getBoundingClientRect();
    if (e.clientY < r.top + r.height / 2) step(-1); else step(1);
  };

  return (
    <div className="wd-col">
      <div
        className="wd-stage"
        ref={stageRef}
        tabIndex={0}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        onClick={onClick}
        onKeyDown={(e) => { if (e.key === "ArrowUp") { e.preventDefault(); step(-1); } else if (e.key === "ArrowDown") { e.preventDefault(); step(1); } }}
      >
        {wallets.map((w, i) => (
          <div
            key={w.address}
            className="wcard"
            data-tone={w.tone}
            ref={(el) => { if (el) cardRefs.current[i] = el; }}
          >
            <div className="wc-top">
              <span className="wc-mono">{initialOf(w)}</span>
              <span className="wc-sev"><span className="d" />지갑</span>
            </div>
            <div className="wc-mid">
              <div className="wc-name">{w.label ?? "—"}</div>
              <div className="wc-addr">{w.address.slice(0, 6)} ·· {w.address.slice(-4)}</div>
            </div>
            <div className="wc-bot">
              <div><div className="lbl">잔액</div><div className="wc-bal">{usd(w.balanceUsd)}</div></div>
            </div>
          </div>
        ))}
      </div>
      <div className="wd-dots">
        {wallets.map((w, i) => (
          <button key={w.address} type="button" className={i === active ? "on" : ""} aria-label={w.label ?? w.address} onClick={() => onSelect(i)} />
        ))}
      </div>
    </div>
  );
}

// ── RIGHT: the package/policy/param panel ──────────────────────────────────
function WalletPanel({
  wallet,
  snap,
  onOpenOverview,
  onSync,
  syncing,
  onRename,
  onDelete,
}: {
  wallet: DialWallet;
  snap: StoreSnapshot | null;
  onOpenOverview: () => void;
  onSync: (address: string) => void;
  syncing: boolean;
  onRename: () => void;
  onDelete: () => void;
}) {
  const qc = useQueryClient();
  const folders = useMemo(() => (snap ? buildFolders(snap, wallet.address) : []), [snap, wallet.address]);
  const applied = snap ? appliedCount(snap, wallet.address) : BASELINE_COUNT;
  const polTotal = snap ? totalPolicyCount(snap, wallet.address) : 0;

  const [openFolders, setOpenFolders] = useState<Set<string>>(new Set());
  const [openPolicies, setOpenPolicies] = useState<Set<string>>(new Set());

  const invalidate = () => qc.invalidateQueries({ queryKey: ["ps2-overview"] });
  const pkgMut = useMutation({ mutationFn: setPackageEnabled, onSettled: invalidate });
  const paramMut = useMutation({ mutationFn: updateBinding, onSettled: invalidate });

  const toggleFolder = (id: string) => setOpenFolders((s) => { const n = new Set(s); n.has(id) ? n.delete(id) : n.add(id); return n; });
  const togglePolicy = (id: string) => setOpenPolicies((s) => { const n = new Set(s); n.has(id) ? n.delete(id) : n.add(id); return n; });

  return (
    <div className="dp-fade">
      <div className="dp-head">
        <button className="dp-coin" type="button" data-tone={wallet.tone} title="전체 지갑 보기" onClick={onOpenOverview}>
          {initialOf(wallet)}
        </button>
        <div className="dp-id">
          <div className="nr">
            <span className="name">{wallet.label ?? "—"}</span>
            <span className="addr">{wallet.address}</span>
          </div>
        </div>
        <div className="dp-bal">
          <div className="dp-acts">
            <button className={`dp-ib${syncing ? " spinning" : ""}`} type="button" title="지금 동기화" onClick={() => onSync(wallet.address)}><Sync /></button>
            <button className="dp-ib" type="button" title="이름 변경" onClick={onRename}><Rename /></button>
            <button className="dp-ib danger" type="button" title="지갑 삭제" onClick={onDelete}><Trash /></button>
          </div>
          <b>{usd(wallet.balanceUsd)}</b>
          <div><Link className="open" to={`/monitoring?wallet=${wallet.address}`}>자산 열기 <ArrowOut /></Link></div>
        </div>
      </div>

      <div className="dp-stack">
        <span className="base-chip"><Shield />Baseline {BASELINE_COUNT}</span>
        <span className="seg"><span className="arrow">+</span> 패키지 <b>{folders.length}</b><span className="mute"> ({polTotal}개 정책)</span></span>
        <span className="arrow">→</span>
        <span className="seg">이 지갑에 적용 <b>{applied}</b></span>
      </div>

      {folders.length === 0 ? (
        <div className="dp-empty">
          <div className="et"><b>설치된 패키지 없음</b><br />baseline만 적용됩니다.</div>
          <Link className="btn" to="/market">패키지 받기</Link>
        </div>
      ) : (
        <div className="dp-policies dp-folders">
          {folders.map((f) => (
            <FolderRow
              key={f.packageId}
              folder={f}
              open={openFolders.has(f.packageId)}
              openPolicies={openPolicies}
              onToggleFolder={() => toggleFolder(f.packageId)}
              onTogglePolicy={togglePolicy}
              onTogglePackage={(on) => pkgMut.mutate({ address: wallet.address, packageId: f.packageId, enabled: on })}
              onToggleParam={(p, holeName, on) =>
                paramMut.mutate({
                  address: wallet.address,
                  bindingId: p.bindingId,
                  patch: { params: toggledParams(snap?.wallets.byAddress[wallet.address.toLowerCase()]?.bindings[p.bindingId]?.params, holeName, on) },
                })
              }
            />
          ))}
        </div>
      )}
    </div>
  );
}

function FolderRow({
  folder,
  open,
  openPolicies,
  onToggleFolder,
  onTogglePolicy,
  onTogglePackage,
  onToggleParam,
}: {
  folder: FolderVM;
  open: boolean;
  openPolicies: Set<string>;
  onToggleFolder: () => void;
  onTogglePolicy: (id: string) => void;
  onTogglePackage: (on: boolean) => void;
  onToggleParam: (p: PolicyVM, holeName: string, on: boolean) => void;
}) {
  return (
    <div className={`pk-folder${open ? "" : " collapsed"}${folder.on ? "" : " off"}`}>
      <div className="pk-folder-head" onClick={onToggleFolder}>
        <span className="pk-chev"><Chevron /></span>
        <span className="pk-folder-ic"><Folder /></span>
        <span className="pk-folder-name">{folder.name}</span>
        <span className="pk-folder-count"><b>{folder.policies.length}</b> 정책</span>
        <Switch checked={folder.on} onChange={onTogglePackage} className="pk-sw" />
      </div>
      <div className="pk-folder-body" style={{ height: open ? "auto" : 0 }}>
        <div className="pk-folder-inner">
          {folder.policies.map((p) => (
            <PolicyRow
              key={p.bindingId}
              policy={p}
              open={openPolicies.has(p.bindingId)}
              onToggle={() => onTogglePolicy(p.bindingId)}
              onToggleParam={(holeName, on) => onToggleParam(p, holeName, on)}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

function PolicyRow({
  policy,
  open,
  onToggle,
  onToggleParam,
}: {
  policy: PolicyVM;
  open: boolean;
  onToggle: () => void;
  onToggleParam: (holeName: string, on: boolean) => void;
}) {
  const hasParams = policy.params.length > 0;
  const onN = policy.params.filter((p) => p.on).length;
  return (
    <div className={`pol2${open ? " expanded" : ""}`}>
      <div className="pol2-head" onClick={() => hasParams && onToggle()}>
        <span className={`pol2-chev${hasParams ? "" : " empty"}`}>{hasParams && <Chevron />}</span>
        <span className={`pr-dot ${policy.severity}`} />
        <span className="pol2-name">{policy.name}</span>
        <Link className="pol2-edit" title="Editor에서 열기" to={`/editor/${encodeURIComponent(policy.defId)}`} onClick={(e) => e.stopPropagation()}><Edit /></Link>
      </div>
      {hasParams && (
        <div className="pol2-body" style={{ height: open ? "auto" : 0 }}>
          <div className="pol2-inner">
            <div className="param-head">파라미터 <b>{onN}</b>/{policy.params.length}</div>
            {policy.params.map((pr) => (
              <div className="param-row" key={pr.holeName}>
                <span className="param-name">{pr.label}</span>
                {pr.isBool ? (
                  <Switch checked={pr.on} onChange={(on) => onToggleParam(pr.holeName, on)} className="param-sw" small />
                ) : (
                  <span className="pk-folder-count" title={pr.type}>{pr.display}</span>
                )}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// ── overview grid ──────────────────────────────────────────────────────────
function WalletOverview({
  wallets,
  snap,
  active,
  onPick,
  onAddWallet,
}: {
  wallets: DialWallet[];
  snap: StoreSnapshot | null;
  active: number;
  onPick: (i: number) => void;
  onAddWallet: () => void;
}) {
  return (
    <div className="dp-fade wv-wrap">
      <div className="wv-head">
        <div className="wv-title">전체 지갑 <b>{wallets.length}</b></div>
        <button className="btn wv-add" type="button" onClick={onAddWallet}>지갑 추가 +</button>
      </div>
      <div className="wv-grid">
        {wallets.map((w, i) => {
          const pkgs = snap ? buildFolders(snap, w.address).length : 0;
          const ap = snap ? appliedCount(snap, w.address) : BASELINE_COUNT;
          return (
            <button key={w.address} type="button" className={`wv-card${i === active ? " is-active" : ""}`} onClick={() => onPick(i)}>
              <div className="wv-top"><span className="wv-coin" data-tone={w.tone}>{initialOf(w)}</span></div>
              <div className="wv-name">{w.label ?? "—"}</div>
              <div className="wv-addr">{w.address}</div>
              <div className="wv-foot">
                <span className="wv-bal">{usd(w.balanceUsd)}</span>
                <span className="wv-meta">패키지 {pkgs} · 적용 {ap}</span>
              </div>
            </button>
          );
        })}
      </div>
      <div className="wv-hint">지갑을 선택하면 해당 지갑의 패키지로 돌아갑니다.</div>
    </div>
  );
}

// ── reusable toggle (matches the .sw markup used across the dashboard) ──────
function Switch({ checked, onChange, className, small }: { checked: boolean; onChange: (on: boolean) => void; className?: string; small?: boolean }) {
  return (
    <label className={`sw${small ? " sw-sm" : ""}${className ? " " + className : ""}`} onClick={(e) => e.stopPropagation()}>
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span className="track" />
      <span className="thumb" />
    </label>
  );
}

// ── drag-resize the split height (no persistence) ──────────────────────────
function ResizeGrip({ splitRef }: { splitRef: React.RefObject<HTMLDivElement> }) {
  const rs = useRef<{ y: number; h: number } | null>(null);
  const gripRef = useRef<HTMLDivElement>(null);
  const MIN = 360;
  const maxH = () => Math.max(MIN, window.innerHeight - 150);
  return (
    <div
      className="split-resize"
      ref={gripRef}
      title="드래그해서 늘리거나 줄이기 · 더블클릭하면 기본 크기"
      onPointerDown={(e) => {
        const el = splitRef.current; if (!el) return;
        rs.current = { y: e.clientY, h: el.offsetHeight };
        try { gripRef.current?.setPointerCapture(e.pointerId); } catch { /* noop */ }
        gripRef.current?.classList.add("dragging");
      }}
      onPointerMove={(e) => {
        const d = rs.current; const el = splitRef.current; if (!d || !el) return;
        el.style.height = Math.min(maxH(), Math.max(MIN, d.h + (e.clientY - d.y))) + "px";
        window.dispatchEvent(new Event("resize"));
      }}
      onPointerUp={(e) => {
        if (!rs.current) return;
        rs.current = null;
        try { gripRef.current?.releasePointerCapture(e.pointerId); } catch { /* noop */ }
        gripRef.current?.classList.remove("dragging");
      }}
      onDoubleClick={() => { if (splitRef.current) { splitRef.current.style.height = ""; window.dispatchEvent(new Event("resize")); } }}
    >
      <span className="grip" />
    </div>
  );
}
