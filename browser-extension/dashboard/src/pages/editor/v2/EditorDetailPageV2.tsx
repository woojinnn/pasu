import { useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router-dom";

import {
  deleteManagedPolicy,
  listManagedPolicies,
  putPolicy,
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
import { CatIcon, PencilIcon, ShieldIcon, WarnIcon } from "./icons";
import { isDraft, isMarketSource } from "./helpers";
import { PolicyDiagnosis } from "../../../cedar/diagram/PolicyDiagnosis";
import { textToBlocks } from "../../../cedar";

type Tab = "cedar" | "form" | "block" | "diagram";

function defaultTab(method: PolicyMethod | undefined): Tab {
  if (method === "block") return "block";
  return "cedar";
}

/**
 * Phase 3 detail view — mypolicy-editor.jsx ported to the SPA.
 * Layout: header (title + severity + memo + tabs) → body (active tab) →
 * collapsible manifest panel. Form tab is intentionally disabled —
 * surfaced behind a tooltip so the design intent stays visible.
 */
export function EditorDetailPageV2() {
  const navigate = useNavigate();
  const params = useParams<{ id: string }>();
  const id = params.id ? decodeURIComponent(params.id) : "";
  const qc = useQueryClient();

  const listQ = useQuery({
    queryKey: ["managed-policies"],
    queryFn: listManagedPolicies,
  });
  const policy = useMemo(
    () => listQ.data?.find((p) => p.id === id) ?? null,
    [listQ.data, id],
  );

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
        {listQ.isLoading && <div className="ev2-status">불러오는 중…</div>}
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
            policy={policy}
            onSaved={(savedId) => {
              if (savedId !== id) {
                navigate(`/editor/${encodeURIComponent(savedId)}`, {
                  replace: true,
                });
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
  onSaved,
  onDeleted,
}: {
  policy: ManagedPolicy;
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
  const [memo, setMemo] = useState(policy.memo ?? "");
  const [ir, setIr] = useState<PolicyIR | null>(null);
  const [tab, setTab] = useState<Tab>(() => defaultTab(policy.method));
  const [publishOpen, setPublishOpen] = useState(false);

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
    setMemo(policy.memo ?? "");
    setTab(defaultTab(policy.method));
    lastBlockSnapshot.current = policy.text;
    setWorkspaceKey((k) => k + 1);
  }, [policy.id]);

  const draft = isDraft(policy);
  const fromMarket = isMarketSource(policy);
  const cstyle = catStyle(policy.cat);

  const saveMut = useMutation({
    mutationFn: async () => {
      const stamped = stampAnnotations(
        cedarText,
        name.trim() || "untitled",
        severity,
      );
      let manifest: unknown;
      if (ir) {
        const gen = generateManifest(ir, undefined, {
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
        life: policy.life,
        source: policy.source,
        cat: policy.cat,
        dupKey: policy.dupKey,
        sourceListingId: policy.sourceListingId,
        sourceVersion: policy.sourceVersion,
        ...(manifest !== undefined ? { manifest } : {}),
      });
      return policy.id;
    },
    onSuccess: (id) => {
      void qc.invalidateQueries({ queryKey: ["managed-policies"] });
      onSaved(id);
    },
  });

  const publishDraftMut = useMutation({
    mutationFn: async () => {
      await putPolicy({
        id: policy.id,
        cedarText,
        policyTree: treeJson,
        displayName: name.trim() || "untitled",
        memo,
        method: policy.method,
        life: "publish",
        source: policy.source,
        cat: policy.cat,
        dupKey: policy.dupKey,
        sourceListingId: policy.sourceListingId,
        sourceVersion: policy.sourceVersion,
      });
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["managed-policies"] });
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

  const handleTabChange = (next: Tab) => {
    if (next === "form") return;
    if (next === tab) return;
    // When switching to Block tab from Cedar where the user may have
    // typed by hand, force a Workspace remount so Blockly re-parses
    // the latest cedar text rather than keeping its stale AST.
    if (next === "block" && cedarText !== lastBlockSnapshot.current) {
      setWorkspaceKey((k) => k + 1);
    }
    setTab(next);
  };

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
          {draft && (
            <span className="ev2-badge-draft">
              <PencilIcon /> 수정중 · 평가에서 자동 제외
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

        <div className="ev2-detail-memo-row">
          <label className="ev2-detail-memo-label">메모</label>
          <input
            className="ev2-detail-memo"
            value={memo}
            onChange={(e) => setMemo(e.target.value)}
            placeholder="이 정책에 대한 짧은 메모 (선택)"
          />
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
            disabled
            tooltip="준비 중 — Cedar/블록으로 편집하세요"
            onClick={() => handleTabChange("form")}
          />
          <TabBtn
            label="블록"
            active={tab === "block"}
            onClick={() => handleTabChange("block")}
          />
          <TabBtn
            label="다이어그램"
            active={tab === "diagram"}
            onClick={() => handleTabChange("diagram")}
          />
          <span className="ev2-spc" />
          {draft && (
            <button
              type="button"
              className="ev2-pri ghost"
              onClick={() => publishDraftMut.mutate()}
              disabled={publishDraftMut.isPending}
              title="draft 상태를 publish로 전환합니다"
            >
              {publishDraftMut.isPending ? "전환 중…" : "Publish 전환"}
            </button>
          )}
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

      {(saveMut.error || deleteMut.error || publishDraftMut.error) && (
        <div className="ev2-err-banner">
          <WarnIcon />
          {String(
            saveMut.error || deleteMut.error || publishDraftMut.error || "",
          )}
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
            }}
          />
        )}
        {tab === "form" && (
          <div className="ev2-empty">
            <div className="big">폼 모드는 준비 중입니다</div>
            <div className="sm">
              지금은 Cedar 탭 또는 블록 탭에서 편집해 주세요.
            </div>
          </div>
        )}
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
        {tab === "diagram" && <DiagramTab cedarText={cedarText} />}
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

/**
 * The 다이어그램 tab — a read-only UML-feel structure view of the policy.
 * Parses the live `cedarText` (the shared source both the Cedar and Block tabs
 * keep current) into a {@link PolicyIR} via the WASM bridge, then renders it.
 * Last good diagram is kept while a malformed in-progress edit can't parse.
 */
function DiagramTab({ cedarText }: { cedarText: string }) {
  const q = useQuery({
    queryKey: ["editor-diagram-ir", cedarText],
    queryFn: async () => {
      const text = cedarText.trim();
      if (!text) return null;
      const irs = await textToBlocks(text);
      return irs[0] ?? null;
    },
    placeholderData: (prev) => prev, // hold the last diagram across re-parses
    retry: false,
  });

  if (q.isError) {
    return (
      <div className="ev2-empty">
        <div className="big">아직 다이어그램을 그릴 수 없어요</div>
        <div className="sm">
          Cedar 또는 블록 탭에서 정책을 완성하면 구조가 표시됩니다.
        </div>
      </div>
    );
  }
  return (
    <div className="ev2-diagram-pane">
      <PolicyDiagnosis ir={q.data ?? null} />
    </div>
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
