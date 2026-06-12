/**
 * 읽기 전용 조건 트리 — 폼 에디터와 같은 파이프라인(textToBlocks → irToForm)
 * 으로 만든 트리를 렌더한다. 두 소비자:
 *
 *   - {@link PublishPreviewTree} (게시 모달): 비식별 칸(extractHoles)을
 *     배지로 표시, 클릭 = 비우기/공개 토글(Step1 행 토글과 같은 kept 상태).
 *   - {@link ListingConditionTree} (마켓 상세): manifest `x_pasu_holes`의
 *     빈칸을 "설치할 때 채워요" 배지로 표시 — Cedar 원문의 제로주소/0
 *     플레이스홀더만 봐서는 뭐가 빈칸인지 알 수 없는 문제의 해법.
 *
 * 폼 비호환 정책(irToForm 실패)은 안내 문구로 폴백.
 */
import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { i18n } from "../../i18n";
import { textToBlocks } from "../../cedar";
import {
  irToForm,
  isGroupNode,
  KNOWN_ACTIONS,
  type FormCondition,
  type FormModel,
  type FormNode,
  type FormOp,
  type FormValue,
} from "../../cedar/form";
import { canonicalizeModel, parameterizeModel } from "../../cedar/form/parameterize";
import { getGloss } from "../../editor-v9/gloss/paths";
import { splitManifestHoles } from "./publish-holes";
import type { PublishHole } from "./publish-redact";

/** 연산자 칩 — 기호는 언어 공통, 포함류만 번역. 호출 시점 t() (모듈 평가 금지). */
function opLabel(op: FormOp): string {
  switch (op) {
    case "==":
      return "=";
    case "!=":
      return "≠";
    case "<":
      return "<";
    case "<=":
      return "≤";
    case ">":
      return ">";
    case ">=":
      return "≥";
    case "contains":
      return i18n.t("editor:op.contains");
    case "notContains":
      return i18n.t("editor:op.notContains");
    case "in":
      return "∈";
    case "notIn":
      return "∉";
  }
}

function kindMatches(value: FormValue, kind: PublishHole["kind"]): boolean {
  if (kind === "address") {
    return (
      value.kind === "set" ||
      (value.kind === "string" && /^0x[0-9a-fA-F]{40}$/.test(value.value))
    );
  }
  return value.kind === "long" || value.kind === "decimal";
}

/**
 * 노드 순회 순서대로, 같은 fieldPath + 맞는 값 종류의 첫 미배정 hole을 leaf에
 * 배정한다 (computeShippedHoles의 claimed 패턴과 같은 사상 — 같은 path의
 * hole 여러 개가 출현 순서대로 서로 다른 leaf에 붙는다).
 */
export function holeAssignments(
  model: FormModel,
  holes: PublishHole[],
): Map<FormCondition, PublishHole> {
  const out = new Map<FormCondition, PublishHole>();
  const claimed = new Set<string>();
  const visit = (nodes: FormNode[]) => {
    for (const n of nodes) {
      if (isGroupNode(n)) {
        visit(n.conds);
        continue;
      }
      const h = holes.find(
        (x) => !claimed.has(x.key) && x.path === n.fieldPath && kindMatches(n.value, x.kind),
      );
      if (h) {
        claimed.add(h.key);
        out.set(n, h);
      }
    }
  };
  visit(model.when);
  visit(model.unless);
  return out;
}

function fieldLabel(path: string): string {
  const g = getGloss(path);
  if (g) return i18n.language?.startsWith("en") ? g.en : g.ko;
  return path.split(".").pop() ?? path;
}

function shortAddr(a: string): string {
  return `${a.slice(0, 6)}…${a.slice(-4)}`;
}

function valueText(v: FormValue): string {
  switch (v.kind) {
    case "bool":
      return v.value ? i18n.t("editor:value.true") : i18n.t("editor:value.false");
    case "long":
      return String(v.value);
    case "decimal":
      return v.value;
    case "string":
      return /^0x[0-9a-fA-F]{40}$/.test(v.value) ? shortAddr(v.value) : v.value;
    case "set":
      return v.values.map((x) => (/^0x[0-9a-fA-F]{40}$/.test(x) ? shortAddr(x) : x)).join(", ");
    case "field":
      return fieldLabel(v.path);
  }
}

function triggerText(model: FormModel): string {
  if (model.trigger.kind === "any") return i18n.t("editor:tree.anyAction");
  const t = model.trigger;
  const known = KNOWN_ACTIONS.find((a) => a.entityType === t.entityType && a.id === t.id);
  return known?.label ?? t.id;
}

/** cedar 텍스트 → FormModel 비동기 로드 (실패 = null = 폼 비호환). */
function useFormModel(
  cedarText: string,
  transform?: (m: FormModel) => FormModel,
): FormModel | null | "loading" {
  const [model, setModel] = useState<FormModel | null | "loading">("loading");
  useEffect(() => {
    let alive = true;
    setModel("loading");
    textToBlocks(cedarText)
      .then((irs) => {
        if (!alive) return;
        const ir = irs[0];
        const m = ir ? irToForm(ir) : null;
        setModel(m && transform ? transform(m) : m);
      })
      .catch(() => alive && setModel(null));
    return () => {
      alive = false;
    };
    // transform은 모듈 수준 순수 함수만 받는다 — 의존성에서 제외.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cedarText]);
  return model;
}

/** 트리거 + when/unless(그룹 재귀)를 렌더하는 공용 읽기 전용 트리.
 *  값 칸의 표현(배지/토글)은 renderValue 콜백이 결정한다. */
export function ConditionTree(props: {
  model: FormModel;
  renderValue: (leaf: FormCondition) => ReactNode;
}) {
  const { model, renderValue } = props;
  const { t } = useTranslation("editor");
  const joiner = (j: "and" | "or") => (j === "or" ? t("or") : t("and"));
  const renderNodes = (nodes: FormNode[]) =>
    nodes.map((n, i) =>
      isGroupNode(n) ? (
        <div key={i} className="pub-tree-grouprow">
          {i > 0 && <span className="pub-tree-joiner">{joiner(n.joiner)}</span>}
          <div className="pub-tree-group">{renderNodes(n.conds)}</div>
        </div>
      ) : (
        <div key={i} className="pub-tree-row">
          {i > 0 && <span className="pub-tree-joiner">{joiner(n.joiner)}</span>}
          <span className="pub-tree-field">
            {fieldLabel(n.fieldPath)} <code>{n.fieldPath}</code>
          </span>
          <span className="pub-tree-op">{opLabel(n.op)}</span>
          {renderValue(n)}
        </div>
      ),
    );

  return (
    <div className="pub-tree">
      <div className="pub-tree-sec">
        <span className="pub-tree-sech">{t("tree.target")}</span>
        <span className="pub-tree-trigger">{triggerText(model)}</span>
      </div>
      {model.when.length > 0 && (
        <div className="pub-tree-sec">
          <span className="pub-tree-sech">{t("tree.conditions")}</span>
          <div className="pub-tree-list">{renderNodes(model.when)}</div>
        </div>
      )}
      {model.unless.length > 0 && (
        <div className="pub-tree-sec">
          <span className="pub-tree-sech">{t("tree.unless")}</span>
          <div className="pub-tree-list">{renderNodes(model.unless)}</div>
        </div>
      )}
    </div>
  );
}

/** 게시 모달용: 비식별 칸 배지(비워짐↔공개 토글). */
export function PublishPreviewTree(props: {
  cedarText: string;
  holes: PublishHole[];
  kept: Set<string>;
  onToggleKeep: (key: string) => void;
}) {
  const { cedarText, holes, kept, onToggleKeep } = props;
  const { t } = useTranslation("editor");
  const model = useFormModel(cedarText);

  if (model === "loading")
    return <div className="pub-tree-muted">{t("tree.loadingConditions")}</div>;
  if (!model) {
    return <div className="pub-tree-muted">{t("tree.noFormPreview")}</div>;
  }

  const assigned = holeAssignments(model, holes);

  const renderValue = (leaf: FormCondition) => {
    const h = assigned.get(leaf);
    if (!h) return <span className="pub-tree-val">{valueText(leaf.value)}</span>;
    const isKept = kept.has(h.key);
    return (
      <button
        type="button"
        className={`pub-tree-hole${isKept ? " public" : ""}`}
        title={
          isKept
            ? t("tree.keptTitle")
            : t("tree.blankedTitle", { param: h.paramName })
        }
        onClick={() => onToggleKeep(h.key)}
      >
        <span className={isKept ? "" : "strike"}>{valueText(leaf.value)}</span>
        <span className="tag">{isKept ? t("tree.publicTag") : h.paramName}</span>
      </button>
    );
  };

  return <ConditionTree model={model} renderValue={renderValue} />;
}

/** parameterize 파이프라인은 모듈 수준 순수 함수 — useFormModel transform 용. */
function toParameterized(m: FormModel): FormModel {
  return parameterizeModel(canonicalizeModel(m));
}

/**
 * 마켓 상세용: 설치자가 채울 빈칸(manifest `x_pasu_holes`)을 트리 위에 표시.
 * 게시·설치와 같은 위치 기반 param 이름(v1..vN) 파이프라인으로 leaf를 찾으므로
 * 번호가 일치한다. Cedar의 제로주소/0 플레이스홀더 대신 "설치할 때 채워요"로.
 */
export function ListingConditionTree(props: { cedarText: string; manifest?: unknown }) {
  const { cedarText, manifest } = props;
  const { t } = useTranslation("editor");
  const model = useFormModel(cedarText, toParameterized);
  const shipped = splitManifestHoles(manifest).shipped;
  const byName = new Map(shipped.map((s) => [s.name, s]));

  if (model === "loading")
    return <div className="pub-tree-muted">{t("tree.loadingConditions")}</div>;
  if (!model) {
    return <div className="pub-tree-muted">{t("tree.noConditionView")}</div>;
  }

  const renderValue = (leaf: FormCondition) => {
    const spec = leaf.param ? byName.get(leaf.param.name) : undefined;
    if (spec) {
      return (
        <span className="pub-tree-hole asis" title={t("tree.holeTitle")}>
          <span className="tag">{t("tree.holeTag")}</span>
          {t("tree.fillAtInstall", { label: spec.label })}
        </span>
      );
    }
    return <span className="pub-tree-val">{valueText(leaf.value)}</span>;
  };

  return <ConditionTree model={model} renderValue={renderValue} />;
}
