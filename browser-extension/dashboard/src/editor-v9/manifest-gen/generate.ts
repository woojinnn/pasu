/**
 * Manifest auto-generation: given an editor policy (PolicyIR), produce the
 * `policy_rpc` + `custom_context` manifest that fills the `context.custom.*`
 * fields the policy reads — so an enrichment policy authored in /editor actually
 * fires instead of fail-opening on an unpopulated field.
 *
 * Pure function of (PolicyIR, registry). No I/O, no WASM. See
 * docs/design/editor-manifest-autogen.md.
 */

import { i18n } from "../../i18n";
import type { ActionScope, Expr, PolicyIR } from "../../cedar/blocks";
import {
  ENRICHMENT_FIELDS,
  type EnrichmentRegistry,
  type ParamSpec,
} from "./registry";

/** One output projection in a generated manifest. */
export interface ManifestOutput {
  kind: "context";
  field: string;
  type: string; // capitalized projection type ("Decimal", "Long", …)
  from: string;
  required: boolean;
}

/** One generated policy-RPC call spec. */
export interface ManifestRpc {
  id: string;
  method: string;
  params: Record<string, unknown>;
  outputs: ManifestOutput[];
  optional: boolean;
}

/** The generated v2 manifest (serializes to the engine's manifest.json shape). */
export interface GeneratedManifest {
  id: string;
  schema_version: 2;
  trigger: { where: { "action.tag": { eq: string } } };
  policy_rpc: ManifestRpc[];
  custom_context: { fields: Record<string, string> };
}

export interface GenError {
  /** The offending custom field, when the error is field-scoped. */
  field?: string;
  message: string;
}

export interface GenResult {
  /** `undefined` when the policy reads no enrichment fields (a base-context
   *  policy needs no manifest) or when generation failed (see `errors`). */
  manifest: GeneratedManifest | undefined;
  errors: GenError[];
}

/** Authoritative `id` / `severity` from the editor's save-time inputs, which
 *  take precedence over the (possibly unstamped) IR annotations. */
export interface GenOverrides {
  id?: string;
  severity?: string;
}

/** Generate the manifest for a single editor policy. */
export function generateManifest(
  policy: PolicyIR,
  registry: EnrichmentRegistry = ENRICHMENT_FIELDS,
  overrides: GenOverrides = {},
): GenResult {
  const fields = collectCustomFields(policy);
  // No enrichment fields → base-context policy → no manifest needed.
  if (fields.length === 0) return { manifest: undefined, errors: [] };

  const errors: GenError[] = [];
  const id = overrides.id ?? annotation(policy, "id");
  const severity = overrides.severity ?? annotation(policy, "severity");
  const tag = actionTag(policy.scope.action);

  if (!id) errors.push({ message: i18n.t("blocks:manifest.noId") });
  if (!tag) {
    errors.push({
      message: i18n.t("blocks:manifest.needSingleAction"),
    });
  }

  const policyRpc: ManifestRpc[] = [];
  const customFields: Record<string, string> = {};

  for (const field of fields) {
    const entry = registry[field];
    if (!entry) {
      errors.push({
        field,
        message: i18n.t("blocks:manifest.noBinding", { field }),
      });
      continue;
    }
    if (tag && !entry.appliesTo.includes(tag)) {
      errors.push({
        field,
        message: i18n.t("blocks:manifest.notApplicable", { field, tag, supported: entry.appliesTo.join(", ") }),
      });
      continue;
    }
    // A deny that hinges on enrichment must fail CLOSED on a missing value, so
    // the feeding call is required + non-optional (mirrors the engine's
    // default-policy gate). A warn may fail open, so it stays optional.
    const required = severity === "deny";
    policyRpc.push({
      id: field,
      method: entry.method,
      params: resolveParams(entry.params),
      outputs: [
        {
          kind: "context",
          field,
          type: projectionType(entry.type),
          from: entry.projection,
          required,
        },
      ],
      optional: !required,
    });
    customFields[field] = entry.type;
  }

  if (errors.length > 0) return { manifest: undefined, errors };

  return {
    manifest: {
      id: id as string,
      schema_version: 2,
      trigger: { where: { "action.tag": { eq: tag as string } } },
      policy_rpc: policyRpc,
      custom_context: { fields: customFields },
    },
    errors: [],
  };
}

/** Every identifier reached via `context.custom.<X>` or `context.custom has <X>`. */
export function collectCustomFields(policy: PolicyIR): string[] {
  const found = new Set<string>();

  // `e` is the `context.custom` chain (attr "custom" on var "context").
  const isContextCustom = (e: Expr): boolean =>
    e.kind === "attr" &&
    e.attr === "custom" &&
    e.of.kind === "var" &&
    e.of.name === "context";

  const visit = (e: Expr): void => {
    switch (e.kind) {
      case "attr":
        if (isContextCustom(e.of)) found.add(e.attr);
        visit(e.of);
        break;
      case "has":
        if (isContextCustom(e.of)) found.add(e.attr);
        visit(e.of);
        break;
      case "binary":
        visit(e.left);
        visit(e.right);
        break;
      case "unary":
        visit(e.operand);
        break;
      case "set":
        e.elements.forEach(visit);
        break;
      case "record":
        e.pairs.forEach((p) => visit(p.value));
        break;
      case "like":
        visit(e.of);
        break;
      case "is":
        visit(e.of);
        if (e.in) visit(e.in);
        break;
      case "if":
        visit(e.cond);
        visit(e.then);
        visit(e.else);
        break;
      case "ext":
        e.args.forEach(visit);
        break;
      // var / lit / litEntity / raw / hole — no sub-expressions to walk.
      default:
        break;
    }
  };

  for (const cond of policy.conditions) visit(cond.body);
  return [...found];
}

function annotation(policy: PolicyIR, name: string): string | undefined {
  return policy.annotations.find((a) => a.name === name)?.value;
}

/** `action == Ns::Action::"Swap"` → `"swap"`. Only a single `==` head is
 *  supported (enrichment params are action-shaped). */
function actionTag(scope: ActionScope): string | undefined {
  if (scope.kind !== "scopeEq") return undefined;
  return snakeCase(scope.entity.id);
}

const snakeCase = (s: string): string =>
  s.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase();

/** `custom_context` spelling → capitalized projection type spelling. */
const projectionType = (t: string): string => t.charAt(0).toUpperCase() + t.slice(1);

function resolveParams(params: Record<string, ParamSpec>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [key, spec] of Object.entries(params)) {
    out[key] = typeof spec === "object" && spec !== null && "literal" in spec ? spec.literal : spec;
  }
  return out;
}
