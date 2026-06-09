import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  deleteListing,
  deleteManagedPolicy,
  deletePolicySet,
  deleteWallet,
  listListings,
  listManagedPolicies,
  listPolicySets,
  listWallets,
  pickI18n,
} from "../server-api";
import { useAuth } from "../hooks/useAuth";
import { Topbar } from "../shell/Topbar";

import "./profile.css";

/**
 * `/profile` — account page. Three concerns:
 *  1. Identity + sign out.
 *  2. Posts this account published to the market (view / open).
 *  3. Reset switches: wipe this account's wallets / policies.
 */
export function ProfilePage() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const { user, logout } = useAuth();

  const myListingsQ = useQuery({
    queryKey: ["my-listings", user?.user_id],
    queryFn: () => listListings({ publisher_id: user!.user_id, limit: 100 }),
    enabled: !!user?.user_id,
  });
  const walletsQ = useQuery({ queryKey: ["wallets"], queryFn: listWallets });
  const policiesQ = useQuery({
    queryKey: ["managed-policies"],
    queryFn: listManagedPolicies,
  });
  const setsQ = useQuery({ queryKey: ["policy-sets"], queryFn: listPolicySets });

  const walletCount = walletsQ.data?.length ?? 0;
  const policyCount = policiesQ.data?.length ?? 0;
  const setCount = setsQ.data?.length ?? 0;

  const [banner, setBanner] = useState<string | null>(null);

  const resetWalletsMut = useMutation({
    mutationFn: async () => {
      const wallets = walletsQ.data ?? [];
      for (const w of wallets) await deleteWallet(w.address);
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["wallets"] });
      setBanner("지갑을 모두 초기화했어요.");
    },
    onError: (e) => setBanner(`지갑 초기화 실패: ${String(e)}`),
  });

  const resetPoliciesMut = useMutation({
    mutationFn: async () => {
      const policies = policiesQ.data ?? [];
      const sets = setsQ.data ?? [];
      for (const s of sets) await deletePolicySet(s.id);
      for (const p of policies) await deleteManagedPolicy(p.id);
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["managed-policies"] });
      void qc.invalidateQueries({ queryKey: ["policy-sets"] });
      void qc.invalidateQueries({ queryKey: ["enabled-policy-ids"] });
      setBanner("정책·패키지를 모두 초기화했어요.");
    },
    onError: (e) => setBanner(`정책 초기화 실패: ${String(e)}`),
  });

  const deleteListingMut = useMutation({
    mutationFn: (listingId: string) => deleteListing(listingId),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["my-listings"] });
      void qc.invalidateQueries({ queryKey: ["market-listings"] });
      setBanner("게시물을 삭제했어요.");
    },
    onError: (e) => setBanner(`게시물 삭제 실패: ${String(e)}`),
  });

  const onLogout = () => {
    logout();
    navigate("/login", { replace: true });
  };

  const email = user?.email ?? "—";
  const initials = email.slice(0, 2).toUpperCase();

  return (
    <>
      <Topbar here="프로필" subtitle="내 계정" showSearch={false} />
      <div className="pp-body">
        {banner && <div className="pp-banner">{banner}</div>}

        {/* identity */}
        <section className="pp-card pp-identity">
          <span className="pp-av">{initials}</span>
          <div className="pp-id-meta">
            <div className="pp-email">{email}</div>
          </div>
          <button type="button" className="pp-btn ghost danger" onClick={onLogout}>
            로그아웃
          </button>
        </section>

        {/* published posts */}
        <section className="pp-card">
          <div className="pp-sec-head">
            <h2>올린 게시물</h2>
            <span className="pp-count">{myListingsQ.data?.length ?? 0}</span>
          </div>
          {myListingsQ.isLoading && <div className="pp-muted">불러오는 중…</div>}
          {myListingsQ.data && myListingsQ.data.length === 0 && (
            <div className="pp-empty">
              아직 마켓에 올린 게시물이 없어요. 에디터에서 정책을 만들고 “마켓에
              올리기”로 공개해보세요.
            </div>
          )}
          {myListingsQ.data && myListingsQ.data.length > 0 && (
            <ul className="pp-listings">
              {myListingsQ.data.map((l) => (
                <li key={l.id} className="pp-listing-row">
                  <Link to={`/market/${l.slug}`} className="pp-listing">
                    <div className="pp-listing-main">
                      <span className="pp-listing-name">
                        {pickI18n(l.display_name)}
                      </span>
                      <span className="pp-listing-slug">{l.slug}</span>
                    </div>
                    <div className="pp-listing-stats">
                      <span title="설치 수">↓ {l.install_count}</span>
                      {l.current_version && (
                        <span className="pp-ver">{l.current_version}</span>
                      )}
                      <span className={`pp-status ${l.status}`}>{l.status}</span>
                    </div>
                  </Link>
                  <button
                    type="button"
                    className="pp-listing-del"
                    title="이 게시물 삭제"
                    disabled={deleteListingMut.isPending}
                    onClick={() => {
                      if (
                        window.confirm(
                          `게시물 "${pickI18n(l.display_name)}"을(를) 마켓에서 삭제할까요?\n되돌릴 수 없어요.`,
                        )
                      )
                        deleteListingMut.mutate(l.id);
                    }}
                  >
                    삭제
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>

        {/* reset switches */}
        <section className="pp-card">
          <div className="pp-sec-head">
            <h2>초기화</h2>
          </div>
          <p className="pp-muted">
            이 계정에 저장된 데이터를 지웁니다. 되돌릴 수 없어요.
          </p>

          <div className="pp-reset-row">
            <div className="pp-reset-info">
              <div className="pp-reset-title">지갑 초기화</div>
              <div className="pp-reset-sub">
                추적 중인 지갑 {walletCount}개를 모두 제거합니다.
              </div>
            </div>
            <button
              type="button"
              className="pp-btn danger"
              disabled={walletCount === 0 || resetWalletsMut.isPending}
              onClick={() => {
                if (
                  window.confirm(
                    `추적 중인 지갑 ${walletCount}개를 모두 제거할까요?\n되돌릴 수 없어요.`,
                  )
                )
                  resetWalletsMut.mutate();
              }}
            >
              {resetWalletsMut.isPending ? "초기화 중…" : "지갑 비우기"}
            </button>
          </div>

          <div className="pp-reset-row">
            <div className="pp-reset-info">
              <div className="pp-reset-title">정책 초기화</div>
              <div className="pp-reset-sub">
                내 정책 {policyCount}개 · 패키지 {setCount}개를 모두 삭제합니다.
              </div>
            </div>
            <button
              type="button"
              className="pp-btn danger"
              disabled={
                (policyCount === 0 && setCount === 0) || resetPoliciesMut.isPending
              }
              onClick={() => {
                if (
                  window.confirm(
                    `내 정책 ${policyCount}개와 패키지 ${setCount}개를 모두 삭제할까요?\n되돌릴 수 없어요.`,
                  )
                )
                  resetPoliciesMut.mutate();
              }}
            >
              {resetPoliciesMut.isPending ? "초기화 중…" : "정책 비우기"}
            </button>
          </div>
        </section>
      </div>
    </>
  );
}
