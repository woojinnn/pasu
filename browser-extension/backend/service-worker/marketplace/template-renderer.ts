import { renderCedarLiteral } from "./cedar-literal";
import type {
  ParamSchema,
  ParamsSchema,
  ParamValues,
} from "./params-validator";

const PLACEHOLDER_RE = /\{\{\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*\}\}/g;

/**
 * Whitelisted preceding-character patterns that mark a slot as appearing
 * in a safe Cedar AST position. A `{{x}}` placeholder MUST appear
 * immediately to the right of one of:
 *  - a comparison operator (==, !=, <, <=, >, >=)
 *  - the `in` keyword
 *  - an opening bracket / comma in a set or array literal `[...]`
 *
 * This blocks `when { {{cap}} }` (entire predicate as slot) — Codex's
 * documented round-3 limitation — at install time.
 */
const ALLOWED_PRECEDING: readonly RegExp[] = [
  /==\s*$/,
  /!=\s*$/,
  /<=\s*$/,
  />=\s*$/,
  /<\s*$/,
  />\s*$/,
  /\bin\s+$/,
  /[,[]\s*$/,
];

export interface RenderInput {
  policyId: string;
  templateText: string;
  paramsSchema: ParamsSchema;
  paramValues: ParamValues;
}

/** Strip `//` and `/* * /` comments so they don't false-match the slot scan. */
function stripCedarComments(text: string): string {
  const bytes = text;
  let out = "";
  let i = 0;
  while (i < bytes.length) {
    const c = bytes[i];
    const next = bytes[i + 1];
    if (c === "/" && next === "/") {
      while (i < bytes.length && bytes[i] !== "\n") i++;
    } else if (c === "/" && next === "*") {
      i += 2;
      while (
        i + 1 < bytes.length &&
        !(bytes[i] === "*" && bytes[i + 1] === "/")
      )
        i++;
      i = Math.min(i + 2, bytes.length);
    } else {
      out += c;
      i++;
    }
  }
  return out;
}

/**
 * Walk the template, locate each `{{name}}`, and assert the preceding
 * characters match a whitelisted Cedar context. Throws on the first
 * violation. Comments are stripped first to avoid false matches.
 */
export function assertSlotPositions(template: string): void {
  const stripped = stripCedarComments(template);
  for (const match of stripped.matchAll(PLACEHOLDER_RE)) {
    const before = stripped.slice(0, match.index ?? 0);
    if (!ALLOWED_PRECEDING.some((rx) => rx.test(before))) {
      throw new Error(
        `slot {{${match[1]}}} appears in a non-whitelisted position. ` +
          `Slots may only fill the right-hand side of comparison operators ` +
          `(==, !=, <, >, <=, >=), the right operand of \`in\`, or elements ` +
          `of a set/record literal.`,
      );
    }
  }
}

function substituteFinal(
  template: string,
  schema: ParamsSchema,
  values: ParamValues,
): string {
  return template.replace(PLACEHOLDER_RE, (_match, name: string) => {
    const decl: ParamSchema | undefined = schema[name];
    if (!decl) throw new Error(`template references undeclared param ${name}`);
    if (!(name in values))
      throw new Error(`param missing at render time: ${name}`);
    return renderCedarLiteral(decl, values[name]);
  });
}

/**
 * Render a Cedar template with typed param values. Layer 0 (slot
 * whitelist) runs first; subsequent rendering uses the typed Cedar
 * literal serializer. AST equivalence (Layer 3) is deferred: it requires
 * a Rust-side `parse_policy_ast_json` export which is documented in the
 * plan as a future addition. For v1, layers 0 + 1 + 2 are sufficient
 * because address renders as a string literal (no EthereumAddress entity)
 * and integers go through the regex-validated literal path.
 */
export function renderAndVerify(input: RenderInput): string {
  assertSlotPositions(input.templateText);
  return substituteFinal(
    input.templateText,
    input.paramsSchema,
    input.paramValues,
  );
}
