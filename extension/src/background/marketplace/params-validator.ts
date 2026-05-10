export type ParamSchema =
  | { type: "integer"; min: number; max: number; default?: number }
  | { type: "address"; default?: string }
  | { type: "enum"; values: readonly string[]; default?: string }
  | { type: "string"; maxLen: number; allowedChars: string; default?: string }
  | {
      type: "array";
      items: ParamSchema;
      maxItems: number;
      default?: unknown[];
    };

export type ParamsSchema = Record<string, ParamSchema>;
export type ParamValues = Record<string, unknown>;

const ADDRESS_RE = /^0x[a-fA-F0-9]{40}$/;

/**
 * Validate that `values` matches `schema`. Throws on the first violation.
 * No mutation; just an assertion gate.
 */
export function validateParams(
  schema: ParamsSchema,
  values: ParamValues,
): void {
  for (const [key, decl] of Object.entries(schema)) {
    if (!(key in values)) throw new Error(`param missing: ${key}`);
    validateOne(`${key}`, decl, values[key]);
  }
  for (const key of Object.keys(values)) {
    if (!(key in schema))
      throw new Error(`param not declared in schema: ${key}`);
  }
}

function validateOne(path: string, decl: ParamSchema, value: unknown): void {
  switch (decl.type) {
    case "integer": {
      if (typeof value !== "number" || !Number.isInteger(value)) {
        throw new Error(`${path}: expected integer, got ${typeof value}`);
      }
      if (value < decl.min || value > decl.max) {
        throw new Error(`${path}: ${value} outside [${decl.min}, ${decl.max}]`);
      }
      return;
    }
    case "address": {
      if (typeof value !== "string" || !ADDRESS_RE.test(value)) {
        throw new Error(`${path}: expected 0x-prefixed 40-char hex address`);
      }
      return;
    }
    case "enum": {
      if (typeof value !== "string" || !decl.values.includes(value)) {
        throw new Error(`${path}: must be one of ${decl.values.join(",")}`);
      }
      return;
    }
    case "string": {
      if (typeof value !== "string")
        throw new Error(`${path}: expected string`);
      if (value.length > decl.maxLen)
        throw new Error(`${path}: length > ${decl.maxLen}`);
      const allowed = new RegExp(
        `^[${escapeForCharClass(decl.allowedChars)}]*$`,
      );
      if (!allowed.test(value))
        throw new Error(`${path}: contains disallowed characters`);
      return;
    }
    case "array": {
      if (!Array.isArray(value)) throw new Error(`${path}: expected array`);
      if (value.length > decl.maxItems)
        throw new Error(`${path}: more than ${decl.maxItems} items`);
      value.forEach((item, i) =>
        validateOne(`${path}[${i}]`, decl.items, item),
      );
      return;
    }
  }
}

function escapeForCharClass(s: string): string {
  return s.replace(/[\\\]^-]/g, "\\$&");
}

/**
 * Build a ParamValues object from the schema's declared `default`s, with
 * any `overrides` taking precedence. Throws if any param has no `default`
 * AND was not supplied in `overrides`.
 */
export function defaultsFor(
  schema: ParamsSchema,
  overrides: ParamValues = {},
): ParamValues {
  const out: ParamValues = { ...overrides };
  for (const [key, decl] of Object.entries(schema)) {
    if (key in out) continue;
    if (decl.default === undefined) {
      throw new Error(
        `param "${key}" has no default and was not supplied at install time`,
      );
    }
    out[key] = decl.default;
  }
  return out;
}
