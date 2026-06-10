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
  type Binding,
  type HoleValue,
  type PolicyDef,
  type StoreSnapshot,
} from "../../../server-api/policy-store";
import { listWallets } from "../../../server-api/wallets";
import { deriveMatrix } from "./apply-matrix-derive";
import { formatHoleValue, parseHoleInput } from "./hole-params";
import { PlusIcon, TrashIcon } from "./icons";

/** 적용 현황 — 지갑×패키지 매트릭스. 셀 = 패키지 토글 + 활성 바인딩 수,
 *  클릭 = 우측 바인딩 상세 패널(개별 토글·파라미터·복사/이동·정책 추가). */
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
    const missing = walletsQ.data
      .map((w) => w.address.toLowerCase())
      .filter((a) => !known[a]);
    provisioned.current = true;
    if (missing.length === 0) return;
    void provisionWallets(missing)
      .then(invalidate)
      .catch((err) => console.warn("[v2 apply] provisioning failed:", err));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [walletsQ.data, overviewQ.data]);

  const [sel, setSel] = useState<{ address: string; packageId: string } | null>(null);

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

  const togglePackage = async (address: string, packageId: string, enabled: boolean) => {
    try {
      await setPackageEnabled({ address, packageId, enabled });
      invalidate();
    } catch (err) {
      console.error("[v2 apply] package toggle failed:", err);
      onToast("패키지 상태를 바꾸지 못했어요");
    }
  };

  return (
    <div className={`pm-wrap${sel ? " with-panel" : ""}`}>
      <div className="pm-scroll">
        <table className="pm-grid">
          <thead>
            <tr>
              <th className="pm-walletcol">지갑</th>
              {matrix.cols.map((c) => (
                <th key={c.id}>{c.displayName}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {matrix.rows.map((row) => (
              <tr key={row.address}>
                <td className="pm-walletcol" title={row.address}>
                  {row.label ?? shortAddr(row.address)}
                </td>
                {matrix.cols.map((col) => {
                  const cell = matrix.cellOf(row.address, col.id);
                  const active = sel?.address === row.address && sel?.packageId === col.id;
                  return (
                    <td key={col.id} className={active ? "on" : ""}>
                      {cell.total === 0 ? (
                        <button
                          type="button"
                          className="pm-empty"
                          title="정책 추가"
                          onClick={() => setSel({ address: row.address, packageId: col.id })}
                        >
                          –
                        </button>
                      ) : (
                        <span className="pm-cell">
                          <label className="pm-switch" title="패키지 전체 켜기/끄기">
                            <input
                              type="checkbox"
                              checked={cell.packageOn}
                              onChange={(e) =>
                                void togglePackage(row.address, col.id, e.target.checked)
                              }
                            />
                            <span className="trk" />
                          </label>
                          <button
                            type="button"
                            className="pm-count"
                            title="바인딩 상세"
                            onClick={() => setSel({ address: row.address, packageId: col.id })}
                          >
                            {cell.activeBindings}/{cell.total}
                          </button>
                        </span>
                      )}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {sel && (
        <CellPanel
          snap={snap}
          address={sel.address}
          packageId={sel.packageId}
          allAddresses={matrix.rows.map((r) => r.address)}
          onClose={() => setSel(null)}
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

/* ─────────────── 셀 상세 패널 ─────────────── */

function CellPanel(props: {
  snap: StoreSnapshot;
  address: string;
  packageId: string;
  allAddresses: string[];
  onClose: () => void;
  onToast: (text: string) => void;
  invalidate: () => void;
}) {
  const { snap, address, packageId, allAddresses, onClose, onToast, invalidate } = props;
  const wallet = snap.wallets.byAddress[address] ?? { bindings: {}, packageEnabled: {} };
  const pkg = snap.library.packages[packageId];
  const bindings = Object.values(wallet.bindings)
    .filter((b) => b.packageId === packageId)
    .sort((a, b) => a.id.localeCompare(b.id));

  const [addDefId, setAddDefId] = useState("");
  const defs = useMemo(
    () =>
      Object.values(snap.library.defs).sort((a, b) =>
        a.displayName.localeCompare(b.displayName, "ko"),
      ),
    [snap],
  );

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

  return (
    <aside className="pm-panel">
      <div className="pm-panel-head">
        <div className="t">
          {shortAddr(address)} · {pkg?.displayName ?? packageId}
        </div>
        <button type="button" className="ev2-iconbtn" title="닫기" onClick={onClose}>
          ×
        </button>
      </div>

      <div className="pm-panel-body">
        {bindings.length === 0 && <div className="pm-none">이 칸에 적용된 정책이 없어요.</div>}

        {bindings.map((b) => {
          const def = snap.library.defs[b.defId];
          if (!def) return null;
          return (
            <BindingCard
              key={b.id}
              def={def}
              binding={b}
              effective={isEffectiveOn(wallet, b)}
              targets={allAddresses.filter((a) => a !== address)}
              onToggle={(on) =>
                void run("토글", () =>
                  updateBinding({ address, bindingId: b.id, patch: { enabled: on } }),
                )
              }
              onParams={(params) =>
                void run("파라미터 저장", () =>
                  updateBinding({ address, bindingId: b.id, patch: { params } }),
                )
              }
              onRemove={() =>
                void run("제거", () => removeBinding({ address, bindingId: b.id }))
              }
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
          );
        })}

        <div className="pm-add">
          <select value={addDefId} onChange={(e) => setAddDefId(e.target.value)}>
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
                bindDef({ defId: addDefId, packageId, addresses: [address] }),
              ).then((ok) => ok && setAddDefId(""))
            }
          >
            <PlusIcon /> 추가
          </button>
        </div>
      </div>
    </aside>
  );
}

function BindingCard(props: {
  def: PolicyDef;
  binding: Binding;
  effective: boolean;
  targets: string[];
  onToggle: (on: boolean) => void;
  onParams: (params: Record<string, HoleValue>) => void;
  onRemove: () => void;
  onCopy: (to: string) => void;
  onMove: (to: string) => void;
}) {
  const { def, binding, effective, targets, onToggle, onParams, onRemove, onCopy, onMove } = props;
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
