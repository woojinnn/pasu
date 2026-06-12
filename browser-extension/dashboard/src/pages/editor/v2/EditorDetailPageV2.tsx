import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useLocation, useNavigate, useParams, useSearchParams } from "react-router-dom";
import { Trans, useTranslation } from "react-i18next";

import { stripDashboardId, type PolicyMethod } from "../../../server-api";
import type { PolicySeverity } from "../../../server-api";
import {
  bindDef,
  deleteDef,
  getOverview,
  putDef,
  putPackage,
  putWalletFolder,
  updateBinding,
  type Binding,
  type PolicyDef,
  type StoreSnapshot,
} from "../../../server-api/policy-store";
import { listWallets } from "../../../server-api/wallets";
import { buildDefPayload } from "./save-def";
import { SaveScopeModal, WALLET_FOLDER_UNCAT, type SaveScopeChoice } from "./SaveScopeModal";
import { defUsageCount } from "./wallet-policies-derive";
import {
  canonicalizeModel,
  diffParamValues,
  parameterizeModel,
  structureKey,
} from "../../../cedar/form/parameterize";
import { holesFromIr } from "./save-def";
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
import {
  emptyFormModel,
  findInvalidIrDecimals,
  findInvalidModelDecimals,
  formToIr,
  irToForm,
  type FormModel,
} from "../../../cedar/form";

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
  const { t } = useTranslation("editor");
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
            {t("detail.backToList")}
          </Link>
        }
      />
      <div className="ev2-detail-body">
        {loading && !policy && <div className="ev2-status">{t("common:loading")}</div>}
        {!loading && !policy && (
          <div className="ev2-empty">
            <div className="big">{t("detail.notFoundTitle")}</div>
            <div className="sm">
              <code>{id}</code>
              <br />
              <Link to="/editor">{t("detail.backToListLong")}</Link>
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
              if (isNew) {
                // 새 정책 저장 완료 → "+ 새 정책"을 눌렀던 목록으로 복귀.
                // (상세에 머무르지 않는다 — 저장이 끝났다는 감각 + 다음 작업 동선)
                navigate("/editor", { replace: true });
                return;
              }
              if (savedId !== id) {
                navigate(`/editor/${encodeURIComponent(savedId)}`, {
                  replace: true,
                });
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
  const { t } = useTranslation("editor");
  const [name, setName] = useState(() => policy.displayName);
  const [severity, setSeverity] = useState<PolicySeverity>(() =>
    severityFromCedar(policy.text),
  );
  const [cedarText, setCedarText] = useState(policy.text);
  const [ir, setIr] = useState<PolicyIR | null>(null);
  const [lastModel, setLastModel] = useState<FormModel | null>(null);
  // A hand-edited manifest from the form, wrapped so `null` = no override
  // (auto-generate) is distinct from an override whose value is `undefined`.
  const [manifestOverride, setManifestOverride] = useState<{ value: unknown } | null>(null);
  const [tab, setTab] = useState<Tab>(() => defaultTab(policy.method));
  const [publishOpen, setPublishOpen] = useState(false);
  // Manifest computed at publish time so an UNSAVED policy still ships its
  // auto-generated manifest to the market (otherwise the listing carries
  // `policy.manifest` = undefined, and an installed manifest-less def crashes
  // evaluation — the marketplace anti-liquidation-guard bug). Wrapped so
  // `null` = "use the saved manifest" is distinct from a computed `undefined`.
  const [publishManifest, setPublishManifest] = useState<{ value: unknown } | null>(null);
  // Form tab: computed on entry from the live cedar/IR (not on every form edit,
  // so editing doesn't remount the form). `formKey` bumps to remount the pane
  // with a fresh `initialModel`.
  const [formEntry, setFormEntry] = useState<FormEntry | null>(null);
  const [formKey, setFormKey] = useState(0);
  // 값 시트(바인딩) 유효성 + "형식오류 → 변경 전으로 되돌리기" 배선.
  const [formValidity, setFormValidity] = useState<{ valid: boolean; error: string | null }>({
    valid: true,
    error: null,
  });
  const [resetToken, setResetToken] = useState(0);
  const [revertNotice, setRevertNotice] = useState<string | null>(null);

  // Reseed when the parent swaps to a different policy id.
  useEffect(() => {
    setName(policy.displayName);
    setSeverity(severityFromCedar(policy.text));
    setCedarText(policy.text);
    setTab(defaultTab(policy.method));
    setManifestOverride(null);
    setFormEntry(null);
    setFormValidity({ valid: true, error: null });
    setRevertNotice(null);
  }, [policy.id]);

  const fromMarket = policy.source === "market";
  const cstyle = catStyle(policy.cat);

  // 신규 def 첫 저장의 범위 모달 — prepare()가 만든 페이로드를 들고 띄운다.
  const [scopeAsk, setScopeAsk] = useState<{ ir: PolicyIR; manifest: unknown } | null>(null);

  /** 저장 페이로드 준비. v2 저장 형식은 BlockIR이므로 IR이 필수 — Cedar 탭에서
   *  변환 불가한 구문이면 사유와 함께 저장을 거부한다. */
  const prepare = async (): Promise<{ ir: PolicyIR; manifest: unknown; model: FormModel | null }> => {
    const stamped = stampAnnotations(cedarText, name.trim() || "untitled", severity);
    let effectiveIr = ir;
    if (!effectiveIr) {
      if (!stamped.trim()) throw new Error(t("detail.emptyBody"));
      // Cedar 컴파일 체크: text→EST가 wasm의 Cedar 파서를 통과해야 한다 —
      // 컴파일 안 되는 텍스트는 여기서 저장이 거부된다.
      try {
        effectiveIr = (await textToBlocks(stamped))[0] ?? null;
      } catch (err) {
        throw new Error(
          t("detail.cedarCompileError", {
            message: err instanceof Error ? err.message : String(err),
          }),
        );
      }
      if (!effectiveIr) {
        throw new Error(t("detail.cedarNotBlocks"));
      }
    }
    // 템플릿 저장 형식: 폼 호환이면 모든 값 자리를 파라미터로(form-canonical).
    // 폼이 못 여는 복잡한 정책은 구체 IR 그대로(파라미터 없는 템플릿).
    let finalIr: PolicyIR = effectiveIr;
    const editedModel =
      tab === "form" && lastModel ? lastModel : irToForm(effectiveIr);
    if (editedModel) {
      const stampedModel = {
        ...editedModel,
        id: stripDashboardId(policy.id),
        severity: severity as FormModel["severity"],
      };
      finalIr = formToIr(parameterizeModel(stampedModel));
    } else if ((storedDef?.holes.length ?? 0) > 0) {
      if (!window.confirm(t("detail.holesResetConfirm"))) {
        throw new Error(t("detail.saveCancelled"));
      }
    }
    // 메타 검증: 심각도·사유가 채워졌는지. (Cedar 컴파일은 위 textToBlocks가,
    // 폼 경로는 IR 생성이 보장한다.)
    if (!["deny", "warn", "info"].includes(severity)) {
      throw new Error(t("detail.severityRequired"));
    }
    const reasonText = (
      editedModel?.reason ??
      finalIr.annotations.find((a) => a.name === "reason")?.value ??
      ""
    ).trim();
    if (!reasonText) {
      throw new Error(t("detail.reasonRequired"));
    }
    // decimal 리터럴 형식: Cedar 파서는 통과하지만 엔진 설치 시 거부돼
    // 모든 요청이 막히므로(fail-closed) 저장 단계에서 잡는다.
    const badDecimals = findInvalidIrDecimals(concretizeIr(finalIr));
    if (badDecimals.length > 0) {
      throw new Error(
        t("detail.badDecimalsSave", {
          values: badDecimals.map((v) => `"${v}"`).join(", "),
        }),
      );
    }
    let manifest: unknown;
    if (tab === "form" && manifestOverride) {
      // The form supplied a hand-edited manifest — persist it as-is.
      manifest = manifestOverride.value;
    } else {
      // manifest 생성은 홀을 기본값으로 굳힌 구체 IR로 — 평가 시 렌더는 바인딩
      // 파라미터를 채운 IR을 따로 쓴다.
      const gen = generateManifest(concretizeIr(finalIr), undefined, { id: policy.id, severity });
      if (gen.errors.length > 0) {
        throw new Error(gen.errors.map((e) => e.message).join("\n"));
      }
      manifest = gen.manifest;
    }
    return { ir: finalIr, manifest, model: editedModel };
  };

  /** 바인딩(인스턴스) 저장: 구조가 같으면 "달라진 값"만 이 바인딩의 params로.
   *  def는 아직 파라미터화 전이면 한 번 canonical 형태로 승격(의미 불변)하고,
   *  이후로는 절대 건드리지 않는다. 구조가 다르면 복제 안내. */
  const saveBindingEdit = async (editedModel: FormModel | null): Promise<string> => {
    const ctx = bindingCtx!;
    const def = storedDef!;
    const aliasInput = name.trim();
    const alias = aliasInput && aliasInput !== def.displayName ? aliasInput : undefined;
    const defModel = irToForm(def.skeleton.ir as PolicyIR);
    if (!defModel || !editedModel) {
      window.alert(t("detail.noFormForBinding"));
      throw new Error(t("detail.saveCancelled"));
    }
    // 인스턴스 저장은 prepare()를 거치지 않으니 decimal 값을 여기서 검증한다.
    // ("3"처럼 정규화로 고쳐지는 값은 직렬화가 알아서 고치고, 숫자가 아닌
    // 값만 걸린다.)
    const badDecimals = findInvalidModelDecimals(editedModel);
    if (badDecimals.length > 0) {
      window.alert(
        t("detail.badDecimalsBinding", {
          values: badDecimals.map((v) => `"${v}"`).join(", "),
        }),
      );
      throw new Error(t("detail.saveCancelled"));
    }
    if (structureKey(canonicalizeModel(defModel)) !== structureKey(canonicalizeModel(editedModel))) {
      // 지갑 전용 정책이 이 지갑에만 묶여 있으면 구조도 자유 — 템플릿의 집이
      // 이 지갑이고, 바뀌어도 다른 지갑에 퍼질 게 없다.
      const soleOwner =
        def.hidden &&
        snap !== null &&
        Object.entries(snap.wallets.byAddress).every(
          ([addr, w]) =>
            addr === ctx.address ||
            Object.values(w.bindings).every((b) => b.defId !== def.id),
        );
      if (soleOwner) {
        const pEdited = formToIr(parameterizeModel(canonicalizeModel(editedModel)));
        const { holes, paramDefaults } = holesFromIr(pEdited);
        await putDef({
          ...def,
          displayName: aliasInput || def.displayName,
          skeleton: { ...def.skeleton, ir: pEdited },
          holes,
          defaults: { ...def.defaults, params: paramDefaults },
          updatedAtMs: Date.now(),
        });
        // 구조가 바뀌었으니 옛 오버라이드는 의미를 잃는다 — 비운다(값=새 기본).
        await updateBinding({ address: ctx.address, bindingId: ctx.binding.id, patch: { params: {} } });
        return def.id;
      }
      window.alert(t("detail.structureLockedBinding"));
      throw new Error(t("detail.saveCancelled"));
    }

    // def가 아직 파라미터화 전이면 canonical 파라미터 형태로 1회 승격(의미 불변).
    // 지갑 전용(hidden) def의 이름 변경은 별칭이 아니라 def 자체에 저장한다 —
    // 이 지갑에만 존재하는 정책의 유일한 이름이고, 그래야 지갑별 정책 목록과
    // 게시 모달에도 그 이름이 보인다. 공유 def만 바인딩 별칭을 쓴다.
    const renameDef = alias !== undefined && def.hidden === true;
    // 사유(reason)는 파라미터가 아니라 정책 공통(def 뼈대)이다. 시트에서 바꾼
    // 사유를 뼈대에 반영 — 바뀌면 아래 putDef가 발동해 모든 지갑에 적용된다.
    const skeletonModel = { ...defModel, reason: editedModel.reason };
    const pIr = formToIr(parameterizeModel(canonicalizeModel(skeletonModel)));
    if (
      renameDef ||
      def.holes.length === 0 ||
      JSON.stringify(def.skeleton.ir) !== JSON.stringify(pIr)
    ) {
      const { holes, paramDefaults } = holesFromIr(pIr);
      await putDef({
        ...def,
        ...(renameDef ? { displayName: aliasInput } : {}),
        skeleton: { ...def.skeleton, ir: pIr },
        holes,
        defaults: { ...def.defaults, params: paramDefaults },
        updatedAtMs: Date.now(),
      });
    }

    // 값 오버라이드: 템플릿 기본값과 같아진 항목은 자연히 빠진다(기본값 상속).
    const params = diffParamValues(defModel, editedModel);
    const aliasPatch = renameDef
      ? {} // 이름은 def로 갔다 — 별칭은 만들지 않는다(기존 별칭은 같은 값이라 무해)
      : alias !== (ctx.binding.alias ?? undefined)
        ? { alias }
        : {};
    await updateBinding({
      address: ctx.address,
      bindingId: ctx.binding.id,
      patch: { params, ...aliasPatch },
    });
    return def.id;
  };

  const saveMut = useMutation({
    mutationFn: async (): Promise<string | null> => {
      const prepared = await prepare();
      if (bindingCtx && storedDef) {
        return saveBindingEdit(prepared.model);
      }
      // 템플릿 구조 잠금: 지갑에 적용된 def는 구조를 바꿀 수 없다(값·이름·심각도는
      // 가능). 구조가 다른 정책이 필요하면 복제.
      if (!isNew && storedDef && snap) {
        const usage = Object.values(snap.wallets.byAddress).reduce(
          (n, w) => n + Object.values(w.bindings).filter((b) => b.defId === storedDef.id).length,
          0,
        );
        if (usage > 0) {
          const oldModel = irToForm(storedDef.skeleton.ir as PolicyIR);
          const changed =
            !oldModel || !prepared.model
              ? JSON.stringify(storedDef.skeleton.ir) !== JSON.stringify(prepared.ir)
              : structureKey(canonicalizeModel(oldModel)) !== structureKey(canonicalizeModel(prepared.model));
          if (changed) {
            window.alert(t("detail.structureLockedUsage", { count: usage }));
            throw new Error(t("detail.saveCancelled"));
          }
        }
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

  // 범위 모달 confirm → (필요시 패키지/폴더 생성) → put-def + bind.
  const finishMut = useMutation({
    mutationFn: async (choice: SaveScopeChoice): Promise<string> => {
      if (!scopeAsk) throw new Error(t("detail.internalNoPrepared"));

      // 지갑 전용 경로(모델 A): 지갑마다 **독립 def 사본**을 만들어 그 지갑의
      // 전용 폴더에 앵커한다. 바인딩(적용)은 만들지 않는다 — 적용은 지갑별
      // 정책에서 패키지에 끌어다 놓는 동선. {newName} 폴더는 find-or-create.
      if (choice.scope.kind === "wallets") {
        let lastId = "";
        for (const address of choice.scope.addresses) {
          const addr = address.toLowerCase();
          const pick = choice.walletFolders?.[address] ?? { id: WALLET_FOLDER_UNCAT };
          let folderId: string | undefined;
          if ("newName" in pick) {
            const existing = Object.values(
              snap?.wallets.byAddress[addr]?.folders ?? {},
            ).find((f) => f.displayName === pick.newName);
            if (existing) {
              folderId = existing.id;
            } else {
              folderId = `fold::${crypto.randomUUID()}`;
              await putWalletFolder({
                address: addr,
                folder: { id: folderId, displayName: pick.newName },
              });
            }
          } else if (pick.id !== WALLET_FOLDER_UNCAT) {
            folderId = pick.id;
          }
          const { def } = buildDefPayload({
            existing: null,
            displayName: choice.name || name.trim() || "untitled",
            cat: policy.cat,
            ir: scopeAsk.ir,
            manifest: scopeAsk.manifest,
            scope: choice.scope,
            packageId: null,
            applyToNewWallets: false,
            walletOnly: { homeWallet: addr, ...(folderId ? { walletFolderId: folderId } : {}) },
          });
          await putDef(def);
          lastId = def.id;
        }
        return lastId;
      }

      // 라이브러리 경로.
      let pkgId = choice.packageId;
      if (pkgId === "__new__") {
        pkgId = `pkg::${crypto.randomUUID()}`;
        await putPackage({
          id: pkgId,
          displayName: choice.newPackageName ?? t("list.newFolderName"),
          source: "mine",
          updatedAtMs: Date.now(),
        });
      }
      const { def, bindPlan } = buildDefPayload({
        existing: null,
        displayName: choice.name || name.trim() || "untitled",
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

  // 범위 모달의 지갑 목록: 서버 지갑 ∪ ps2 지갑(소문자) + 각 지갑의 패키지.
  const walletsQ = useQuery({ queryKey: ["wallets"], queryFn: listWallets, enabled: isNew });
  const modalWallets = useMemo(() => {
    const addrs = new Set([
      ...(walletsQ.data ?? []).map((w) => w.address.toLowerCase()),
      ...Object.keys(snap?.wallets.byAddress ?? {}),
    ]);
    return [...addrs].sort().map((address) => ({
      address,
      folders: Object.values(snap?.wallets.byAddress[address]?.folders ?? {})
        .map((f) => ({ id: f.id, displayName: f.displayName }))
        .sort((a, b) => a.displayName.localeCompare(b.displayName, "ko")),
    }));
  }, [walletsQ.data, snap]);
  const modalPackages = useMemo(
    () => Object.values(snap?.library.packages ?? {}),
    [snap],
  );

  const publishSource: PublishSource = {
    kind: "policy",
    cedarText,
    // Prefer the manifest computed when "마켓에 올리기" was clicked (covers an
    // unsaved policy); fall back to the persisted def manifest otherwise.
    manifest: publishManifest ? publishManifest.value : policy.manifest,
    policyTree: null,
    suggestedDisplayName: policy.displayName,
    suggestedSlug: stripDashboardId(policy.id),
  };

  /** "마켓에 올리기": generate the manifest from the CURRENT editor state
   *  (same path as save's `prepare()`), so an unsaved policy still publishes a
   *  valid manifest. Validation errors block the publish with a message. */
  const openPublish = async () => {
    try {
      const { manifest } = await prepare();
      setPublishManifest({ value: manifest });
      setPublishOpen(true);
    } catch (err) {
      window.alert(err instanceof Error ? err.message : String(err));
    }
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
      console.warn("[editor] 폼 열기 실패 — Cedar 탭으로 안내:", err); // i18n-ok
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
            placeholder={t("detail.namePlaceholder")}
          />
          <span className="ev2-detail-slug">{stripDashboardId(policy.id)}</span>
          {/* 폼 탭은 ③ 심각도가 이 값을 소유(onChange로 동기화)하므로 헤더
              셀렉트는 Cedar 탭에서만 — 같은 값이 두 군데면 헷갈린다. 바인딩
              모드의 Cedar 탭은 읽기 전용이라 여기서도 숨긴다. */}
          {tab !== "form" && !bindingCtx && (
            <select
              value={severity}
              onChange={(e) => setSeverity(e.target.value as PolicySeverity)}
              className="ev2-detail-sev"
            >
              <option value="deny">{t("detail.sevDeny")}</option>
              <option value="warn">{t("detail.sevWarn")}</option>
              <option value="info">{t("detail.sevInfo")}</option>
            </select>
          )}
          {policy.cat && (
            <span className="ev2-cat-tag" style={cstyle.tag}>
              {catLabel(policy.cat)}
            </span>
          )}
        </div>

        <div className="ev2-detail-meta">
          {isNew && <span className="ev2-badge-draft">{t("detail.draftBadge")}</span>}
          {bindingCtx && (
            <span className="ev2-badge-draft">
              {t("detail.bindingBadge", {
                addr: `${bindingCtx.address.slice(0, 6)}…${bindingCtx.address.slice(-4)}`,
              })}
            </span>
          )}
          {fromMarket && (
            <span className="ev2-detail-prov">
              <ShieldIcon />
              {t("detail.fromMarket")}
              {policy.sourceVersion ? ` · ${policy.sourceVersion}` : ""}
            </span>
          )}
        </div>

        <div className="ev2-detail-tabs" role="tablist">
          <TabBtn
            label="Cedar"
            active={tab === "cedar"}
            badge={bindingCtx ? t("detail.readOnlyBadge") : undefined}
            onClick={() => handleTabChange("cedar")}
          />
          <TabBtn
            label={t("detail.formTab")}
            active={tab === "form"}
            onClick={() => handleTabChange("form")}
          />
          <span className="ev2-spc" />
          {!bindingCtx && (
            <button
              type="button"
              className="ev2-pri ghost"
              onClick={openPublish}
              title={t("publish.title")}
            >
              <ShieldIcon /> {t("publish.title")}
            </button>
          )}
          {!bindingCtx && (
          <button
            type="button"
            className="ev2-pri danger"
            onClick={() => {
              const msg =
                usageCount > 0
                  ? t("detail.deleteConfirmUsed", { name, count: usageCount })
                  : t("detail.deleteConfirm", { name });
              if (!confirm(msg)) return;
              deleteMut.mutate();
            }}
            disabled={deleteMut.isPending}
          >
            {t("common:delete")}
          </button>
          )}
          <button
            type="button"
            className={`ev2-pri${bindingCtx && !formValidity.valid ? " invalid" : ""}`}
            title={bindingCtx && !formValidity.valid ? t("detail.invalidSaveTitle") : undefined}
            onClick={() => {
              // 값 시트에서 형식이 안 맞으면: 저장하지 않고 안내 + 변경 전으로 복원.
              if (bindingCtx && !formValidity.valid) {
                setRevertNotice(
                  formValidity.error
                    ? t("detail.revertNoticeWith", { error: formValidity.error })
                    : t("detail.revertNotice"),
                );
                setResetToken((tok) => tok + 1);
                return;
              }
              setRevertNotice(null);
              saveMut.mutate();
            }}
            disabled={saveMut.isPending || !cedarText.trim()}
          >
            {saveMut.isPending ? t("saving") : t("common:save")}
          </button>
        </div>
      </div>

      {(saveMut.error || finishMut.error || deleteMut.error) && (
        <div className="ev2-err-banner">
          <WarnIcon />
          {String(saveMut.error || finishMut.error || deleteMut.error || "")}
        </div>
      )}
      {revertNotice && (
        <div className="ev2-err-banner warn">
          <WarnIcon />
          {revertNotice}
        </div>
      )}

      <div className="ev2-detail-tabbody">
        {tab === "cedar" && (
          <CedarPane
            value={cedarText}
            readOnly={!!bindingCtx}
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
              valuesOnly={!!bindingCtx}
              onValidity={setFormValidity}
              resetToken={resetToken}
              onChange={({ cedarText: c, ir: nextIr, model, manifest, manifestOverridden }) => {
                setCedarText(c);
                setIr(nextIr);
                setLastModel(model);
                // Keep the header severity in sync so save stamps it correctly.
                setSeverity(model.severity as PolicySeverity);
                // Carry the form's manifest override (if any) so save persists it
                // instead of re-generating.
                setManifestOverride(manifestOverridden ? { value: manifest } : null);
              }}
            />
          ) : formEntry?.kind === "closed" ? (
            <div className="ev2-empty">
              <div className="big">{t("detail.formClosedTitle")}</div>
              <div className="sm">{t("detail.formClosedHint")}</div>
              <div className="ev2-empty-actions">
                <button type="button" className="ev2-pri ghost" onClick={() => handleTabChange("cedar")}>
                  {t("detail.toCedarTab")}
                </button>
              </div>
            </div>
          ) : (
            <div className="ev2-empty">
              <div className="sm">{t("detail.formLoading")}</div>
            </div>
          ))}
      </div>

      <PublishModal
        open={publishOpen}
        source={publishSource}
        onClose={() => {
          setPublishOpen(false);
          setPublishManifest(null); // next publish recomputes from current state
        }}
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
  badge?: string | undefined;
  tooltip?: string;
  onClick: () => void;
}) {
  const { t } = useTranslation("editor");
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
      {props.disabled && <span className="ev2-tab-soon">{t("chooser.soon")}</span>}
      {!props.disabled && props.badge && <span className="ev2-tab-soon">{props.badge}</span>}
    </button>
  );
}

function CedarPane({
  value,
  readOnly = false,
  onChange,
}: {
  value: string;
  readOnly?: boolean;
  onChange: (next: string) => void;
}) {
  const { t } = useTranslation("editor");
  return (
    <div className="ev2-cedar-pane">
      <div className="ev2-cedar-toolbar">
        <span className="ev2-cedar-hint">
          {readOnly ? (
            t("detail.cedarReadOnlyHint")
          ) : (
            <Trans t={t} i18nKey="detail.cedarHint" components={{ c: <code /> }} />
          )}
        </span>
      </div>
      <textarea
        className="ev2-cedar-textarea"
        value={value}
        readOnly={readOnly}
        onChange={(e) => {
          if (!readOnly) onChange(e.target.value);
        }}
        spellCheck={false}
        autoCorrect="off"
        autoCapitalize="off"
      />
    </div>
  );
}
