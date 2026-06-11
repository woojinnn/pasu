import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useLocation, useNavigate, useParams, useSearchParams } from "react-router-dom";

import { stripDashboardId, type PolicyMethod } from "../../../server-api";
import type { PolicySeverity } from "../../../server-api";
import {
  bindDef,
  deleteDef,
  getOverview,
  putDef,
  putPackage,
  updateBinding,
  type Binding,
  type PolicyDef,
  type StoreSnapshot,
} from "../../../server-api/policy-store";
import { listWallets } from "../../../server-api/wallets";
import { buildDefPayload } from "./save-def";
import { SaveScopeModal, type SaveScopeChoice } from "./SaveScopeModal";
import { defUsageCount } from "./wallet-policies-derive";
import { applyBindingEdit, diffBindingEdit } from "./binding-edit";
import { Topbar } from "../../../shell/Topbar";

import { stampAnnotations } from "../../../editor-v9/annotations";
import { generateManifest } from "../../../editor-v9/manifest-gen";
import type { PolicyIR } from "../../../cedar/blocks";

import { severityFromCedar } from "../policy-meta";
import { PublishModal, type PublishSource } from "../PublishModal";
// PublishModal classes (.publish-modal, .publish-modal-backdrop) are
// authored in market.css; pull it in so the modal renders with a solid
// background when launched from the v2 editor.
import "../../market.css";

import { catLabel, catStyle } from "./categories";
import { CatIcon, ShieldIcon, WarnIcon } from "./icons";
import { blocksToText, textToBlocks } from "../../../cedar";
import { concretizeIr } from "../../../cedar/blocks";
import { PolicyFormPane } from "./PolicyFormPane";
import { emptyFormModel, irToForm, type FormModel } from "../../../cedar/form";

type Tab = "cedar" | "form";

function defaultTab(method: PolicyMethod | undefined): Tab {
  // Legacy `block`-method policies fall through to Cedar — they keep their full
  // cedar text, so the Cedar tab opens them correctly.
  if (method === "form") return "form";
  return "cedar";
}

/** Result of trying to open the current policy in the form tab. `loading` while
 *  parsing cedar→IR; `closed` when the policy is outside the form-representable
 *  subset (complex OR/NOT/nesting). */
type FormEntry =
  | { kind: "loading" }
  | { kind: "ok"; model: FormModel }
  | { kind: "closed" };

/** Seed handed in by {@link NewPolicyChooser} via navigation state. Nothing is
 *  persisted until the user saves, so an abandoned new policy never exists. */
interface NewPolicySeed {
  method: PolicyMethod;
  cedarText: string;
  displayName: string;
}

/** 에디터 본문이 다루는 뷰모델 — 저장된 def(IR→텍스트 변환) 또는 새 정책 시드. */
interface EditorPolicy {
  id: string;
  displayName: string;
  text: string;
  /** 폼 초기 IR — 있으면 텍스트 재파싱 없이 바로 폼을 연다. 바인딩 모드에서는
   *  바인딩 파라미터가 적용된 구체 IR(이 지갑의 실제 값). */
  initialIr?: PolicyIR | undefined;
  method: PolicyMethod;
  cat?: string | undefined;
  source: PolicyDef["source"];
  sourceVersion?: string | undefined;
  manifest?: unknown;
}

export function EditorDetailPageV2() {
  const navigate = useNavigate();
  const location = useLocation();
  const params = useParams<{ id: string }>();
  const id = params.id ? decodeURIComponent(params.id) : "";
  const qc = useQueryClient();
  const [sp] = useSearchParams();

  const overviewQ = useQuery({ queryKey: ["ps2-overview"], queryFn: getOverview });
  const storedDef = overviewQ.data?.library.defs[id] ?? null;

  // 바인딩 편집 모드: ?wallet=<addr>&binding=<id> — 그 지갑 인스턴스의 값을 연다.
  const walletAddr = sp.get("wallet")?.toLowerCase() ?? null;
  const bindingId = sp.get("binding");
  const binding: Binding | null =
    (walletAddr && bindingId
      ? overviewQ.data?.wallets.byAddress[walletAddr]?.bindings[bindingId]
      : null) ?? null;
  const bindingCtx = storedDef && walletAddr && binding ? { address: walletAddr, binding } : null;

  // 폼/텍스트의 기준 IR: 바인딩 모드면 그 지갑의 파라미터를 적용한 구체 IR.
  const baseIr = useMemo(() => {
    if (!storedDef) return null;
    const ir = storedDef.skeleton.ir as PolicyIR;
    if (!bindingCtx) return ir;
    const live = new Set(storedDef.holes.map((h) => h.name));
    const merged = Object.fromEntries(
      Object.entries({ ...storedDef.defaults.params, ...bindingCtx.binding.params }).filter(
        ([k]) => live.has(k),
      ),
    );
    return concretizeIr(ir, merged as never);
  }, [storedDef, bindingCtx]);

  // def 뼈대(BlockIR)는 텍스트가 아니므로 Cedar 탭용 텍스트는 비동기로 렌더한다.
  const textQ = useQuery({
    queryKey: ["ps2-def-text", id, storedDef?.updatedAtMs ?? 0, bindingCtx?.binding.id ?? "", bindingCtx?.binding.updatedAtMs ?? 0],
    enabled: !!baseIr,
    queryFn: () => blocksToText(baseIr!),
  });

  // A fresh policy carried in via navigation state — nothing is written to
  // storage until the user saves (and picks a scope).
  const seed = (location.state as { newPolicy?: NewPolicySeed } | null)?.newPolicy;
  const isNew = !storedDef && !!seed;

  const policy = useMemo<EditorPolicy | null>(() => {
    if (storedDef) {
      if (textQ.data === undefined) return null; // IR→텍스트 변환 중
      return {
        id: storedDef.id,
        displayName: bindingCtx ? (bindingCtx.binding.alias ?? storedDef.displayName) : storedDef.displayName,
        text: textQ.data,
        initialIr: baseIr ?? undefined,
        // def에는 작성 방식이 저장되지 않는다 — 폼 우선으로 열고, 폼으로 표현
        // 불가하면 openForm이 Cedar 탭 안내로 떨어진다.
        method: "form",
        cat: storedDef.cat,
        source: storedDef.source,
        sourceVersion: storedDef.sourceVersion,
        manifest: storedDef.skeleton.manifest,
      };
    }
    if (seed) {
      return {
        id,
        displayName: seed.displayName,
        text: seed.cedarText,
        method: seed.method,
        source: "mine",
      };
    }
    return null;
  }, [storedDef, textQ.data, seed, id, bindingCtx, baseIr]);

  const loading = overviewQ.isLoading || (!!storedDef && textQ.isLoading);

  return (
    <>
      <Topbar
        here="Policy Editor"
        subtitle={policy ? policy.displayName : id || "…"}
        right={
          <Link to="/editor" className="ev2-back">
            ← 목록
          </Link>
        }
      />
      <div className="ev2-detail-body">
        {loading && !policy && <div className="ev2-status">불러오는 중…</div>}
        {!loading && !policy && (
          <div className="ev2-empty">
            <div className="big">정책을 찾을 수 없습니다</div>
            <div className="sm">
              <code>{id}</code>
              <br />
              <Link to="/editor">← 목록으로 돌아가기</Link>
            </div>
          </div>
        )}
        {policy && (
          <EditorBody
            key={`${policy.id}:${bindingCtx?.binding.id ?? ""}`}
            policy={policy}
            storedDef={storedDef}
            snap={overviewQ.data ?? null}
            bindingCtx={bindingCtx}
            isNew={isNew}
            onSaved={(savedId) => {
              void qc.invalidateQueries({ queryKey: ["ps2-overview"] });
              if (bindingCtx) {
                navigate("/editor"); // 지갑별 정책(기본 탭)으로 복귀
                return;
              }
              if (savedId !== id) {
                navigate(`/editor/${encodeURIComponent(savedId)}`, {
                  replace: true,
                });
              } else if (isNew) {
                // Drop the navigation seed so a reload doesn't re-enter new mode.
                navigate(`/editor/${encodeURIComponent(id)}`, { replace: true });
              }
            }}
            onDeleted={() => {
              void qc.invalidateQueries({ queryKey: ["ps2-overview"] });
              navigate("/editor");
            }}
          />
        )}
      </div>
    </>
  );
}

function EditorBody({
  policy,
  storedDef,
  snap,
  bindingCtx,
  isNew,
  onSaved,
  onDeleted,
}: {
  policy: EditorPolicy;
  storedDef: PolicyDef | null;
  snap: StoreSnapshot | null;
  bindingCtx: { address: string; binding: Binding } | null;
  isNew: boolean;
  onSaved: (id: string) => void;
  onDeleted: () => void;
}) {
  const [name, setName] = useState(() => policy.displayName);
  const [severity, setSeverity] = useState<PolicySeverity>(() =>
    severityFromCedar(policy.text),
  );
  const [cedarText, setCedarText] = useState(policy.text);
  const [ir, setIr] = useState<PolicyIR | null>(null);
  // A hand-edited manifest from the form, wrapped so `null` = no override
  // (auto-generate) is distinct from an override whose value is `undefined`.
  const [manifestOverride, setManifestOverride] = useState<{ value: unknown } | null>(null);
  const [tab, setTab] = useState<Tab>(() => defaultTab(policy.method));
  const [publishOpen, setPublishOpen] = useState(false);
  // Form tab: computed on entry from the live cedar/IR (not on every form edit,
  // so editing doesn't remount the form). `formKey` bumps to remount the pane
  // with a fresh `initialModel`.
  const [formEntry, setFormEntry] = useState<FormEntry | null>(null);
  const [formKey, setFormKey] = useState(0);

  // Reseed when the parent swaps to a different policy id.
  useEffect(() => {
    setName(policy.displayName);
    setSeverity(severityFromCedar(policy.text));
    setCedarText(policy.text);
    setTab(defaultTab(policy.method));
    setManifestOverride(null);
    setFormEntry(null);
  }, [policy.id]);

  const fromMarket = policy.source === "market";
  const cstyle = catStyle(policy.cat);

  // 신규 def 첫 저장의 범위 모달 — prepare()가 만든 페이로드를 들고 띄운다.
  const [scopeAsk, setScopeAsk] = useState<{ ir: PolicyIR; manifest: unknown } | null>(null);

  /** 저장 페이로드 준비. v2 저장 형식은 BlockIR이므로 IR이 필수 — Cedar 탭에서
   *  변환 불가한 구문이면 사유와 함께 저장을 거부한다. */
  const prepare = async (): Promise<{ ir: PolicyIR; manifest: unknown }> => {
    // Cedar 탭 저장은 구체 텍스트 기반이라 기존 파라미터(홀)가 해제된다 — 확인.
    if (tab === "cedar" && (storedDef?.holes.length ?? 0) > 0 && !ir) {
      const n = storedDef!.holes.length;
      if (!window.confirm(`Cedar 텍스트로 저장하면 지갑별 설정 ${n}개가 해제됩니다. 계속할까요?`)) {
        throw new Error("저장을 취소했어요");
      }
    }
    const stamped = stampAnnotations(cedarText, name.trim() || "untitled", severity);
    let effectiveIr = ir;
    if (!effectiveIr) {
      if (!stamped.trim()) throw new Error("정책 본문이 비어 있어요");
      try {
        effectiveIr = (await textToBlocks(stamped))[0] ?? null;
      } catch (err) {
        throw new Error(
          `이 Cedar 구문은 저장 형식(블록)으로 변환할 수 없어요: ${err instanceof Error ? err.message : String(err)}`,
        );
      }
      if (!effectiveIr) {
        throw new Error("이 Cedar 구문은 저장 형식(블록)으로 변환할 수 없어요");
      }
    }
    let manifest: unknown;
    if (tab === "form" && manifestOverride) {
      // The form supplied a hand-edited manifest — persist it as-is.
      manifest = manifestOverride.value;
    } else {
      // manifest 생성은 홀을 기본값으로 굳힌 구체 IR로 — 평가 시 렌더는 바인딩
      // 파라미터를 채운 IR을 따로 쓴다.
      const gen = generateManifest(concretizeIr(effectiveIr), undefined, { id: policy.id, severity });
      if (gen.errors.length > 0) {
        throw new Error(gen.errors.map((e) => e.message).join("\n"));
      }
      manifest = gen.manifest;
    }
    return { ir: effectiveIr, manifest };
  };

  /** 바인딩 모드 저장: 값만 바뀌면 숨은 홀 승격 + 이 바인딩의 params, 구조가
   *  바뀌면 confirm 후 이 지갑 전용 def로 분기(복제+리바인드). 이름 입력은
   *  바인딩 별칭이 된다. */
  const saveBindingEdit = async (editedIr: PolicyIR): Promise<string> => {
    const ctx = bindingCtx!;
    const def = storedDef!;
    const aliasInput = name.trim();
    const alias = aliasInput && aliasInput !== def.displayName ? aliasInput : undefined;
    const diff = diffBindingEdit(def.skeleton.ir as PolicyIR, editedIr);

    if (diff.kind === "structural") {
      // 지갑 인스턴스는 값만 다르게 가질 수 있다 — 구조가 다르면 그건 새 정책
      // 원본이고, 원본은 라이브러리에서 만든다(지갑 조작은 라이브러리에 흔적을
      // 남기지 않는다).
      window.alert(
        "조건의 구조가 바뀌어서 이 화면에서는 저장할 수 없어요.\n구조가 다른 정책이 필요하면 라이브러리에서 이 정책을 복제한 뒤 수정하고, 지갑에 추가해 주세요.",
      );
      throw new Error("저장을 취소했어요");
    }

    if (diff.kind === "params") {
      const applied = applyBindingEdit(def, diff.updates);
      if (applied.def !== def) await putDef(applied.def);
      // 기존 오버라이드 중 여전히 유효한 키를 유지하고 새 값을 얹는다.
      const live = new Set(applied.def.holes.map((h) => h.name));
      const params = Object.fromEntries(
        Object.entries({ ...ctx.binding.params, ...applied.params }).filter(([k]) => live.has(k)),
      );
      await updateBinding({
        address: ctx.address,
        bindingId: ctx.binding.id,
        patch: { params, ...(alias !== (ctx.binding.alias ?? undefined) ? { alias } : {}) },
      });
      return def.id;
    }

    // 값 변화 없음 — 별칭만 갱신될 수 있다.
    if (alias !== (ctx.binding.alias ?? undefined)) {
      await updateBinding({ address: ctx.address, bindingId: ctx.binding.id, patch: { alias } });
    }
    return def.id;
  };

  const saveMut = useMutation({
    mutationFn: async (): Promise<string | null> => {
      const prepared = await prepare();
      if (bindingCtx && storedDef) {
        return saveBindingEdit(prepared.ir);
      }
      if (isNew) {
        // 첫 저장: 범위 모달이 finishMut로 마무리한다.
        setScopeAsk(prepared);
        return null;
      }
      const { def } = buildDefPayload({
        existing: storedDef,
        displayName: name.trim() || "untitled",
        cat: policy.cat,
        ir: prepared.ir,
        manifest: prepared.manifest,
        scope: null,
        packageId: null,
        applyToNewWallets: null,
      });
      await putDef(def);
      return def.id;
    },
    onSuccess: (savedId) => {
      if (savedId) onSaved(savedId);
    },
  });

  // 범위 모달 confirm → (필요시 패키지 생성) → put-def + bind.
  const finishMut = useMutation({
    mutationFn: async (choice: SaveScopeChoice): Promise<string> => {
      if (!scopeAsk) throw new Error("내부 오류: 저장 준비가 비어 있어요");
      let pkgId = choice.packageId;
      if (pkgId === "__new__") {
        pkgId = `pkg::${crypto.randomUUID()}`;
        await putPackage({
          id: pkgId,
          displayName: choice.newPackageName ?? "새 패키지",
          source: "mine",
          updatedAtMs: Date.now(),
        });
      }
      const { def, bindPlan } = buildDefPayload({
        existing: null,
        displayName: name.trim() || "untitled",
        cat: policy.cat,
        ir: scopeAsk.ir,
        manifest: scopeAsk.manifest,
        scope: choice.scope,
        packageId: pkgId,
        applyToNewWallets: choice.applyToNewWallets,
      });
      await putDef(def);
      if (bindPlan) await bindDef(bindPlan);
      return def.id;
    },
    onSuccess: (savedId) => {
      setScopeAsk(null);
      onSaved(savedId);
    },
  });

  const usageCount = snap ? defUsageCount(snap, policy.id) : 0;
  const deleteMut = useMutation({
    mutationFn: async () => deleteDef(policy.id),
    onSuccess: () => onDeleted(),
  });

  // 범위 모달의 지갑 목록: 서버 지갑 ∪ ps2 지갑(소문자).
  const walletsQ = useQuery({ queryKey: ["wallets"], queryFn: listWallets, enabled: isNew });
  const modalWallets = useMemo(() => {
    const addrs = new Set([
      ...(walletsQ.data ?? []).map((w) => w.address.toLowerCase()),
      ...Object.keys(snap?.wallets.byAddress ?? {}),
    ]);
    return [...addrs].sort().map((address) => ({ address }));
  }, [walletsQ.data, snap]);
  const modalPackages = useMemo(
    () => Object.values(snap?.library.packages ?? {}),
    [snap],
  );

  const publishSource: PublishSource = {
    kind: "policy",
    cedarText,
    manifest: policy.manifest,
    policyTree: null,
    suggestedDisplayName: policy.displayName,
    suggestedSlug: stripDashboardId(policy.id),
  };

  /** Compute the form view from the live IR (or by parsing cedar). Sets
   *  `closed` when the policy can't be represented as a form. */
  const openForm = async () => {
    setFormEntry({ kind: "loading" });
    try {
      let effectiveIr = ir ?? policy.initialIr ?? null;
      if (!effectiveIr && cedarText.trim()) {
        effectiveIr = (await textToBlocks(cedarText))[0] ?? null;
      }
      const parsed = effectiveIr ? irToForm(effectiveIr) : emptyFormModel(stripDashboardId(policy.id));
      if (!parsed) {
        setFormEntry({ kind: "closed" });
        return;
      }
      // The editor header owns the policy id (slug) + severity; mirror them into
      // the form so its section-3 matches what save will stamp.
      setFormEntry({
        kind: "ok",
        model: { ...parsed, id: stripDashboardId(policy.id), severity: severity as FormModel["severity"] },
      });
      setFormKey((k) => k + 1);
    } catch (err) {
      console.warn("[editor] 폼 열기 실패 — Cedar 탭으로 안내:", err);
      setFormEntry({ kind: "closed" });
    }
  };

  const handleTabChange = (next: Tab) => {
    if (next === tab) return;
    if (next === "form") void openForm(); // recompute the form from latest cedar
    setTab(next);
  };

  // Open the form on first mount when it is the default tab (method === "form").
  useEffect(() => {
    if (tab === "form" && formEntry === null) void openForm();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab]);

  return (
    <div className="ev2-detail">
      <div className="ev2-detail-head">
        <div className="ev2-detail-title-row">
          <span className="ev2-cat-ic" style={cstyle.iconWrap}>
            <CatIcon cat={policy.cat} />
          </span>
          <input
            className="ev2-detail-title"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="정책 이름"
          />
          <span className="ev2-detail-slug">{stripDashboardId(policy.id)}</span>
          {/* 폼 탭은 ③ 심각도가 이 값을 소유(onChange로 동기화)하므로 헤더
              셀렉트는 Cedar 탭에서만 — 같은 값이 두 군데면 헷갈린다. */}
          {tab !== "form" && (
            <select
              value={severity}
              onChange={(e) => setSeverity(e.target.value as PolicySeverity)}
              className="ev2-detail-sev"
            >
              <option value="deny">deny (차단)</option>
              <option value="warn">warn (경고)</option>
              <option value="info">info (정보)</option>
            </select>
          )}
          {policy.cat && (
            <span className="ev2-cat-tag" style={cstyle.tag}>
              {catLabel(policy.cat)}
            </span>
          )}
        </div>

        <div className="ev2-detail-meta">
          {isNew && (
            <span className="ev2-badge-draft">
              새 정책 · 저장해야 적용됩니다
            </span>
          )}
          {bindingCtx && (
            <span className="ev2-badge-draft">
              {bindingCtx.address.slice(0, 6)}…{bindingCtx.address.slice(-4)} 지갑의 인스턴스 편집 —
              값 변경은 이 지갑에만 적용돼요
            </span>
          )}
          {fromMarket && (
            <span className="ev2-detail-prov">
              <ShieldIcon />
              마켓에서 가져옴
              {policy.sourceVersion ? ` · ${policy.sourceVersion}` : ""}
            </span>
          )}
        </div>

        <div className="ev2-detail-tabs" role="tablist">
          <TabBtn
            label="Cedar"
            active={tab === "cedar"}
            disabled={!!bindingCtx}
            onClick={() => {
              if (bindingCtx) return; // 지갑 인스턴스는 폼에서만 — 텍스트 편집은 라이브러리에서
              handleTabChange("cedar");
            }}
          />
          <TabBtn
            label="폼"
            active={tab === "form"}
            onClick={() => handleTabChange("form")}
          />
          <span className="ev2-spc" />
          {!bindingCtx && (
            <button
              type="button"
              className="ev2-pri ghost"
              onClick={() => setPublishOpen(true)}
              title="마켓에 올리기"
            >
              <ShieldIcon /> 마켓에 올리기
            </button>
          )}
          {!bindingCtx && (
          <button
            type="button"
            className="ev2-pri danger"
            onClick={() => {
              const extra = usageCount > 0 ? `\n${usageCount}개 지갑에서 함께 제거됩니다.` : "";
              if (!confirm(`정책 "${name}"을 삭제할까요?${extra}`)) return;
              deleteMut.mutate();
            }}
            disabled={deleteMut.isPending}
          >
            삭제
          </button>
          )}
          <button
            type="button"
            className="ev2-pri"
            onClick={() => saveMut.mutate()}
            disabled={saveMut.isPending || !cedarText.trim()}
          >
            {saveMut.isPending ? "저장 중…" : "저장"}
          </button>
        </div>
      </div>

      {(saveMut.error || finishMut.error || deleteMut.error) && (
        <div className="ev2-err-banner">
          <WarnIcon />
          {String(saveMut.error || finishMut.error || deleteMut.error || "")}
        </div>
      )}

      <div className="ev2-detail-tabbody">
        {tab === "cedar" && (
          <CedarPane
            value={cedarText}
            onChange={(next) => {
              setCedarText(next);
              // Drop the cached IR. Otherwise the form tab (openForm) and
              // save (manifest gen) reuse the IR captured by the last form/block
              // edit and the hand-typed cedar never reflects into form/block.
              setIr(null);
            }}
          />
        )}
        {tab === "form" &&
          (formEntry?.kind === "ok" ? (
            <PolicyFormPane
              key={formKey}
              initialModel={formEntry.model}
              initialManifest={policy.manifest}
              onChange={({ cedarText: c, ir: nextIr, model, manifest, manifestOverridden }) => {
                setCedarText(c);
                setIr(nextIr);
                // Keep the header severity in sync so save stamps it correctly.
                setSeverity(model.severity as PolicySeverity);
                // Carry the form's manifest override (if any) so save persists it
                // instead of re-generating.
                setManifestOverride(manifestOverridden ? { value: manifest } : null);
              }}
            />
          ) : formEntry?.kind === "closed" ? (
            <div className="ev2-empty">
              <div className="big">이 정책은 폼으로 열 수 없어요</div>
              <div className="sm">
                폼은 단순한 조건(AND/OR 비교)만 다뤄요. 부정(!)·중첩·if 같은 복잡한
                정책은 Cedar 탭에서 편집해 주세요.
              </div>
              <div className="ev2-empty-actions">
                <button type="button" className="ev2-pri ghost" onClick={() => handleTabChange("cedar")}>
                  Cedar 탭으로
                </button>
              </div>
            </div>
          ) : (
            <div className="ev2-empty">
              <div className="sm">폼을 불러오는 중…</div>
            </div>
          ))}
      </div>

      <PublishModal
        open={publishOpen}
        source={publishSource}
        onClose={() => setPublishOpen(false)}
      />

      <SaveScopeModal
        open={scopeAsk !== null}
        policyName={name.trim() || "untitled"}
        wallets={modalWallets}
        packages={modalPackages}
        busy={finishMut.isPending}
        onCancel={() => setScopeAsk(null)}
        onConfirm={(choice) => finishMut.mutate(choice)}
      />
    </div>
  );
}

function TabBtn(props: {
  label: string;
  active: boolean;
  disabled?: boolean;
  tooltip?: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={props.active}
      className={`ev2-tab${props.active ? " on" : ""}${
        props.disabled ? " is-disabled" : ""
      }`}
      onClick={props.onClick}
      disabled={props.disabled}
      title={props.tooltip}
    >
      {props.label}
      {props.disabled && <span className="ev2-tab-soon">준비 중</span>}
    </button>
  );
}

function CedarPane({
  value,
  onChange,
}: {
  value: string;
  onChange: (next: string) => void;
}) {
  return (
    <div className="ev2-cedar-pane">
      <div className="ev2-cedar-toolbar">
        <span className="ev2-cedar-hint">
          Cedar 코드를 직접 편집합니다. 저장 시 자동으로 <code>@id</code> /{" "}
          <code>@severity</code> 주석이 갱신됩니다.
        </span>
      </div>
      <textarea
        className="ev2-cedar-textarea"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        spellCheck={false}
        autoCorrect="off"
        autoCapitalize="off"
      />
    </div>
  );
}
