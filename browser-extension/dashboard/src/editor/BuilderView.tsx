import { useEffect, useState } from "react";
import {
  compileRule,
  fetchActions,
  fetchActionSchema,
  type OverlayField,
} from "../policy/builder-wasm";
import { loadOverlay } from "../policy/manifest-overlay";
import type {
  ActionSchemaDto,
  PolicyRule,
  Predicate,
} from "../policy/types";
import { useExtension } from "../sdk-context";
import { PredicateRow } from "./PredicateRow";
import "./BuilderView.css";

interface BuilderViewProps {
  rule: PolicyRule;
  onRuleChange: (rule: PolicyRule) => void;
  /**
   * Fired when the user successfully compiles the current rule. `cedarText`
   * is the emitted Cedar string; `compiledRule` is the exact `PolicyRule`
   * snapshot the compile ran against so the caller can detect later edits
   * (`rule !== compiledRule`) and disable save until a re-compile.
   */
  onCedarChange: (cedarText: string, compiledRule: PolicyRule) => void;
}

export function BuilderView({
  rule,
  onRuleChange,
  onCedarChange,
}: BuilderViewProps) {
  const { client } = useExtension();
  const [actions, setActions] = useState<string[]>([]);
  const [schema, setSchema] = useState<ActionSchemaDto | null>(null);
  const [schemaErr, setSchemaErr] = useState<string | null>(null);
  const [compileError, setCompileError] = useState<string | null>(null);
  // We keep the overlay alongside the schema so the compile path can use
  // the SAME overlay the picker rendered against. If they drift (e.g.
  // user adds a manifest field, picks it, then a sibling tab clears
  // chrome.storage between schema fetch and compile click), the compile
  // would dead-end with `unknown_field`. Holding state here pins them.
  const [overlay, setOverlay] = useState<readonly OverlayField[]>([]);

  useEffect(() => {
    void fetchActions().then(setActions);
  }, []);

  // Pull the action schema whenever the user picks a different action.
  // Operators per field come back in this same call so we never need a
  // parallel client-side operator table.
  //
  // Manifest-installed custom fields (those the user added via the
  // `/manifests/<action>` editor) live only in the engine's enriched
  // schema, not the bundled static schema. We pull `customContexts` from
  // `getEnrichedSchema()` and pass scalar entries the static schema
  // doesn't already cover as an overlay so they surface in the picker
  // alongside the bundled custom fields.
  //
  // Best-effort: if `getEnrichedSchema()` fails (e.g. no manifests
  // installed yet, transport error) we fall back to the static schema.
  // The builder must always render — overlay is an enhancement, not a
  // hard requirement.
  useEffect(() => {
    let cancelled = false;
    setSchemaErr(null);
    void (async () => {
      const loaded = (await loadOverlay(client, rule.action)) ?? [];
      if (cancelled) return;
      setOverlay(loaded);
      const res = await fetchActionSchema(
        rule.action,
        loaded.length > 0 ? loaded : undefined,
      );
      if (cancelled) return;
      if (res.schema) {
        setSchema(res.schema);
      } else {
        setSchema(null);
        setSchemaErr(res.error?.message ?? "schema lookup failed");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [rule.action, client]);

  const handleField = <K extends keyof PolicyRule>(
    key: K,
    value: PolicyRule[K],
  ) => {
    onRuleChange({ ...rule, [key]: value });
  };

  // The id stores `dashboard::<action>/<suffix>` internally so the catalog
  // can keep the same `<source>::<action>/<name>` namespacing as bundled
  // defaults. The user only types the `<suffix>` half; these helpers split
  // and recompose around that boundary.
  const idSuffix = stripIdPrefix(rule.id, rule.action);
  const composeId = (action: string, suffix: string): string =>
    `dashboard::${action}/${suffix}`;

  const handleIdSuffixChange = (suffix: string) => {
    onRuleChange({ ...rule, id: composeId(rule.action, suffix) });
  };

  // Action change keeps the user-visible suffix but swaps the namespace so
  // id ↔ action stay in lockstep — no save-time mismatch can be triggered
  // from the builder.
  const handleActionChange = (nextAction: string) => {
    onRuleChange({
      ...rule,
      action: nextAction,
      id: composeId(nextAction, idSuffix),
    });
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
    // Capture the snapshot the compile ran against so the caller can pair
    // the resulting Cedar text with the exact rule shape that produced it.
    // Without the snapshot, a parallel edit during the (small but non-zero)
    // compile latency window could silently associate fresh text with a
    // mutated rule.
    const snapshot = rule;
    // Pass the overlay so a rule built against an overlay field doesn't
    // dead-end with `unknown_field` — the schema and compile paths must
    // see the same field set.
    const { cedarText, error } = await compileRule(
      snapshot,
      overlay.length > 0 ? overlay : undefined,
    );
    if (cedarText) onCedarChange(cedarText, snapshot);
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
            value={idSuffix}
            placeholder="newrule(0)"
            onChange={(e) => handleIdSuffixChange(e.target.value)}
          />
        </Field>
        <Field label="Action">
          {actions.length > 0 ? (
            <select
              value={rule.action}
              onChange={(e) => handleActionChange(e.target.value)}
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
              onChange={(e) => handleActionChange(e.target.value)}
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
          placeholder="describe why this should be blocked"
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

// Peel off whichever prefix this id actually carries so the user input
// stays the bare suffix. Prefers the current-action prefix; falls back to
// `dashboard::` alone for hydrated ids whose stored action segment doesn't
// match the dropdown (e.g. legacy `dashboard::my/foo` policies).
function stripIdPrefix(id: string, action: string): string {
  const actionPrefix = `dashboard::${action}/`;
  if (id.startsWith(actionPrefix)) return id.slice(actionPrefix.length);
  const dashboardPrefix = "dashboard::";
  if (id.startsWith(dashboardPrefix)) return id.slice(dashboardPrefix.length);
  return id;
}

