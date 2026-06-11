/**
 * Step 2 — pick policies. Three groups in order: 선택 지갑 관련 → 패키지 → 전체.
 * Selecting policies filters the right-hand state view down to the token/state
 * each enabled policy actually references (the rest dims out).
 */
import type { ReactNode } from "react";

import { StateDashboard } from "./StateDashboard";
import type { SimController, PkgState } from "./useSimController";
import type { PolicyView } from "./types";

export function Step2Policies({ c }: { c: SimController }) {
  const related = c.walletRelatedPolicies;
  const relatedIds = new Set(related.map((p) => p.id));
  const pkgPolicyIds = new Set(c.packages.flatMap((p) => p.policyIds));
  // 전체 = policies not already shown under 지갑관련/패키지.
  const rest = c.policies.filter((p) => !relatedIds.has(p.id) && !pkgPolicyIds.has(p.id));
  const filtering = c.hasRelevanceFilter;

  return (
    <div className="sw-step">
      <header className="sw-step-head">
        <h2>② 정책 선택</h2>
        <p>정책은 <b>지갑 단위</b>로 관리됩니다. 지갑을 고른 뒤 정책·패키지를 켜고 끄세요.</p>
      </header>

      <WalletSwitcher c={c} />

      <div className="sw-cols">
        <section className="sw-policies">
          {related.length > 0 && (
            <Group title="선택 지갑 관련 정책" hint="고른 지갑을 대상으로 하는 정책">
              {related.map((p) => (
                <PolicyRow key={p.id} p={p} on={c.enabled.has(p.id)} toggle={() => c.togglePolicy(p.id)} />
              ))}
            </Group>
          )}

          <Group title="정책 패키지" hint="패키지를 켜면 포함된 정책이 한 번에 켜집니다">
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
            <Group title="전체 정책" hint="그 밖의 모든 정책">
              {rest.map((p) => (
                <PolicyRow key={p.id} p={p} on={c.enabled.has(p.id)} toggle={() => c.togglePolicy(p.id)} />
              ))}
            </Group>
          )}
        </section>

        <aside className="sw-relstate">
          <div className="sw-relstate-head">
            <b>관련 상태</b>
            <span className="sw-mut">
              {filtering
                ? "켠 정책이 다루는 자산만 남고 나머지는 빠집니다"
                : "정책을 켜면 관련 자산만 남습니다"}
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
  const sel = c.wallets.filter((w) => c.selected.has(w.address));
  if (sel.length <= 1) {
    const w = sel[0];
    return (
      <div className="sw-wsw single">
        <span className="sw-mut">정책을 관리할 지갑:</span>
        <b className="sw-wsw-name">{w ? w.name : "선택된 지갑 없음"}</b>
        {w && <span className="sw-wsw-count">{c.enabledCount(w.address)}개 켜짐</span>}
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
  return (
    <button type="button" className={`sw-pkgrow ${state}`} onClick={toggle}>
      <span className={`sw-pkgtog ${state}`}>
        <span className="sw-pkgtog-dot" />
      </span>
      <span className="sw-pkg-name">{name}</span>
      <span className="sw-mut">정책 {count}개 · {state === "on" ? "전체 켜짐" : state === "partial" ? "일부 켜짐" : "꺼짐"}</span>
    </button>
  );
}
