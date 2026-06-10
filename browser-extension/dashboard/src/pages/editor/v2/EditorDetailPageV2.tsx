import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useLocation, useNavigate, useParams } from "react-router-dom";

import { stripDashboardId, type PolicyMethod } from "../../../server-api";
import type { PolicySeverity } from "../../../server-api";
import {
  bindDef,
  deleteDef,
  getOverview,
  putDef,
  putPackage,
  type PolicyDef,
  type StoreSnapshot,
} from "../../../server-api/policy-store";
import { listWallets } from "../../../server-api/wallets";
import { buildDefPayload } from "./save-def";
import { SaveScopeModal, type SaveScopeChoice } from "./SaveScopeModal";
import { defUsageCount } from "./wallet-policies-derive";
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

  const overviewQ = useQuery({ queryKey: ["ps2-overview"], queryFn: getOverview });
  const storedDef = overviewQ.data?.library.defs[id] ?? null;

  // def 뼈대(BlockIR)는 텍스트가 아니므로 Cedar 탭용 텍스트는 비동기로 렌더한다.
  const textQ = useQuery({
    queryKey: ["ps2-def-text", id, storedDef?.updatedAtMs ?? 0],
    enabled: !!storedDef,
    queryFn: () => blocksToText(storedDef!.skeleton.ir as PolicyIR),
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
        displayName: storedDef.displayName,
        text: textQ.data,
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
  }, [storedDef, textQ.data, seed, id]);

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
            key={policy.id}
            policy={policy}
            storedDef={storedDef}
            snap={overviewQ.data ?? null}
            isNew={isNew}
            onSaved={(savedId) => {
              void qc.invalidateQueries({ queryKey: ["ps2-overview"] });
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
  isNew,
  onSaved,
  onDeleted,
}: {
  policy: EditorPolicy;
  storedDef: PolicyDef | null;
  snap: StoreSnapshot | null;
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
      const gen = generateManifest(effectiveIr, undefined, { id: policy.id, severity });
      if (gen.errors.length > 0) {
        throw new Error(gen.errors.map((e) => e.message).join("\n"));
      }
      manifest = gen.manifest;
    }
    return { ir: effectiveIr, manifest };
  };

  const saveMut = useMutation({
    mutationFn: async (): Promise<string | null> => {
      const prepared = await prepare();
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
      let effectiveIr = ir;
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
    } catch {
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
            onClick={() => handleTabChange("cedar")}
          />
          <TabBtn
            label="폼"
            active={tab === "form"}
            onClick={() => handleTabChange("form")}
          />
          <span className="ev2-spc" />
          <button
            type="button"
            className="ev2-pri ghost"
            onClick={() => setPublishOpen(true)}
            title="마켓에 올리기"
          >
            <ShieldIcon /> 마켓에 올리기
          </button>
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
