import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";

import {
  deleteListing,
  deleteWallet,
  listListings,
  listWallets,
  pickI18n,
} from "../server-api";
import { deleteDef, deletePackage, getOverview, UNCATEGORIZED_PKG } from "../server-api/policy-store";
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
  const { t } = useTranslation("common");
  const navigate = useNavigate();
  const qc = useQueryClient();
  const { user, logout } = useAuth();

  const myListingsQ = useQuery({
    queryKey: ["my-listings", user?.user_id],
    queryFn: () => listListings({ publisher_id: user!.user_id, limit: 100 }),
    enabled: !!user?.user_id,
  });
  const walletsQ = useQuery({ queryKey: ["wallets"], queryFn: listWallets });
  const overviewQ = useQuery({ queryKey: ["ps2-overview"], queryFn: getOverview });

  const walletCount = walletsQ.data?.length ?? 0;
  const policyCount = overviewQ.data ? Object.keys(overviewQ.data.library.defs).length : 0;
  const setCount = overviewQ.data
    ? Object.keys(overviewQ.data.library.packages).filter((id) => id !== UNCATEGORIZED_PKG).length
    : 0;

  const [banner, setBanner] = useState<string | null>(null);

  const resetWalletsMut = useMutation({
    mutationFn: async () => {
      const wallets = walletsQ.data ?? [];
      for (const w of wallets) await deleteWallet(w.address);
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["wallets"] });
      setBanner(t("profile.resetWalletsDone"));
    },
    onError: (e) => setBanner(t("profile.resetWalletsFailed", { error: String(e) })),
  });

  const resetPoliciesMut = useMutation({
    mutationFn: async () => {
      const snap = overviewQ.data;
      if (!snap) return;
      // 정의 삭제가 바인딩을 cascade하고, 패키지 삭제는 미분류로 해체한다.
      for (const id of Object.keys(snap.library.defs)) await deleteDef(id);
      for (const id of Object.keys(snap.library.packages)) {
        if (id !== UNCATEGORIZED_PKG) await deletePackage(id);
      }
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["ps2-overview"] });
      setBanner(t("profile.resetPoliciesDone"));
    },
    onError: (e) => setBanner(t("profile.resetPoliciesFailed", { error: String(e) })),
  });

  const deleteListingMut = useMutation({
    mutationFn: (listingId: string) => deleteListing(listingId),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["my-listings"] });
      void qc.invalidateQueries({ queryKey: ["market-listings"] });
      setBanner(t("profile.deleteListingDone"));
    },
    onError: (e) => setBanner(t("profile.deleteListingFailed", { error: String(e) })),
  });

  const onLogout = () => {
    logout();
    navigate("/login", { replace: true });
  };

  const email = user?.email ?? "—";
  const initials = email.slice(0, 2).toUpperCase();

  return (
    <>
      <Topbar here={t("profile.title")} subtitle={t("profile.subtitle")} showSearch={false} />
      <div className="pp-body">
        {banner && <div className="pp-banner">{banner}</div>}

        {/* identity */}
        <section className="pp-card pp-identity">
          <span className="pp-av">{initials}</span>
          <div className="pp-id-meta">
            <div className="pp-email">{email}</div>
          </div>
          <button type="button" className="pp-btn ghost danger" onClick={onLogout}>
            {t("profile.signOut")}
          </button>
        </section>

        {/* published posts */}
        <section className="pp-card">
          <div className="pp-sec-head">
            <h2>{t("profile.myListings")}</h2>
            <span className="pp-count">{myListingsQ.data?.length ?? 0}</span>
          </div>
          {myListingsQ.isLoading && <div className="pp-muted">{t("loading")}</div>}
          {myListingsQ.data && myListingsQ.data.length === 0 && (
            <div className="pp-empty">{t("profile.noListings")}</div>
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
                      <span title={t("profile.installCount")}>↓ {l.install_count}</span>
                      {l.current_version && (
                        <span className="pp-ver">{l.current_version}</span>
                      )}
                      <span className={`pp-status ${l.status}`}>{l.status}</span>
                    </div>
                  </Link>
                  <button
                    type="button"
                    className="pp-listing-del"
                    title={t("profile.deleteListingTitle")}
                    disabled={deleteListingMut.isPending}
                    onClick={() => {
                      if (
                        window.confirm(
                          t("profile.deleteListingConfirm", { name: pickI18n(l.display_name) }),
                        )
                      )
                        deleteListingMut.mutate(l.id);
                    }}
                  >
                    {t("delete")}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>

        {/* reset switches */}
        <section className="pp-card">
          <div className="pp-sec-head">
            <h2>{t("profile.resetSection")}</h2>
          </div>
          <p className="pp-muted">{t("profile.resetDesc")}</p>

          <div className="pp-reset-row">
            <div className="pp-reset-info">
              <div className="pp-reset-title">{t("profile.resetWalletsTitle")}</div>
              <div className="pp-reset-sub">
                {t("profile.resetWalletsSub", { count: walletCount })}
              </div>
            </div>
            <button
              type="button"
              className="pp-btn danger"
              disabled={walletCount === 0 || resetWalletsMut.isPending}
              onClick={() => {
                if (
                  window.confirm(
                    t("profile.resetWalletsConfirm", { count: walletCount }),
                  )
                )
                  resetWalletsMut.mutate();
              }}
            >
              {resetWalletsMut.isPending ? t("profile.resetting") : t("profile.resetWalletsBtn")}
            </button>
          </div>

          <div className="pp-reset-row">
            <div className="pp-reset-info">
              <div className="pp-reset-title">{t("profile.resetPoliciesTitle")}</div>
              <div className="pp-reset-sub">
                {t("profile.resetPoliciesSub", { policyCount, setCount })}
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
                    t("profile.resetPoliciesConfirm", { policyCount, setCount }),
                  )
                )
                  resetPoliciesMut.mutate();
              }}
            >
              {resetPoliciesMut.isPending ? t("profile.resetting") : t("profile.resetPoliciesBtn")}
            </button>
          </div>
        </section>
      </div>
    </>
  );
}
