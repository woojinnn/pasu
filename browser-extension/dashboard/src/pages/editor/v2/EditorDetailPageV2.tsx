import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useLocation, useNavigate, useParams } from "react-router-dom";

import {
  deleteManagedPolicy,
  getEnabledPolicyIds,
  listManagedPolicies,
  putPolicy,
  setEnabledPolicyIds,
  stripDashboardId,
  type ManagedPolicy,
  type PolicyMethod,
} from "../../../server-api";
import type { PolicySeverity } from "../../../server-api";
import { Topbar } from "../../../shell/Topbar";

import { stampAnnotations } from "../../../editor-v9/annotations";
import { WorkspaceV9 } from "../../../editor-v9/Workspace";
import { generateManifest } from "../../../editor-v9/manifest-gen";
import type { PolicyIR } from "../../../cedar/blocks";

import { nameFromPolicy, severityFromCedar } from "../policy-meta";
import { PublishModal, type PublishSource } from "../PublishModal";
// PublishModal classes (.publish-modal, .publish-modal-backdrop) are
// authored in market.css; pull it in so the modal renders with a solid
// background when launched from the v2 editor.
import "../../market.css";

import { catLabel, catStyle } from "./categories";
import { CatIcon, ShieldIcon, WarnIcon } from "./icons";
import { isMarketSource } from "./helpers";
import { textToBlocks } from "../../../cedar";
import { PolicyFormPane } from "./PolicyFormPane";
import { emptyFormModel, irToForm, type FormModel } from "../../../cedar/form";

type Tab = "cedar" | "form" | "block";

function defaultTab(method: PolicyMethod | undefined): Tab {
  if (method === "block") return "block";
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

/**
 * Phase 3 detail view — mypolicy-editor.jsx ported to the SPA.
 * Layout: header (title + severity + memo + tabs) → body (active tab) →
 * collapsible manifest panel. Form tab is intentionally disabled —
 * surfaced behind a tooltip so the design intent stays visible.
 */
/** Seed handed in by {@link NewPolicyChooser} via navigation state. Nothing is
 *  persisted until the user saves, so an abandoned new policy never exists. */
interface NewPolicySeed {
  method: PolicyMethod;
  cedarText: string;
  displayName: string;
}

export function EditorDetailPageV2() {
  const navigate = useNavigate();
  const location = useLocation();
  const params = useParams<{ id: string }>();
  const id = params.id ? decodeURIComponent(params.id) : "";
  const qc = useQueryClient();

  const listQ = useQuery({
    queryKey: ["managed-policies"],
    queryFn: listManagedPolicies,
  });
  const stored = useMemo(
    () => listQ.data?.find((p) => p.id === id) ?? null,
    [listQ.data, id],
  );

  // A fresh policy carried in via navigation state — synthesize an in-memory
  // ManagedPolicy so the editor renders before anything is written to storage.
  const seed = (location.state as { newPolicy?: NewPolicySeed } | null)?.newPolicy;
  const isNew = !stored && !!seed;
  const draftPolicy = useMemo<ManagedPolicy | null>(() => {
    if (!seed) return null;
    return {
      id,
      kind: "raw",
      text: seed.cedarText,
      displayName: seed.displayName,
      method: seed.method,
      source: "mine",
      life: "publish",
      updatedAtMs: Date.now(),
      schemaVersion: 1,
    };
  }, [seed, id]);

  const policy = stored ?? (isNew ? draftPolicy : null);

  return (
    <>
      <Topbar
        here="Policy Editor"
        subtitle={policy ? nameFromPolicy(policy) : id || "…"}
        right={
          <Link to="/editor" className="ev2-back">
            ← 목록
          </Link>
        }
      />
      <div className="ev2-detail-body">
        {listQ.isLoading && !policy && (
          <div className="ev2-status">불러오는 중…</div>
        )}
        {!listQ.isLoading && !policy && (
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
            isNew={isNew}
            onSaved={(savedId) => {
              if (savedId !== id) {
                navigate(`/editor/${encodeURIComponent(savedId)}`, {
                  replace: true,
                });
              } else if (isNew) {
                // Drop the navigation seed so a reload doesn't re-enter new mode.
                navigate(`/editor/${encodeURIComponent(id)}`, { replace: true });
              }
              void qc.invalidateQueries({ queryKey: ["managed-policies"] });
            }}
            onDeleted={() => navigate("/editor")}
          />
        )}
      </div>
    </>
  );
}

function EditorBody({
  policy,
  isNew,
  onSaved,
  onDeleted,
}: {
  policy: ManagedPolicy;
  isNew: boolean;
  onSaved: (id: string) => void;
  onDeleted: () => void;
}) {
  const qc = useQueryClient();

  const [name, setName] = useState(() => nameFromPolicy(policy));
  const [severity, setSeverity] = useState<PolicySeverity>(() =>
    severityFromCedar(policy.text),
  );
  const [cedarText, setCedarText] = useState(policy.text);
  // For `cedar`-method policies the cedar text is canonical, so we
  // ignore any persisted block snapshot on mount — it can be stale
  // relative to user edits made before this commit landed. The Block
  // tab always re-parses from `cedarText` on its first visit. For
  // `block`-method (and legacy `undefined`) policies we keep the
  // snapshot so previously-arranged blocks reappear.
  const [treeJson, setTreeJson] = useState<string | null>(
    policy.method === "cedar" ? null : (policy.policyTree ?? null),
  );
  // Memo is no longer edited in the UI (the form's 사유 covers it); preserve any
  // existing value so saving doesn't wipe it.
  const memo = policy.memo ?? "";
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

  /** Force a Workspace remount when seeding swaps (e.g. user typed in
   *  the Cedar tab, then switched to Block — Blockly needs to re-parse). */
  const [workspaceKey, setWorkspaceKey] = useState(0);
  const lastBlockSnapshot = useRef<string>(policy.text);

  // Reseed when the parent swaps to a different policy id.
  useEffect(() => {
    setName(nameFromPolicy(policy));
    setSeverity(severityFromCedar(policy.text));
    setCedarText(policy.text);
    setTreeJson(
      policy.method === "cedar" ? null : (policy.policyTree ?? null),
    );
    setTab(defaultTab(policy.method));
    setManifestOverride(null);
    setFormEntry(null);
    lastBlockSnapshot.current = policy.text;
    setWorkspaceKey((k) => k + 1);
  }, [policy.id]);

  const fromMarket = isMarketSource(policy);
  const cstyle = catStyle(policy.cat);

  const saveMut = useMutation({
    mutationFn: async () => {
      const stamped = stampAnnotations(
        cedarText,
        name.trim() || "untitled",
        severity,
      );
      // The manifest is generated from the policy IR. The Block tab keeps `ir`
      // live, but the Cedar tab edits text directly — so when `ir` is null we
      // parse the text here. Otherwise a `context.custom.*` policy authored in
      // the Cedar tab saves WITHOUT a manifest, the enrichment is never planned,
      // its `has` guards short-circuit to false, and the policy silently never
      // fires.
      let effectiveIr = ir;
      if (!effectiveIr && cedarText.trim()) {
        try {
          effectiveIr = (await textToBlocks(cedarText))[0] ?? null;
        } catch {
          effectiveIr = null; // unparseable in-progress text → save w/o manifest
        }
      }
      let manifest: unknown;
      if (tab === "form" && manifestOverride) {
        // The form supplied a hand-edited manifest — persist it as-is.
        manifest = manifestOverride.value;
      } else if (effectiveIr) {
        const gen = generateManifest(effectiveIr, undefined, {
          id: policy.id,
          severity,
        });
        if (gen.errors.length > 0) {
          throw new Error(gen.errors.map((e) => e.message).join("\n"));
        }
        manifest = gen.manifest;
      }
      await putPolicy({
        id: policy.id,
        cedarText: stamped,
        policyTree: treeJson,
        displayName: name.trim() || "untitled",
        memo,
        method: policy.method,
        // There is no draft lifecycle: every save makes the policy live.
        life: "publish",
        source: policy.source,
        cat: policy.cat,
        dupKey: policy.dupKey,
        sourceListingId: policy.sourceListingId,
        sourceVersion: policy.sourceVersion,
        ...(manifest !== undefined ? { manifest } : {}),
      });
      // A freshly-created policy should be active the moment it's saved
      // ("저장 = 활성"). Add it to the enabled set (idempotent for re-saves).
      if (isNew) {
        try {
          const enabled = await getEnabledPolicyIds();
          if (!enabled.includes(policy.id)) {
            await setEnabledPolicyIds([...enabled, policy.id]);
          }
        } catch {
          // Non-fatal: the policy is saved; the user can toggle it on manually.
        }
      }
      return policy.id;
    },
    onSuccess: (id) => {
      void qc.invalidateQueries({ queryKey: ["managed-policies"] });
      void qc.invalidateQueries({ queryKey: ["enabled-policy-ids"] });
      onSaved(id);
    },
  });

  const deleteMut = useMutation({
    mutationFn: async () => deleteManagedPolicy(policy.id),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["managed-policies"] });
      onDeleted();
    },
  });

  const publishSource: PublishSource = {
    kind: "policy",
    cedarText,
    manifest: policy.manifest,
    policyTree: treeJson,
    suggestedDisplayName: nameFromPolicy(policy),
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
    // When switching to Block tab from Cedar where the user may have
    // typed by hand, force a Workspace remount so Blockly re-parses
    // the latest cedar text rather than keeping its stale AST.
    if (next === "block" && cedarText !== lastBlockSnapshot.current) {
      setWorkspaceKey((k) => k + 1);
    }
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
          <select
            value={severity}
            onChange={(e) => setSeverity(e.target.value as PolicySeverity)}
            className="ev2-detail-sev"
          >
            <option value="deny">deny (차단)</option>
            <option value="warn">warn (경고)</option>
            <option value="info">info (정보)</option>
          </select>
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
          <TabBtn
            label="블록"
            active={tab === "block"}
            onClick={() => handleTabChange("block")}
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
              if (!confirm(`정책 "${name}"을 삭제할까요?`)) return;
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

      {(saveMut.error || deleteMut.error) && (
        <div className="ev2-err-banner">
          <WarnIcon />
          {String(saveMut.error || deleteMut.error || "")}
        </div>
      )}

      <div className="ev2-detail-tabbody">
        {tab === "cedar" && (
          <CedarPane
            value={cedarText}
            onChange={(next) => {
              setCedarText(next);
              // Cedar edits invalidate the Blockly snapshot — otherwise
              // a subsequent Block-tab visit would re-seed from the
              // stale `policyTree` (Workspace prefers initialJson over
              // initialCedarText), silently dropping the new cedar.
              setTreeJson(null);
              // Drop the cached IR too. Otherwise the form tab (openForm) and
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
              onChange={({ cedarText: c, ir: nextIr, model, manifest, manifestOverridden }) => {
                setCedarText(c);
                setIr(nextIr);
                // Keep the header severity in sync so save stamps it correctly.
                setSeverity(model.severity as PolicySeverity);
                // The form doesn't produce a Blockly tree; drop the snapshot so
                // a later Block-tab visit re-parses from the new cedar.
                setTreeJson(null);
                lastBlockSnapshot.current = c;
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
                정책은 Cedar 또는 블록 탭에서 편집해 주세요.
              </div>
              <div className="ev2-empty-actions">
                <button type="button" className="ev2-pri ghost" onClick={() => handleTabChange("cedar")}>
                  Cedar 탭으로
                </button>
                <button type="button" className="ev2-pri ghost" onClick={() => handleTabChange("block")}>
                  블록 탭으로
                </button>
              </div>
            </div>
          ) : (
            <div className="ev2-empty">
              <div className="sm">폼을 불러오는 중…</div>
            </div>
          ))}
        {tab === "block" && (
          <WorkspaceV9
            key={workspaceKey}
            policyName={name.trim() || "untitled"}
            initialJson={tryParseV9Json(treeJson)}
            initialCedarText={cedarText}
            hideImport
            hidePreview
            onChange={({ cedarText: c, json, ir: nextIr }) => {
              setCedarText(c);
              setTreeJson(JSON.stringify({ v: 9, ws: json }));
              setIr(nextIr ?? null);
              lastBlockSnapshot.current = c;
            }}
          />
        )}
      </div>

      <PublishModal
        open={publishOpen}
        source={publishSource}
        onClose={() => setPublishOpen(false)}
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

function tryParseV9Json(s: string | null): object | null {
  if (!s) return null;
  try {
    const obj = JSON.parse(s);
    if (obj && typeof obj === "object" && (obj as { v?: number }).v === 9) {
      return (obj as { ws: object }).ws;
    }
    return null;
  } catch {
    return null;
  }
}
