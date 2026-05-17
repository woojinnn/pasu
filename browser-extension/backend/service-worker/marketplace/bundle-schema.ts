/**
 * Adapter Function Bundle JSON schema (Tier A — Adapter Marketplace).
 *
 * Spec: ADAPTER_MARKETPLACE_ARCHITECTURE.md §4.1, §5.1 (BNF), §5.3.
 *
 * Phase 0 scope:
 *   - TypeScript types matching the spec 1:1
 *   - A runtime validator (`parseBundle`) that parses unknown JSON into an
 *     `AdapterFunctionBundle` and throws on any shape violation.
 *   - All 4 strategies are parsed; only `single_emit` is the Phase 1
 *     execution target. Strategies parse so the registry / installer can
 *     reject unsupported strategies with clear errors instead of opaque
 *     JSON errors.
 *
 * Implementation note: the existing marketplace files
 * (`bundle-validator.ts`, `params-validator.ts`) use plain TypeScript +
 * hand-written validators rather than zod. We follow the same convention
 * for consistency (zod is not in the dependency tree).
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type BundleType = "adapter_function";

export interface BundleMatch {
  chain_ids: number[];
  to: string[];
  /** "0x" + 8 hex chars. */
  selector: string;
}

export interface AbiFragment {
  function_name: string;
  /** Opaque JSON ABI (consumed by alloy at runtime). */
  abi: unknown;
}

export interface Requires {
  imperative: string[];
  host_capabilities: string[];
  /** semver requirement, e.g. ">=0.1.0". */
  extension: string;
}

// ----- ValueExpr (§5.1 BNF) -----

export type ValueExpr = LiteralExpr | FromArgExpr | TransformExpr;

export interface LiteralExpr {
  literal: unknown;
}

export interface FromArgExpr {
  /** JsonPath, e.g. "$.args.path". */
  from: string;
  /** Optional host capability hint (e.g. "host:token_metadata"). */
  via?: string;
  /** Optional AmountKind ("exact" | "min" | "max"). */
  kind?: string;
}

export interface TransformExpr {
  fn: BuiltinFn;
  /** Bounded by max_4 per BNF — validated at parse time. */
  args: ValueExpr[];
}

/**
 * `WhitelistedFn` = `BuiltinFn` ∪ `TierBBackedFn` per §5.1.
 *
 * Note: spec BNF lists `concat`, but §5.3.1 has signature `concat_bytes`.
 * We adopt `concat_bytes` to match the executable signature. See review
 * finding M-3.
 */
export type BuiltinFn =
  // Phase 1
  | "select_address"
  // §5.3.1 builtins
  | "div"
  | "mul"
  | "concat_bytes"
  | "select_uint"
  | "select_bytes"
  | "literal_address"
  | "addr_eq"
  | "selector_eq"
  | "chain_id"
  | "now"
  // §5.3.2 Tier-B backed
  | "unfold_packed"
  | "unfold_v3_path";

const ALL_BUILTIN_FNS = new Set<BuiltinFn>([
  "select_address",
  "div",
  "mul",
  "concat_bytes",
  "select_uint",
  "select_bytes",
  "literal_address",
  "addr_eq",
  "selector_eq",
  "chain_id",
  "now",
  "unfold_packed",
  "unfold_v3_path",
]);

// ----- EmitRule strategies -----

export type EmitRule =
  | SingleEmit
  | OpcodeStreamDispatch
  | EnumTaggedDispatch
  | MulticallRecurse;

export interface SingleEmit {
  strategy: "single_emit";
  category: string;
  action: string;
  fields: Record<string, ValueExpr>;
}

export type UnknownOpcodePolicy = "deny" | "warn" | "ignore_step";
export type UnknownVariantPolicy = "deny" | "warn";

export interface PerOpcodeEmit {
  name: string;
  category: string;
  action: string;
  fields: Record<string, ValueExpr>;
}

export interface OpcodeStreamDispatch {
  strategy: "opcode_stream_dispatch";
  dispatcher_id: string;
  /** "0x" + 1-2 hex (e.g. "0x7f"). */
  mask: string;
  /** "0x" + 1-2 hex (e.g. "0x80"). */
  allow_revert_bit: string;
  per_opcode_emit: Record<string, PerOpcodeEmit>;
  unknown_opcode_policy: UnknownOpcodePolicy;
}

export interface PerVariantEmit {
  name: string;
  category: string;
  action: string;
  fields: Record<string, ValueExpr>;
}

export interface EnumTaggedDispatch {
  strategy: "enum_tagged_dispatch";
  dispatcher_id: string;
  tag_path: string;
  tag_decoder: string;
  per_variant_emit: Record<string, PerVariantEmit>;
  unknown_variant_policy: UnknownVariantPolicy;
}

export interface MulticallRecurse {
  strategy: "multicall_recurse";
  recurse_rule_id: string;
  /** 1..=5 per BNF. */
  max_depth: number;
}

// ----- Top-level -----

export interface AdapterFunctionBundle {
  type: BundleType;
  id: string;
  publisher: string;
  match: BundleMatch;
  abi_fragment: AbiFragment;
  emit: EmitRule;
  requires: Requires;
}

// ---------------------------------------------------------------------------
// Runtime validator — `parseBundle(json) => AdapterFunctionBundle`
// ---------------------------------------------------------------------------

const SELECTOR_RE = /^0x[0-9a-fA-F]{8}$/;
const HEX_U8_RE = /^0x[0-9a-fA-F]{1,2}$/;
const MAX_TRANSFORM_ARGS = 4; // per BNF "max_4"

export class BundleParseError extends Error {
  constructor(message: string) {
    super(`bundle parse: ${message}`);
    this.name = "BundleParseError";
  }
}

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function reqObj(v: unknown, path: string): Record<string, unknown> {
  if (!isPlainObject(v)) {
    throw new BundleParseError(`${path}: expected object`);
  }
  return v;
}

function reqString(v: unknown, path: string): string {
  if (typeof v !== "string") {
    throw new BundleParseError(`${path}: expected string`);
  }
  return v;
}

function reqArray(v: unknown, path: string): unknown[] {
  if (!Array.isArray(v)) {
    throw new BundleParseError(`${path}: expected array`);
  }
  return v;
}

function reqStringArray(v: unknown, path: string): string[] {
  const arr = reqArray(v, path);
  return arr.map((item, i) => reqString(item, `${path}[${i}]`));
}

function reqIntegerArray(v: unknown, path: string): number[] {
  const arr = reqArray(v, path);
  return arr.map((item, i) => {
    if (typeof item !== "number" || !Number.isInteger(item) || item < 0) {
      throw new BundleParseError(
        `${path}[${i}]: expected non-negative integer`,
      );
    }
    return item;
  });
}

function parseValueExpr(v: unknown, path: string): ValueExpr {
  const obj = reqObj(v, path);

  if ("literal" in obj) {
    if ("from" in obj || "fn" in obj) {
      throw new BundleParseError(
        `${path}: { literal } must not mix with { from } or { fn }`,
      );
    }
    return { literal: obj.literal };
  }

  if ("fn" in obj) {
    if ("from" in obj || "literal" in obj) {
      throw new BundleParseError(
        `${path}: { fn } must not mix with { from } or { literal }`,
      );
    }
    const fnName = reqString(obj.fn, `${path}.fn`);
    if (!ALL_BUILTIN_FNS.has(fnName as BuiltinFn)) {
      throw new BundleParseError(`${path}.fn: unknown function "${fnName}"`);
    }
    const argsRaw = reqArray(obj.args, `${path}.args`);
    if (argsRaw.length > MAX_TRANSFORM_ARGS) {
      throw new BundleParseError(
        `${path}.args: max ${MAX_TRANSFORM_ARGS} args, got ${argsRaw.length}`,
      );
    }
    const args = argsRaw.map((a, i) => parseValueExpr(a, `${path}.args[${i}]`));
    return { fn: fnName as BuiltinFn, args };
  }

  if ("from" in obj) {
    const from = reqString(obj.from, `${path}.from`);
    const out: FromArgExpr = { from };
    if ("via" in obj) out.via = reqString(obj.via, `${path}.via`);
    if ("kind" in obj) out.kind = reqString(obj.kind, `${path}.kind`);
    return out;
  }

  throw new BundleParseError(
    `${path}: ValueExpr must be one of { literal } | { from } | { fn }`,
  );
}

function parseFields(
  v: unknown,
  path: string,
): Record<string, ValueExpr> {
  const obj = reqObj(v, path);
  const out: Record<string, ValueExpr> = {};
  for (const [k, raw] of Object.entries(obj)) {
    out[k] = parseValueExpr(raw, `${path}.${k}`);
  }
  return out;
}

function parsePerOpcodeEmit(v: unknown, path: string): PerOpcodeEmit {
  const obj = reqObj(v, path);
  return {
    name: reqString(obj.name, `${path}.name`),
    category: reqString(obj.category, `${path}.category`),
    action: reqString(obj.action, `${path}.action`),
    fields: parseFields(obj.fields, `${path}.fields`),
  };
}

function parsePerVariantEmit(v: unknown, path: string): PerVariantEmit {
  const obj = reqObj(v, path);
  return {
    name: reqString(obj.name, `${path}.name`),
    category: reqString(obj.category, `${path}.category`),
    action: reqString(obj.action, `${path}.action`),
    fields: parseFields(obj.fields, `${path}.fields`),
  };
}

function parseEmitRule(v: unknown, path: string): EmitRule {
  const obj = reqObj(v, path);
  const strategy = reqString(obj.strategy, `${path}.strategy`);

  switch (strategy) {
    case "single_emit":
      return {
        strategy: "single_emit",
        category: reqString(obj.category, `${path}.category`),
        action: reqString(obj.action, `${path}.action`),
        fields: parseFields(obj.fields, `${path}.fields`),
      };

    case "opcode_stream_dispatch": {
      const mask = reqString(obj.mask, `${path}.mask`);
      if (!HEX_U8_RE.test(mask)) {
        throw new BundleParseError(
          `${path}.mask: expected "0x" + 1-2 hex chars`,
        );
      }
      const allowRevertBit = reqString(
        obj.allow_revert_bit,
        `${path}.allow_revert_bit`,
      );
      if (!HEX_U8_RE.test(allowRevertBit)) {
        throw new BundleParseError(
          `${path}.allow_revert_bit: expected "0x" + 1-2 hex chars`,
        );
      }
      const perOp = reqObj(obj.per_opcode_emit, `${path}.per_opcode_emit`);
      const perOpcodeEmit: Record<string, PerOpcodeEmit> = {};
      for (const [k, raw] of Object.entries(perOp)) {
        perOpcodeEmit[k] = parsePerOpcodeEmit(
          raw,
          `${path}.per_opcode_emit[${k}]`,
        );
      }
      const policy = reqString(
        obj.unknown_opcode_policy,
        `${path}.unknown_opcode_policy`,
      );
      if (!["deny", "warn", "ignore_step"].includes(policy)) {
        throw new BundleParseError(
          `${path}.unknown_opcode_policy: must be deny|warn|ignore_step`,
        );
      }
      return {
        strategy: "opcode_stream_dispatch",
        dispatcher_id: reqString(obj.dispatcher_id, `${path}.dispatcher_id`),
        mask,
        allow_revert_bit: allowRevertBit,
        per_opcode_emit: perOpcodeEmit,
        unknown_opcode_policy: policy as UnknownOpcodePolicy,
      };
    }

    case "enum_tagged_dispatch": {
      const perVar = reqObj(
        obj.per_variant_emit,
        `${path}.per_variant_emit`,
      );
      const perVariantEmit: Record<string, PerVariantEmit> = {};
      for (const [k, raw] of Object.entries(perVar)) {
        perVariantEmit[k] = parsePerVariantEmit(
          raw,
          `${path}.per_variant_emit[${k}]`,
        );
      }
      const policy = reqString(
        obj.unknown_variant_policy,
        `${path}.unknown_variant_policy`,
      );
      if (!["deny", "warn"].includes(policy)) {
        throw new BundleParseError(
          `${path}.unknown_variant_policy: must be deny|warn`,
        );
      }
      return {
        strategy: "enum_tagged_dispatch",
        dispatcher_id: reqString(obj.dispatcher_id, `${path}.dispatcher_id`),
        tag_path: reqString(obj.tag_path, `${path}.tag_path`),
        tag_decoder: reqString(obj.tag_decoder, `${path}.tag_decoder`),
        per_variant_emit: perVariantEmit,
        unknown_variant_policy: policy as UnknownVariantPolicy,
      };
    }

    case "multicall_recurse": {
      const maxDepth = obj.max_depth;
      if (
        typeof maxDepth !== "number" ||
        !Number.isInteger(maxDepth) ||
        maxDepth < 1 ||
        maxDepth > 5
      ) {
        throw new BundleParseError(
          `${path}.max_depth: expected integer in [1, 5]`,
        );
      }
      return {
        strategy: "multicall_recurse",
        recurse_rule_id: reqString(
          obj.recurse_rule_id,
          `${path}.recurse_rule_id`,
        ),
        max_depth: maxDepth,
      };
    }

    default:
      throw new BundleParseError(
        `${path}.strategy: unknown strategy "${strategy}"`,
      );
  }
}

function parseRequires(v: unknown, path: string): Requires {
  const obj = reqObj(v, path);
  return {
    imperative: reqStringArray(obj.imperative, `${path}.imperative`),
    host_capabilities: reqStringArray(
      obj.host_capabilities,
      `${path}.host_capabilities`,
    ),
    extension: reqString(obj.extension, `${path}.extension`),
  };
}

function parseMatch(v: unknown, path: string): BundleMatch {
  const obj = reqObj(v, path);
  const selector = reqString(obj.selector, `${path}.selector`);
  if (!SELECTOR_RE.test(selector)) {
    throw new BundleParseError(
      `${path}.selector: expected "0x" + 8 hex chars, got "${selector}"`,
    );
  }
  return {
    chain_ids: reqIntegerArray(obj.chain_ids, `${path}.chain_ids`),
    to: reqStringArray(obj.to, `${path}.to`),
    selector,
  };
}

function parseAbiFragment(v: unknown, path: string): AbiFragment {
  const obj = reqObj(v, path);
  return {
    function_name: reqString(obj.function_name, `${path}.function_name`),
    abi: obj.abi, // opaque — alloy validates at runtime
  };
}

/**
 * Parse arbitrary JSON into an `AdapterFunctionBundle`. Throws
 * `BundleParseError` on any shape violation. This is a pure shape check —
 * semantic validation (e.g. ABI inputs match field paths, Tier B
 * imperatives are installed) lives elsewhere.
 */
export function parseBundle(input: unknown): AdapterFunctionBundle {
  const obj = reqObj(input, "$");

  const bundleType = reqString(obj.type, "$.type");
  if (bundleType !== "adapter_function") {
    throw new BundleParseError(
      `$.type: only "adapter_function" supported, got "${bundleType}"`,
    );
  }

  return {
    type: "adapter_function",
    id: reqString(obj.id, "$.id"),
    publisher: reqString(obj.publisher, "$.publisher"),
    match: parseMatch(obj.match, "$.match"),
    abi_fragment: parseAbiFragment(obj.abi_fragment, "$.abi_fragment"),
    emit: parseEmitRule(obj.emit, "$.emit"),
    requires: parseRequires(obj.requires, "$.requires"),
  };
}
