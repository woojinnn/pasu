/**
 * CustomFieldModal — "+ 새 보강 필드 만들기".
 *
 * policy-server 메서드를 골라 파라미터를 매핑하면 `context.custom.<이름>`
 * 필드가 생긴다: 즉시 필드 선택기에 나타나고, 저장 시 manifest의
 * `policy_rpc` + `custom_context`로 직렬화된다.
 *
 * 파라미터 값은 셀렉터 문법(`$.root.chain_id`)을 직접 쓰게 하지 않는다 —
 * 폼의 한국어 필드 카탈로그를 재사용한 드롭다운("이 거래에서 가져오기")으로
 * 고르고, 셀렉터 원문/결과 위치는 "고급" 안에서만 보인다. `context.<X>` Cedar
 * 경로는 plan-time 셀렉터 `$.action.<X>`와 1:1 대응한다.
 */
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";

import type { EnrichmentField, EnrichmentRegistry, ParamSpec } from "../../../editor-v9/manifest-gen";
import { i18n } from "../../../i18n";
import type { FieldOption } from "../../../cedar/form";

import { METHOD_CATALOG, methodDesc, methodLabel, type MethodSpec } from "./custom-field-methods";

export interface CustomFieldDraft {
  name: string;
  field: EnrichmentField;
}

/** 내부 필드 이름 자동 생성: 메서드 꼬리를 camelCase로, 충돌 시 숫자 suffix.
 *  (`address.risk_score` → `riskScore`, 이미 있으면 `riskScore2`, …) */
function autoName(methodName: string, existing: readonly string[]): string {
  const tail = methodName.split(".").pop() ?? "value";
  const base = tail.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase());
  if (!existing.includes(base)) return base;
  for (let i = 2; ; i++) {
    const cand = `${base}${i}`;
    if (!existing.includes(cand)) return cand;
  }
}

/** 메서드 파라미터 키 → 라벨 i18n 키 (모르는 키는 키 그대로 표시). */
const PARAM_KEY_LABEL: Record<string, string> = {
  chain_id: "customField.paramKey.chainId",
  asset: "customField.paramKey.asset",
  amount: "customField.paramKey.amount",
  address: "customField.paramKey.address",
  wallet: "customField.paramKey.wallet",
  decimals: "customField.paramKey.decimals",
};

function paramKeyLabel(key: string, t: TFunction): string {
  const k = PARAM_KEY_LABEL[key];
  return k ? t(`editor:${k}`) : key;
}

/** 파라미터 키가 기대하는 값의 종류 — 드롭다운을 이 종류로만 거른다. */
type ParamKind = "chain" | "address" | "amount" | "any";
const PARAM_KIND: Record<string, ParamKind> = {
  chain_id: "chain",
  asset: "address",
  address: "address",
  wallet: "address",
  amount: "amount",
};
const kindOf = (key: string): ParamKind => PARAM_KIND[key] ?? "any";
/** 파라미터 라벨 옆에 보여줄 기대-타입 칩. */
function kindChip(kind: ParamKind, t: TFunction): string | null {
  switch (kind) {
    case "chain":
      return t("editor:customField.kindChip.chain");
    case "address":
      return t("editor:customField.kindChip.address");
    case "amount":
      return t("editor:customField.kindChip.amount");
    default:
      return null;
  }
}

interface SelectorOption {
  sel: string;
  label: string;
  isChain: boolean;
  isAddress: boolean;
  isAmount: boolean;
}

/** 거래(tx) 레벨 + 액션 필드 → 종류 태그가 붙은 셀렉터 선택지. */
function buildOptions(fields: readonly FieldOption[]): SelectorOption[] {
  const root: SelectorOption[] = [
    { sel: "$.root.chain_id", label: i18n.t("editor:customField.rootChain"), isChain: true, isAddress: false, isAmount: false },
    { sel: "$.root.from", label: i18n.t("editor:customField.rootFrom"), isChain: false, isAddress: true, isAmount: false },
    { sel: "$.root.to", label: i18n.t("editor:customField.rootTo"), isChain: false, isAddress: true, isAmount: false },
  ];
  const action = fields
    .filter((f) => f.source === "base" && f.path.startsWith("context.") && f.fieldKind.startsWith("primitive."))
    .map((f) => {
      const leaf = f.path.split(".").pop() ?? "";
      return {
        sel: `$.action.${f.path.slice("context.".length)}`,
        label: f.label,
        isChain: leaf === "chain",
        isAddress: f.role === "address",
        isAmount:
          /amount|size|qty/i.test(leaf) ||
          f.fieldKind === "primitive.Long" ||
          f.fieldKind === "primitive.decimal",
      };
    });
  return [...root, ...action];
}

/** `key` 파라미터에 들어갈 수 있는 선택지만 (종류 불명 키는 전부). */
function optionsFor(key: string, all: SelectorOption[]): SelectorOption[] {
  switch (kindOf(key)) {
    case "chain":
      return all.filter((o) => o.isChain);
    case "address":
      return all.filter((o) => o.isAddress);
    case "amount":
      return all.filter((o) => o.isAmount);
    default:
      return all;
  }
}

/** 메서드 템플릿 → 파라미터 기본값. 템플릿 셀렉터가 이 액션에 없으면 같은
 *  종류의 첫 선택지로 대체해, "셀렉터 직접 입력" 원문이 기본으로 노출되지
 *  않게 한다. 종류에 맞는 선택지가 하나도 없으면 빈 고정값. */
function defaultParams(m: MethodSpec, all: SelectorOption[]): Record<string, string> {
  return Object.fromEntries(
    Object.entries(m.params).map(([key, spec]) => {
      const raw = typeof spec === "object" && spec !== null && "literal" in spec ? String(spec.literal) : String(spec);
      if (!raw.startsWith("$.")) return [key, raw];
      const opts = optionsFor(key, all);
      if (opts.some((o) => o.sel === raw)) return [key, raw];
      return [key, opts[0]?.sel ?? ""];
    }),
  );
}

export function CustomFieldModal({
  existing,
  actionTag,
  fields,
  onCreate,
  onClose,
}: {
  /** 이미 있는 보강 필드 전체 (내장 + 사용자) — 이름/입력 중복 검사용. */
  existing: EnrichmentRegistry;
  /** 현재 trigger의 action tag (appliesTo로 기록). null = 모든 동작. */
  actionTag: string | null;
  /** 폼의 필드 카탈로그 — 파라미터 드롭다운의 "이 거래에서 가져오기" 항목. */
  fields: readonly FieldOption[];
  onCreate: (draft: CustomFieldDraft) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation("editor");
  const existingNames = Object.keys(existing);
  const allOptions = useMemo(() => buildOptions(fields), [fields]);
  const [method, setMethod] = useState<MethodSpec>(METHOD_CATALOG[0]);
  // 표시 이름은 메서드 라벨이 기본값 — 사용자가 고치기 전까지 메서드를 따라간다.
  const [label, setLabel] = useState(() => methodLabel(METHOD_CATALOG[0]));
  const [labelTouched, setLabelTouched] = useState(false);
  // 파라미터 기본값: 메서드 템플릿을 이 액션의 선택지로 해석한 것.
  const [params, setParams] = useState<Record<string, string>>(() =>
    defaultParams(METHOD_CATALOG[0], buildOptions(fields)),
  );
  const [projection, setProjection] = useState(METHOD_CATALOG[0].projection);

  // 내부 필드 이름(manifest의 id)은 메서드에서 자동 생성 — 사용자는 안 만진다.
  const name = useMemo(() => autoName(method.method, existingNames), [method.method, existingNames]);

  // 같은 표시 이름 금지 + 같은 메서드를 같은 입력으로 부르는 필드가 이미 있으면
  // 그 이름을 안내하고 생성을 막는다 (조용히 덮어쓰거나 쌍둥이가 생기지 않게).
  const labelDup = useMemo(() => {
    const want = label.trim();
    return want ? Object.values(existing).find((f) => f.label.ko === want) ?? null : null;
  }, [existing, label]);
  const sameCall = useMemo(() => {
    const wantParams = JSON.stringify(sortedParams(paramSpecs(params)));
    return (
      Object.values(existing).find(
        (f) =>
          f.method === method.method &&
          f.projection === projection &&
          JSON.stringify(sortedParams(f.params)) === wantParams,
      ) ?? null
    );
  }, [existing, method.method, projection, params]);

  const blockMsg = sameCall
    ? t("customField.sameCallBlock", { name: sameCall.label.ko })
    : labelDup
      ? t("customField.labelDupBlock")
      : null;
  const canCreate = label.trim().length > 0 && !blockMsg;

  const pickMethod = (m: MethodSpec) => {
    setMethod(m);
    setParams(defaultParams(m, allOptions));
    setProjection(m.projection);
    if (!labelTouched) setLabel(methodLabel(m));
  };

  const create = () => {
    if (!canCreate) return;
    onCreate({
      name,
      field: {
        type: method.type,
        label: { ko: label.trim() || name, en: name },
        appliesTo: actionTag ? [actionTag] : [],
        method: method.method,
        projection,
        params: paramSpecs(params),
      },
    });
    onClose();
  };

  return (
    <div className="cfm-bd" role="dialog" aria-modal onClick={onClose}>
      <div className="cfm" onClick={(e) => e.stopPropagation()}>
        <div className="cfm-h">
          {t("customField.title")}
          <button type="button" className="pf-iconbtn" onClick={onClose} aria-label={t("common:close")}>
            ✕
          </button>
        </div>
        <p className="cfm-sub">{t("customField.sub")}</p>

        <div className="cfm-step">{t("customField.step1")}</div>
        <label className="cfm-row">
          <span className="cfm-label">{t("customField.queryLabel")}</span>
          <select
            className="pf-select"
            value={method.method}
            onChange={(e) => {
              const m = METHOD_CATALOG.find((x) => x.method === e.target.value);
              if (m) pickMethod(m);
            }}
          >
            {METHOD_CATALOG.map((m) => (
              <option key={m.method} value={m.method}>
                {m.mock ? methodLabel(m) + t("customField.mockSuffix") : methodLabel(m)}
              </option>
            ))}
          </select>
        </label>
        <div className="cfm-desc">
          {methodDesc(method)}
          {method.mock && <span className="cfm-mock"> · {t("customField.mockNote")}</span>}
        </div>

        <div className="cfm-params">
          <div className="cfm-step">{t("customField.step2")}</div>
          {Object.entries(params).map(([key, v]) => {
            const chip = kindChip(kindOf(key), t);
            return (
              <div key={key} className="cfm-row">
                <span className="cfm-label">
                  {paramKeyLabel(key, t)}
                  {chip && <span className="cfm-kind">{chip}</span>}
                </span>
                <ParamPicker
                  value={v}
                  options={optionsFor(key, allOptions)}
                  onChange={(next) => setParams((p) => ({ ...p, [key]: next }))}
                />
              </div>
            );
          })}
          <details className="cfm-adv">
            <summary>{t("customField.advanced")}</summary>
            {Object.entries(params).map(([key, v]) => (
              <label key={key} className="cfm-row">
                <span className="cfm-label mono">{key}</span>
                <input
                  className="pf-val wide mono"
                  value={v}
                  onChange={(e) => setParams((p) => ({ ...p, [key]: e.target.value }))}
                />
              </label>
            ))}
            <label className="cfm-row">
              <span className="cfm-label mono">{t("customField.resultPath")}</span>
              <input
                className="pf-val wide mono"
                value={projection}
                onChange={(e) => setProjection(e.target.value)}
              />
            </label>
          </details>
        </div>

        <div className="cfm-step">{t("customField.step3")}</div>
        <label className="cfm-row">
          <span className="cfm-label">{t("customField.nameLabel")}</span>
          <input
            className="pf-val wide"
            value={label}
            autoFocus
            onChange={(e) => {
              setLabel(e.target.value);
              setLabelTouched(true);
            }}
            placeholder={t("customField.namePlaceholder")}
          />
        </label>
        <div className="cfm-autoname">
          {t("customField.autoNote", {
            type:
              method.type === "decimal"
                ? t("type.decimal")
                : method.type === "Long"
                  ? t("type.long")
                  : method.type === "Bool"
                    ? t("type.bool")
                    : t("type.string"),
          })}{" "}
          <code>context.custom.{name}</code>
        </div>
        {blockMsg && <div className="cfm-block">⚠ {blockMsg}</div>}

        <div className="cfm-actions">
          <button type="button" className="pf-add-cond" onClick={onClose}>
            {t("common:cancel")}
          </button>
          <button type="button" className="cfm-create" disabled={!canCreate} onClick={create}>
            {t("customField.create")}
          </button>
        </div>
      </div>
    </div>
  );
}

/** One param's value picker: a labeled dropdown of transaction values, with
 *  "고정값"/"셀렉터 직접 입력" as escape hatches (raw `$.…` never required). */
function ParamPicker({
  value,
  options,
  onChange,
}: {
  value: string;
  options: SelectorOption[];
  onChange: (v: string) => void;
}) {
  const { t } = useTranslation("editor");
  const known = options.find((o) => o.sel === value);
  const mode = known ? "known" : value.startsWith("$.") ? "raw" : "lit";
  return (
    <span className="cfm-pick">
      <select
        className="pf-select"
        value={known ? value : mode === "raw" ? "__raw" : "__lit"}
        onChange={(e) => {
          const v = e.target.value;
          if (v === "__lit") onChange("");
          else if (v === "__raw") onChange(value.startsWith("$.") ? value : "$.action.");
          else onChange(v);
        }}
      >
        <optgroup label={t("customField.fromTx")}>
          {options.map((o) => (
            <option key={o.sel} value={o.sel}>
              {o.label}
            </option>
          ))}
        </optgroup>
        <option value="__lit">{t("customField.litOption")}</option>
        <option value="__raw">{t("customField.rawOption")}</option>
      </select>
      {mode !== "known" && (
        <input
          className="pf-val mono"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={mode === "raw" ? "$.action.…" : t("customField.litPlaceholder")}
        />
      )}
    </span>
  );
}

/** 편집 문자열 맵 → ParamSpec 맵 (생성과 중복 비교가 같은 해석을 쓰게). */
function paramSpecs(params: Record<string, string>): Record<string, ParamSpec> {
  return Object.fromEntries(Object.entries(params).map(([k, v]) => [k, parseParam(v)]));
}

/** 키 순서를 고정해 JSON 문자열 비교가 안정되게. */
function sortedParams(p: Record<string, ParamSpec>): [string, ParamSpec][] {
  return Object.entries(p).sort(([a], [b]) => a.localeCompare(b));
}

/** Editable string → ParamSpec: `$.`-prefixed stays a selector; otherwise a
 *  literal (number when numeric, true/false when boolean, else string). */
function parseParam(v: string): string | { literal: number | string | boolean } {
  const t = v.trim();
  if (t.startsWith("$.")) return t;
  if (t === "true" || t === "false") return { literal: t === "true" };
  if (t !== "" && !Number.isNaN(Number(t))) return { literal: Number(t) };
  return { literal: t };
}
