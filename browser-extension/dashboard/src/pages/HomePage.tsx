import { useEffect, useMemo, useState } from "react";
import { useQueries, useQuery, useQueryClient, useMutation } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";

import {
  getDashboardSummary,
  listAuditVerdicts,
  subscribeToBroadcast,
  syncWallet,
  type VerdictDto,
} from "../server-api";
import { getOverview } from "../server-api/policy-store";
import { useProvisionWallets } from "./use-provision-wallets";

import { AddWalletModal } from "../components/AddWalletModal";
import { RenameWalletModal } from "../components/RenameWalletModal";
import { Topbar } from "../shell/Topbar";

import { WalletGovernance, type DialWallet } from "./home/WalletGovernance";
import { DeleteWalletModal } from "./home/DeleteWalletModal";
import "./home.css";

/**
 * Home — wallet-dial governance.
 *
 * Replaces the old ContextBar + ActivePoliciesCard + vertical WalletList. The
 * dial + package/policy/param panel + all-wallets overview all live in
 * <WalletGovernance>; this file only fetches data and owns the wallet modals.
 */
export function HomePage() {
  const { t } = useTranslation("home");
  const qc = useQueryClient();
  const [addOpen, setAddOpen] = useState(false);
  const [renameFor, setRenameFor] = useState<DialWallet | null>(null);
  const [deleteFor, setDeleteFor] = useState<DialWallet | null>(null);

  const summaryQ = useQuery({ queryKey: ["dashboard", "summary"], queryFn: getDashboardSummary });
  const overviewQ = useQuery({ queryKey: ["ps2-overview"], queryFn: getOverview });

  // refetch the ps2 store when popup / other contexts mutate it
  useEffect(() => {
    return subscribeToBroadcast((keys) => {
      if (keys.some((k) => k.startsWith("ps2:"))) void qc.invalidateQueries({ queryKey: ["ps2-overview"] });
    });
  }, [qc]);

  const summaryWallets = summaryQ.data?.wallets ?? [];

  // 첫 로그인 직후: 서버 지갑이 ps2에 아직 없으면 기본 패키지가 안 보인다 —
  // 에디터처럼 홈도 마운트 시 프로비저닝한다(멱등).
  useProvisionWallets(
    summaryQ.isSuccess ? summaryWallets.map((w) => w.address) : null,
    overviewQ.data ?? null,
    () => void qc.invalidateQueries({ queryKey: ["ps2-overview"] }),
  );

  // per-wallet 24h verdicts → card tone
  const verdictQs = useQueries({
    queries: summaryWallets.map((w) => ({
      queryKey: ["wallet-verdicts", w.address],
      queryFn: () => listAuditVerdicts({ wallet: w.address, range: "24h" as const, limit: 50 }),
      enabled: summaryQ.isSuccess,
      refetchInterval: 60_000,
      retry: false,
    })),
  });

  const dialWallets: DialWallet[] = useMemo(
    () =>
      summaryWallets.map((w, i) => ({
        address: w.address,
        label: w.label,
        balanceUsd: Number(w.total_usd ?? "0"),
        tone: worstToneOf(verdictQs[i]?.data ?? []),
      })),
    [summaryWallets, verdictQs],
  );

  const totalUsd = Number(summaryQ.data?.total_portfolio_usd ?? "0");
  const subtitle = summaryQ.data
    ? t("head.summary", {
        count: summaryQ.data.wallet_count,
        total: "$" + totalUsd.toLocaleString("en-US", { maximumFractionDigits: 0 }),
      })
    : "…";

  const syncMut = useMutation({
    mutationFn: (address: string) => syncWallet(address),
    onSuccess: (_d, address) => {
      qc.invalidateQueries({ queryKey: ["dashboard"] });
      qc.invalidateQueries({ queryKey: ["wallet-verdicts", address] });
    },
  });

  return (
    <>
      <Topbar here="Pasu Home" subtitle={subtitle} showSearch={false} />

      <WalletGovernance
        wallets={dialWallets}
        snap={overviewQ.data ?? null}
        onSync={(address) => syncMut.mutate(address)}
        syncingAddress={syncMut.isPending ? syncMut.variables ?? null : null}
        onRename={setRenameFor}
        onDelete={setDeleteFor}
        onAddWallet={() => setAddOpen(true)}
      />

      <AddWalletModal open={addOpen} onClose={() => setAddOpen(false)} />
      <RenameWalletModal
        open={!!renameFor}
        onClose={() => setRenameFor(null)}
        address={renameFor?.address ?? ""}
        initial={renameFor?.label ?? null}
      />
      <DeleteWalletModal
        open={!!deleteFor}
        onClose={() => setDeleteFor(null)}
        address={deleteFor?.address ?? ""}
        label={deleteFor?.label ?? null}
      />
    </>
  );
}

function worstToneOf(verdicts: VerdictDto[]): "calm" | "warn" | "fail" {
  const open = verdicts.filter((v) => v.user_decision === null);
  if (open.some((v) => v.verdict === "fail")) return "fail";
  if (open.some((v) => v.verdict === "warn")) return "warn";
  return "calm";
}
