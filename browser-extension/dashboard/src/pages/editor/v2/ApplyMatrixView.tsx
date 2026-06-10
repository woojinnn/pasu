import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import {
  bindDef,
  copyBindings,
  getOverview,
  isEffectiveOn,
  provisionWallets,
  removeBinding,
  setPackageEnabled,
  updateBinding,
  UNCATEGORIZED_PKG,
  type Binding,
  type HoleValue,
  type PolicyDef,
  type StoreSnapshot,
  type WalletPolicyState,
} from "../../../server-api/policy-store";
import { listWallets } from "../../../server-api/wallets";
import { deriveMatrix, packageDisplayOn } from "./apply-matrix-derive";
import { DRAG_DEF_MIME, LibraryDirectory } from "./LibraryDirectory";
import { formatHoleValue, parseHoleInput } from "./hole-params";
import { FolderIcon, PlusIcon, TrashIcon } from "./icons";

/** 바인딩 행 드래그 페이로드 — 패키지 드롭 = 그 패키지에 인스턴스 "복사". */
const DRAG_BINDING_MIME = "application/x-pasu-binding-id";

/** 적용 현황 — 지갑별 워크스페이스. 좌: 그 지갑의 패키지, 우: 바인딩 카드,
 *  우측 서랍: 라이브러리 디렉토리(정의를 지갑 패키지로 끌어와 바인딩). */
export function ApplyMatrixView(props: { onToast: (text: string) => void }) {
  const { onToast } = props;
  const qc = useQueryClient();

  const walletsQ = useQuery({ queryKey: ["wallets"], queryFn: listWallets });
  const overviewQ = useQuery({ queryKey: ["ps2-overview"], queryFn: getOverview });
  const invalidate = () => void qc.invalidateQueries({ queryKey: ["ps2-overview"] });

  // 서버 지갑이 ps2 스토어에 아직 없으면 프로비저닝(멱등) — popup의
  // pasu-list-wallets 훅과 같은 역할을 대시보드 REST 경로에서도 보장한다.
  const provisioned = useRef(false);
  useEffect(() => {
    if (provisioned.current || !walletsQ.data || !overviewQ.data) return;
    const known = overviewQ.data.wallets.byAddress;
    const missing = walletsQ.data.map((w) => w.address.toLowerCase()).filter((a) => !known[a]);
    provisioned.current = true;
    if (missing.length === 0) return;
    void provisionWallets(missing)
      .then(invalidate)
      .catch((err) => console.warn("[v2 apply] provisioning failed:", err));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [walletsQ.data, overviewQ.data]);

  const [libOpen, setLibOpen] = useState(false);

  const snap = overviewQ.data ?? null;
  const matrix = useMemo(
    () =>
      snap
        ? deriveMatrix(
            snap,
            (walletsQ.data ?? []).map((w) => ({ address: w.address })),
          )
        : null,
    [snap, walletsQ.data],
  );

  const [addr, setAddr] = useState<string | null>(null);
  const activeAddr = addr ?? matrix?.rows[0]?.address ?? null;

  if (overviewQ.isLoading || !matrix || !snap) {
    return <div className="ev2-status">불러오는 중…</div>;
  }
  if (matrix.rows.length === 0) {
    return (
      <div className="ev2-empty">
        <div className="big">등록된 지갑이 없습니다</div>
        <div className="sm">확장 popup에서 지갑을 추가하면 여기에서 정책을 적용할 수 있어요.</div>
      </div>
    );
  }

  return (
    <div className="wd-wrap">
      <div className="wd-modes">
        {activeAddr && (
          <select className="wd-walletsel" value={activeAddr} onChange={(e) => setAddr(e.target.value)}>
            {matrix.rows.map((r) => (
              <option key={r.address} value={r.address}>
                {r.label ? `${r.label} (${shortAddr(r.address)})` : shortAddr(r.address)}
              </option>
            ))}
          </select>
        )}
        <span className="ev2-spc" />
        <button type="button" className={`ev2-sec${libOpen ? " on" : ""}`} onClick={() => setLibOpen((v) => !v)}>
          {libOpen ? "라이브러리 닫기" : "라이브러리에서 끌어오기"}
        </button>
      </div>

      {activeAddr && (
        <WalletWorkspace
          snap={snap}
          address={activeAddr}
          allAddresses={matrix.rows.map((r) => r.address)}
          libOpen={libOpen}
          onToast={onToast}
          invalidate={invalidate}
        />
      )}
    </div>
  );
}

function shortAddr(a: string): string {
  return a.length > 12 ? `${a.slice(0, 6)}…${a.slice(-4)}` : a;
}

/* ─────────────── 지갑별 워크스페이스 ─────────────── */

function WalletWorkspace(props: {
  snap: StoreSnapshot;
  address: string;
  allAddresses: string[];
  libOpen: boolean;
  onToast: (text: string) => void;
  invalidate: () => void;
}) {
  const { snap, address, allAddresses, libOpen, onToast, invalidate } = props;
  const wallet: WalletPolicyState = snap.wallets.byAddress[address] ?? {
    bindings: {},
    packageEnabled: {},
  };

  const [scope, setScope] = useState<string | "all">("all");
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const [addDefId, setAddDefId] = useState("");

  const run = async (label: string, fn: () => Promise<unknown>): Promise<boolean> => {
    try {
      await fn();
      invalidate();
      return true;
    } catch (err) {
      console.error(`[v2 apply] ${label} failed:`, err);
      onToast(`${label}에 실패했어요`);
      return false;
    }
  };

  // 패키지별 멤버(이 지갑의 바인딩). 라이브러리의 모든 패키지를 드롭 타깃으로
  // 보여주되, 이 지갑에 없는(멤버 0) 패키지는 흐리게 — "패키지는 지갑별" 표현.
  const membersByPkg = useMemo(() => {
    const m = new Map<string, Binding[]>();
    for (const b of Object.values(wallet.bindings)) {
      const arr = m.get(b.packageId) ?? [];
      arr.push(b);
      m.set(b.packageId, arr);
    }
    return m;
  }, [wallet]);

  const packages = useMemo(
    () =>
      Object.values(snap.library.packages).sort((a, b) =>
        a.id === UNCATEGORIZED_PKG ? -1 : b.id === UNCATEGORIZED_PKG ? 1 : a.id.localeCompare(b.id),
      ),
    [snap],
  );

  const defs = useMemo(
    () =>
      Object.values(snap.library.defs).sort((a, b) => a.displayName.localeCompare(b.displayName, "ko")),
    [snap],
  );

  /** 하이브리드 토글: 켜기 = 게이트 on + (전부 꺼져 있으면) 멤버 일괄 on;
   *  끄기 = 게이트 off(부분 상태 보존). */
  const togglePackage = (pkgId: string, members: Binding[], displayedOn: boolean) =>
    void run("패키지 토글", async () => {
      if (displayedOn) {
        await setPackageEnabled({ address, packageId: pkgId, enabled: false });
        return;
      }
      await setPackageEnabled({ address, packageId: pkgId, enabled: true });
      if (members.length > 0 && !members.some((b) => b.enabled)) {
        for (const b of members) {
          await updateBinding({ address, bindingId: b.id, patch: { enabled: true } });
        }
      }
    });

  /** 바인딩 드롭 = 그 패키지에 인스턴스 복사(파라미터 포함). */
  const dropOnPackage = (pkgId: string, bindingId: string) => {
    const src = wallet.bindings[bindingId];
    if (!src) return;
    if (src.packageId === pkgId) return;
    const exists = (membersByPkg.get(pkgId) ?? []).some((b) => b.defId === src.defId);
    if (exists) {
      onToast("이미 이 패키지에 같은 정책이 있어요");
      return;
    }
    const def = snap.library.defs[src.defId];
    void run("패키지에 복사", () =>
      bindDef({
        defId: src.defId,
        packageId: pkgId,
        addresses: [address],
        ...(src.params ? { params: src.params } : {}),
      }),
    ).then((ok) => ok && onToast(`${def?.displayName ?? "정책"}을 복사해 넣었어요`));
  };

  /** 라이브러리 정의 드롭 = 이 지갑의 그 패키지에 바인딩 추가. */
  const dropDefOnPackage = (pkgId: string, defId: string) => {
    const def = snap.library.defs[defId];
    if (!def) return;
    const exists = (membersByPkg.get(pkgId) ?? []).some((b) => b.defId === defId);
    if (exists) {
      onToast("이미 이 패키지에 같은 정책이 있어요");
      return;
    }
    void run("정책 적용", () =>
      bindDef({
        defId,
        packageId: pkgId,
        addresses: [address],
        ...(Object.keys(def.defaults.params).length ? { params: def.defaults.params } : {}),
      }),
    ).then((ok) => ok && onToast(`${def.displayName}을 이 지갑에 적용했어요`));
  };

  const visible = useMemo(() => {
    const all = Object.values(wallet.bindings).sort((a, b) => {
      const an = snap.library.defs[a.defId]?.displayName ?? a.defId;
      const bn = snap.library.defs[b.defId]?.displayName ?? b.defId;
      return an.localeCompare(bn, "ko");
    });
    return scope === "all" ? all : all.filter((b) => b.packageId === scope);
  }, [wallet, scope, snap]);

  const totalActive = Object.values(wallet.bindings).filter((b) => isEffectiveOn(wallet, b)).length;

  return (
    <div className="ev2-2col">
      <aside className="ev2-left">
        <div className="ev2-leftsec">
          <div className="ev2-lefthead">
            <span>이 지갑의 패키지</span>
          </div>
          <div className="ev2-pkglist">
            <button
              type="button"
              className={`ev2-pkgrow wd-scope${scope === "all" ? " on" : ""}`}
              onClick={() => setScope("all")}
            >
              <span className="nm">전체 정책</span>
              <span className="cnt">
                {totalActive}/{Object.keys(wallet.bindings).length}
              </span>
            </button>
            {packages.map((pkg) => {
              const members = membersByPkg.get(pkg.id) ?? [];
              const active = members.filter((b) => isEffectiveOn(wallet, b)).length;
              const displayedOn = packageDisplayOn(wallet.packageEnabled[pkg.id] ?? true,
                members.filter((b) => b.enabled).length);
              const empty = members.length === 0;
              return (
                <div
                  key={pkg.id}
                  className={`ev2-pkgrow wd-scope${scope === pkg.id ? " on" : ""}${empty ? " dim" : ""}${dropTarget === pkg.id ? " droptarget" : ""}`}
                  onClick={() => setScope(pkg.id)}
                  onDragOver={(e) => {
                    if (
                      e.dataTransfer.types.includes(DRAG_BINDING_MIME) ||
                      e.dataTransfer.types.includes(DRAG_DEF_MIME)
                    ) {
                      e.preventDefault();
                      setDropTarget(pkg.id);
                    }
                  }}
                  onDragLeave={() => setDropTarget((t) => (t === pkg.id ? null : t))}
                  onDrop={(e) => {
                    e.preventDefault();
                    setDropTarget(null);
                    const bindingId = e.dataTransfer.getData(DRAG_BINDING_MIME);
                    if (bindingId) {
                      dropOnPackage(pkg.id, bindingId);
                      return;
                    }
                    const defId = e.dataTransfer.getData(DRAG_DEF_MIME);
                    if (defId) dropDefOnPackage(pkg.id, defId);
                  }}
                >
                  <FolderIcon />
                  <span className="nm">{pkg.displayName}</span>
                  <span className="cnt">{empty ? "–" : `${active}/${members.length}`}</span>
                  {!empty && (
                    <label
                      className="pm-switch sm"
                      title="패키지 정책 전체 켜기/끄기"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <input
                        type="checkbox"
                        checked={displayedOn}
                        onChange={() => togglePackage(pkg.id, members, displayedOn)}
                      />
                      <span className="trk" />
                    </label>
                  )}
                </div>
              );
            })}
          </div>
          <div className="ev2-lefthint">
            정책을 끌어다 패키지에 놓으면 그 패키지로 <b>복사</b>돼요 — 같은 정책이 여러
            패키지에 있으면 서로 독립이에요.
          </div>
        </div>
      </aside>

      <section className="ev2-right">
        <div className="ev2-ctrl">
          <span className="wd-scopelabel">
            {scope === "all" ? "전체 정책" : (snap.library.packages[scope]?.displayName ?? scope)}
          </span>
          <span className="ev2-spc" />
          <select className="wd-addsel" value={addDefId} onChange={(e) => setAddDefId(e.target.value)}>
            <option value="">정책 선택…</option>
            {defs.map((d) => (
              <option key={d.id} value={d.id}>
                {d.displayName}
              </option>
            ))}
          </select>
          <button
            type="button"
            className="ev2-sec"
            disabled={!addDefId}
            onClick={() =>
              void run("정책 추가", () =>
                bindDef({
                  defId: addDefId,
                  packageId: scope === "all" ? UNCATEGORIZED_PKG : scope,
                  addresses: [address],
                }),
              ).then((ok) => ok && setAddDefId(""))
            }
          >
            <PlusIcon /> 추가
          </button>
        </div>

        <div className="ev2-scroll wd-cards">
          {visible.length === 0 && (
            <div className="ev2-empty">
              <div className="big">이 {scope === "all" ? "지갑" : "패키지"}에 정책이 없어요</div>
              <div className="sm">위의 “추가” 또는 라이브러리 탭에서 정책을 적용해 보세요.</div>
            </div>
          )}
          {visible.map((b) => {
            const def = snap.library.defs[b.defId];
            if (!def) return null;
            return (
              <div
                key={b.id}
                draggable
                onDragStart={(e) => {
                  e.dataTransfer.setData(DRAG_BINDING_MIME, b.id);
                  e.dataTransfer.effectAllowed = "copy";
                }}
              >
                <BindingCard
                  def={def}
                  binding={b}
                  effective={isEffectiveOn(wallet, b)}
                  pkgName={
                    scope === "all" ? (snap.library.packages[b.packageId]?.displayName ?? null) : null
                  }
                  targets={allAddresses.filter((a) => a !== address)}
                  onToggle={(on) =>
                    void run("토글", () => updateBinding({ address, bindingId: b.id, patch: { enabled: on } }))
                  }
                  onParams={(params) =>
                    void run("파라미터 저장", () =>
                      updateBinding({ address, bindingId: b.id, patch: { params } }),
                    )
                  }
                  onRemove={() => void run("제거", () => removeBinding({ address, bindingId: b.id }))}
                  onCopy={(to) =>
                    void run("복사", () =>
                      copyBindings({ fromAddress: address, toAddress: to, bindingIds: [b.id] }),
                    ).then((ok) => ok && onToast(`${shortAddr(to)}로 복사했어요`))
                  }
                  onMove={(to) =>
                    void run("이동", async () => {
                      await copyBindings({ fromAddress: address, toAddress: to, bindingIds: [b.id] });
                      await removeBinding({ address, bindingId: b.id });
                    }).then((ok) => ok && onToast(`${shortAddr(to)}로 옮겼어요`))
                  }
                />
              </div>
            );
          })}
        </div>
      </section>

      {libOpen && (
        <aside className="ev2-left wd-libdrawer">
          <div className="ev2-leftsec">
            <div className="ev2-lefthead">
              <span>라이브러리</span>
            </div>
            <div className="wd-libdrawer-body">
              <LibraryDirectory snap={snap} mode="pick" query="" catFilter="all" />
            </div>
            <div className="ev2-lefthint">
              정의를 끌어다 왼쪽 <b>지갑 패키지</b>에 놓으면 이 지갑에 적용돼요.
            </div>
          </div>
        </aside>
      )}
    </div>
  );
}

function BindingCard(props: {
  def: PolicyDef;
  binding: Binding;
  effective: boolean;
  /** "전체 정책" 스코프에서만 소속 패키지 칩 표시. */
  pkgName: string | null;
  targets: string[];
  onToggle: (on: boolean) => void;
  onParams: (params: Record<string, HoleValue>) => void;
  onRemove: () => void;
  onCopy: (to: string) => void;
  onMove: (to: string) => void;
}) {
  const { def, binding, effective, pkgName, targets, onToggle, onParams, onRemove, onCopy, onMove } =
    props;
  const [target, setTarget] = useState("");

  return (
    <div className={`pm-card${effective ? "" : " off"}`}>
      <div className="pm-card-head">
        <label className="pm-switch sm" title="이 정책만 켜기/끄기">
          <input
            type="checkbox"
            checked={binding.enabled}
            onChange={(e) => onToggle(e.target.checked)}
          />
          <span className="trk" />
        </label>
        <span className="nm">{def.displayName}</span>
        {pkgName && <span className="pm-pkgchip">{pkgName}</span>}
        <button type="button" className="ev2-iconbtn danger" title="이 지갑에서 제거" onClick={onRemove}>
          <TrashIcon />
        </button>
      </div>

      {def.holes.length > 0 && (
        <HoleParamsEditor
          holes={def.holes}
          values={{ ...def.defaults.params, ...binding.params }}
          onSave={onParams}
        />
      )}

      {targets.length > 0 && (
        <div className="pm-card-move">
          <select value={target} onChange={(e) => setTarget(e.target.value)}>
            <option value="">다른 지갑…</option>
            {targets.map((t) => (
              <option key={t} value={t}>
                {shortAddr(t)}
              </option>
            ))}
          </select>
          <button type="button" className="ev2-sec" disabled={!target} onClick={() => onCopy(target)}>
            복사
          </button>
          <button type="button" className="ev2-sec" disabled={!target} onClick={() => onMove(target)}>
            이동
          </button>
        </div>
      )}
    </div>
  );
}

/** def.holes의 HoleSpec.type별 입력 — 저장 시 모든 hole의 parse 결과를 모아 전달. */
function HoleParamsEditor(props: {
  holes: PolicyDef["holes"];
  values: Record<string, HoleValue>;
  onSave: (params: Record<string, HoleValue>) => void;
}) {
  const { holes, values, onSave } = props;
  const [drafts, setDrafts] = useState<Record<string, string>>(() =>
    Object.fromEntries(holes.map((h) => [h.name, formatHoleValue(values[h.name])])),
  );
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [dirty, setDirty] = useState(false);

  const setDraft = (name: string, raw: string) => {
    setDrafts((d) => ({ ...d, [name]: raw }));
    setDirty(true);
  };

  const save = () => {
    const params: Record<string, HoleValue> = {};
    const errs: Record<string, string> = {};
    for (const h of holes) {
      const r = parseHoleInput(h.type, drafts[h.name] ?? "");
      if (r.ok) params[h.name] = r.value;
      else errs[h.name] = r.error;
    }
    setErrors(errs);
    if (Object.keys(errs).length > 0) return;
    onSave(params);
    setDirty(false);
  };

  return (
    <div className="pm-holes">
      {holes.map((h) => (
        <label key={h.name} className="pm-hole">
          <span className="lb" title={h.desc}>
            {h.label}
          </span>
          {h.type === "bool" ? (
            <select value={drafts[h.name] || "false"} onChange={(e) => setDraft(h.name, e.target.value)}>
              <option value="true">예</option>
              <option value="false">아니오</option>
            </select>
          ) : h.type === "addressSet" ? (
            <textarea
              rows={2}
              value={drafts[h.name] ?? ""}
              placeholder="주소를 줄마다 하나씩"
              onChange={(e) => setDraft(h.name, e.target.value)}
            />
          ) : (
            <input value={drafts[h.name] ?? ""} onChange={(e) => setDraft(h.name, e.target.value)} />
          )}
          {errors[h.name] && <span className="err">{errors[h.name]}</span>}
        </label>
      ))}
      {dirty && (
        <button type="button" className="ev2-sec" onClick={save}>
          파라미터 저장
        </button>
      )}
    </div>
  );
}
