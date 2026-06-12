/**
 * Step 2 — pick policies. Three groups in order: 선택 지갑 관련 → 패키지 → 전체.
 * Selecting policies filters the right-hand state view down to the token/state
 * each enabled policy actually references (the rest dims out).
 */
import type { ReactNode } from "react";
import { Trans, useTranslation } from "react-i18next";

import { StateDashboard } from "./StateDashboard";
import type { SimController, PkgState } from "./useSimController";
import type { PolicyView } from "./types";

export function Step2Policies({ c }: { c: SimController }) {
  const { t } = useTranslation("simulation");
  const related = c.walletRelatedPolicies;
  const relatedIds = new Set(related.map((p) => p.id));
  const pkgPolicyIds = new Set(c.packages.flatMap((p) => p.policyIds));
  // 전체 = policies not already shown under 지갑관련/패키지.
  const rest = c.policies.filter((p) => !relatedIds.has(p.id) && !pkgPolicyIds.has(p.id));
  const filtering = c.hasRelevanceFilter;

  return (
    <div className="sw-step">
      <header className="sw-step-head">
        <h2>{t("wizard.step2.title")}</h2>
        <p>
          <Trans t={t} i18nKey="wizard.step2.desc" components={{ b: <b /> }} />
        </p>
      </header>

      <WalletSwitcher c={c} />

      <div className="sw-cols">
        <section className="sw-policies">
          {related.length > 0 && (
            <Group title={t("wizard.step2.groupRelated")} hint={t("wizard.step2.groupRelatedHint")}>
              {related.map((p) => (
                <PolicyRow key={p.id} p={p} on={c.enabled.has(p.id)} toggle={() => c.togglePolicy(p.id)} />
              ))}
            </Group>
          )}

          <Group title={t("wizard.step2.groupPackages")} hint={t("wizard.step2.groupPackagesHint")}>
            {c.packages.map((pkg) => (
              <div key={pkg.id} className="sw-pkg">
                <PkgRow
                  name={pkg.name}
                  count={pkg.policyIds.length}
                  state={c.packageState(pkg.id)}
                  toggle={() => c.togglePackage(pkg.id)}
                />
                <div className="sw-pkg-kids">
                  {pkg.policyIds
                    .map((id) => c.policies.find((p) => p.id === id))
                    .filter((p): p is PolicyView => Boolean(p))
                    .map((p) => (
                      <PolicyRow key={p.id} p={p} on={c.enabled.has(p.id)} toggle={() => c.togglePolicy(p.id)} small />
                    ))}
                </div>
              </div>
            ))}
          </Group>

          {rest.length > 0 && (
            <Group title={t("wizard.step2.groupAll")} hint={t("wizard.step2.groupAllHint")}>
              {rest.map((p) => (
                <PolicyRow key={p.id} p={p} on={c.enabled.has(p.id)} toggle={() => c.togglePolicy(p.id)} />
              ))}
            </Group>
          )}
        </section>

        <aside className="sw-relstate">
          <div className="sw-relstate-head">
            <b>{t("wizard.step2.relatedState")}</b>
            <span className="sw-mut">
              {filtering ? t("wizard.step2.relatedStateFiltering") : t("wizard.step2.relatedStateIdle")}
            </span>
          </div>
          {c.activeState && (
            <StateDashboard
              key={c.activeWallet}
              s={c.activeState}
              entrance={false}
              filter={{
                active: filtering,
                isWidgetRelevant: c.isWidgetRelevant,
                isTokenRelevant: c.isTokenRelevant,
                isProtocolRelevant: c.isProtocolRelevant,
              }}
            />
          )}
        </aside>
      </div>
    </div>
  );
}

/** Per-wallet tabs — pick which selected wallet's policy set you're editing. */
function WalletSwitcher({ c }: { c: SimController }) {
  const { t } = useTranslation("simulation");
  const sel = c.wallets.filter((w) => c.selected.has(w.address));
  if (sel.length <= 1) {
    const w = sel[0];
    return (
      <div className="sw-wsw single">
        <span className="sw-mut">{t("wizard.step2.managingWallet")}</span>
        <b className="sw-wsw-name">{w ? w.name : t("wizard.step2.noWalletSelected")}</b>
        {w && (
          <span className="sw-wsw-count">
            {t("wizard.step2.enabledCount", { count: c.enabledCount(w.address) })}
          </span>
        )}
      </div>
    );
  }
  return (
    <div className="sw-wsw">
      {sel.map((w) => (
        <button
          key={w.address}
          type="button"
          className={`sw-wtab${c.activeWallet === w.address ? " on" : ""}`}
          onClick={() => c.setActiveWallet(w.address)}
        >
          <span className="sw-wtab-name">{w.name}</span>
          <span className="sw-wtab-count">{c.enabledCount(w.address)}</span>
        </button>
      ))}
    </div>
  );
}

function Group({ title, hint, children }: { title: string; hint: string; children: ReactNode }) {
  return (
    <div className="sw-group">
      <div className="sw-group-head">
        <span className="sw-group-title">{title}</span>
        <span className="sw-mut">{hint}</span>
      </div>
      {children}
    </div>
  );
}

function PolicyRow({ p, on, toggle, small }: { p: PolicyView; on: boolean; toggle: () => void; small?: boolean }) {
  return (
    <label className={`sw-policy${on ? " on" : ""}${small ? " small" : ""}`}>
      <input type="checkbox" checked={on} onChange={toggle} />
      <span className="sw-policy-name">{p.name}</span>
      <span className="sw-policy-action">{p.action}</span>
    </label>
  );
}

function PkgRow({ name, count, state, toggle }: { name: string; count: number; state: PkgState; toggle: () => void }) {
  const { t } = useTranslation("simulation");
  const stateLabel =
    state === "on"
      ? t("wizard.step2.pkgStateOn")
      : state === "partial"
        ? t("wizard.step2.pkgStatePartial")
        : t("wizard.step2.pkgStateOff");
  return (
    <button type="button" className={`sw-pkgrow ${state}`} onClick={toggle}>
      <span className={`sw-pkgtog ${state}`}>
        <span className="sw-pkgtog-dot" />
      </span>
      <span className="sw-pkg-name">{name}</span>
      <span className="sw-mut">{t("wizard.step2.pkgCount", { count })} · {stateLabel}</span>
    </button>
  );
}
