import { useEffect, useState } from "react";
import {
  compileRule,
  fetchActions,
  fetchActionSchema,
} from "../policy/builder-wasm";
import type {
  ActionSchemaDto,
  PolicyRule,
  Predicate,
} from "../policy/types";
import { PredicateRow } from "./PredicateRow";
import "./BuilderView.css";

interface BuilderViewProps {
  rule: PolicyRule;
  onRuleChange: (rule: PolicyRule) => void;
  onCedarChange: (cedarText: string) => void;
}

export function BuilderView({
  rule,
  onRuleChange,
  onCedarChange,
}: BuilderViewProps) {
  const [actions, setActions] = useState<string[]>([]);
  const [schema, setSchema] = useState<ActionSchemaDto | null>(null);
  const [schemaErr, setSchemaErr] = useState<string | null>(null);
  const [compileError, setCompileError] = useState<string | null>(null);

  useEffect(() => {
    void fetchActions().then(setActions);
  }, []);

  // Pull the action schema whenever the user picks a different action.
  // Operators per field come back in this same call so we never need a
  // parallel client-side operator table.
  useEffect(() => {
    let cancelled = false;
    setSchemaErr(null);
    void fetchActionSchema(rule.action).then((res) => {
      if (cancelled) return;
      if (res.schema) {
        setSchema(res.schema);
      } else {
        setSchema(null);
        setSchemaErr(res.error?.message ?? "schema lookup failed");
      }
    });
    return () => {
      cancelled = true;
    };
  }, [rule.action]);

  const handleField = <K extends keyof PolicyRule>(
    key: K,
    value: PolicyRule[K],
  ) => {
    onRuleChange({ ...rule, [key]: value });
  };

  const handlePredicateChange = (idx: number, next: Predicate) => {
    const arr = [...rule.predicates];
    arr[idx] = next;
    onRuleChange({ ...rule, predicates: arr });
  };

  const handleAddPredicate = () => {
    const firstField = schema?.fields[0];
    const firstOp = firstField?.operators[0];
    if (!firstField || !firstOp) return;
    const empty: Predicate = {
      field: firstField.path,
      op: firstOp.id,
      value:
        firstOp.arity === "none"
          ? null
          : firstOp.arity === "many"
            ? []
            : "",
    };
    onRuleChange({ ...rule, predicates: [...rule.predicates, empty] });
  };

  const handleRemovePredicate = (idx: number) => {
    onRuleChange({
      ...rule,
      predicates: rule.predicates.filter((_, i) => i !== idx),
    });
  };

  const handleCompile = async () => {
    setCompileError(null);
    const { cedarText, error } = await compileRule(rule);
    if (cedarText) onCedarChange(cedarText);
    else setCompileError(error?.message ?? "compile failed");
  };

  return (
    <div className="builder-view">
      <h2 className="builder-heading">규칙 빌더</h2>
      <p className="builder-sub">
        Action 스키마 기반 동적 폼. 조건을 추가하고 "Cedar로 컴파일" 클릭.
      </p>

      <div className="builder-row">
        <Field label="ID">
          <input
            type="text"
            value={rule.id}
            placeholder="dashboard::my/rule"
            onChange={(e) => handleField("id", e.target.value)}
          />
        </Field>
        <Field label="Action">
          {actions.length > 0 ? (
            <select
              value={rule.action}
              onChange={(e) => handleField("action", e.target.value)}
            >
              {actions.map((a) => (
                <option key={a} value={a}>
                  {a}
                </option>
              ))}
            </select>
          ) : (
            <input
              type="text"
              value={rule.action}
              onChange={(e) => handleField("action", e.target.value)}
            />
          )}
        </Field>
        <Field label="Severity">
          <select
            value={rule.severity}
            onChange={(e) =>
              handleField("severity", e.target.value as PolicyRule["severity"])
            }
          >
            <option value="deny">deny (Fail)</option>
            <option value="warn">warn (Warn)</option>
          </select>
        </Field>
      </div>

      <Field label="Reason">
        <input
          type="text"
          value={rule.reason}
          placeholder="사용자에게 표시될 설명"
          onChange={(e) => handleField("reason", e.target.value)}
        />
      </Field>

      <section className="builder-predicates">
        <header className="builder-predicates-head">
          <h3>조건 (AND 결합)</h3>
          <button
            type="button"
            className="builder-add"
            onClick={handleAddPredicate}
            disabled={!schema || schema.fields.length === 0}
          >
            + 조건 추가
          </button>
        </header>

        {schemaErr ? (
          <div className="builder-error">스키마 로드 실패: {schemaErr}</div>
        ) : null}

        {schema && schema.fields.some((f) => f.isCustom) ? (
          <p className="builder-predicates-note">
            <strong>커스텀 필드</strong>는 매니페스트 enrichment로 채워지는
            <code> context.custom.* </code>아래 값입니다. 컴파일러가 자동으로
            <code> has </code>가드를 삽입하므로 매니페스트가 해당 필드를
            제공하지 않으면 규칙은 일치하지 않습니다.
          </p>
        ) : null}

        {schema && rule.predicates.length === 0 ? (
          <p className="builder-empty">
            조건이 없으면 무조건 forbid(=Fail) 처리됩니다. 보통은 1개 이상
            추가합니다.
          </p>
        ) : null}

        {schema
          ? rule.predicates.map((p, idx) => (
              <PredicateRow
                key={idx}
                predicate={p}
                fields={schema.fields}
                onChange={(next) => handlePredicateChange(idx, next)}
                onRemove={() => handleRemovePredicate(idx)}
              />
            ))
          : null}
      </section>

      <button type="button" className="builder-compile" onClick={handleCompile}>
        Cedar로 컴파일
      </button>

      {compileError ? (
        <div className="builder-error">컴파일 실패: {compileError}</div>
      ) : null}
    </div>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="builder-field">
      <span>{label}</span>
      {children}
    </label>
  );
}
