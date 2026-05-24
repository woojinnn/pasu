import type { ParamSchema } from "./params-validator";

const INTEGER_RE = /^-?[0-9]+$/;
const ADDRESS_RE = /^0x[a-fA-F0-9]{40}$/;

/**
 * Render a typed parameter value as a Cedar literal. Each call re-validates
 * the value's type and applies the appropriate escaping. Repo policies
 * treat addresses as plain strings (no `EthereumAddress` entity), so
 * address values render as quoted lowercase strings.
 */
export function renderCedarLiteral(decl: ParamSchema, value: unknown): string {
  switch (decl.type) {
    case "integer": {
      const s = String(value);
      if (!INTEGER_RE.test(s)) throw new Error(`integer regex fail: ${s}`);
      return s;
    }
    case "address": {
      const v = String(value);
      if (!ADDRESS_RE.test(v)) throw new Error(`address regex fail: ${v}`);
      return JSON.stringify(v.toLowerCase());
    }
    case "enum": {
      // Reject non-string inputs explicitly so a number that happens to
      // coerce to an allowed string can't sneak through.
      if (typeof value !== "string") {
        throw new Error(`enum value must be a string, got ${typeof value}`);
      }
      if (!decl.values.includes(value))
        throw new Error(`enum value fail: ${value}`);
      return JSON.stringify(value);
    }
    case "string": {
      const v = String(value);
      return JSON.stringify(v);
    }
    case "array": {
      const arr = value as unknown[];
      const inner = arr
        .map((item) => renderCedarLiteral(decl.items, item))
        .join(", ");
      return `[${inner}]`;
    }
  }
}
