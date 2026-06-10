/**
 * Adapter Function Bundle JSON schema (Tier A — Adapter Loader).
 *
 * Spec: ADAPTER_LOADER_ARCHITECTURE.md §4.1, §5.1 (BNF), §5.3.
 *
 * Provides TypeScript types and a runtime validator (`parseBundle`) that
 * parses unknown JSON into an `AdapterFunctionBundle` and throws on any shape
 * violation. All strategies are parsed so the registry can reject unsupported
 * ones with clear errors rather than opaque JSON failures.
 *
 * Uses plain TypeScript hand-written validators for consistency with the rest
 * of the adapter-loader stack (zod is not in the dependency tree).
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type BundleType = "adapter_function";

/**
 * Match criteria identifying a callsite. registry v2 schema uses
 * `chain_to_addresses` (chain → addresses map). v1 legacy (`chain_ids × to`
 * cartesian) is retained for backward compatibility (test fixtures, legacy
 * seed bundles). Consumers should use {@link matchEntries} to iterate
 * `(chainId, address)` pairs without branching on which shape parsed.
 */
export interface BundleMatch {
  /** v2 — chain id (string) → contract addresses. Absent when v1 shape used. */
  chain_to_addresses?: Record<string, string[]>;
  /** v1 legacy — chain ids the bundle applies to. Absent when v2 shape used. Cartesian with `to`. */
  chain_ids?: number[];
  /** v1 legacy — contract addresses. Absent when v2 shape used. Cartesian with `chain_ids`. */
  to?: string[];
  /**
   * Address-agnostic (selector-only) match — the bundle is keyed by
   * `(chain_id, selector)` ALONE (no per-address callkey), so it routes on ANY
   * contract address. Used for standard NFT `setApprovalForAll`. Requires
   * `chain_ids` and forbids `chain_to_addresses` / `to`.
   */
  address_agnostic?: true;
  /** "0x" + 8 hex chars. */
  selector: string;
}

/**
 * Iterate `(chainId, address)` pairs from a bundle match regardless of
 * whether v2 (`chain_to_addresses`) or v1 (`chain_ids × to`) shape parsed.
 * v2 takes precedence when both are present; legacy cartesian is fallback.
 */
export function matchEntries(m: BundleMatch): Array<[number, string]> {
  const v2 = m.chain_to_addresses;
  if (v2) {
    const keys = Object.keys(v2);
    if (keys.length > 0) {
      const out: Array<[number, string]> = [];
      for (const k of keys) {
        const cid = Number(k);
        for (const a of v2[k]) out.push([cid, a]);
      }
      return out;
    }
  }
  // v1 cartesian fallback
  const out: Array<[number, string]> = [];
  for (const cid of m.chain_ids ?? []) {
    for (const a of m.to ?? []) out.push([cid, a]);
  }
  return out;
}

export interface AbiFragment {
  function_name: string;
  /** Opaque JSON ABI (consumed by alloy at runtime). */
  abi: unknown;
}

export interface Requires {
  imperative: string[];
  /**
   * Adapter-layer capabilities — resolved at static lookup time
   * (e.g. "token_metadata" for the registry-side static token endpoint).
   */
  adapter_capabilities: string[];
  /**
   * Host-layer capabilities — dynamic RPC / oracle enrichment only
   * (e.g. "host:oracle"). Static lookups belong in `adapter_capabilities`.
   */
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
 * Uses `concat_bytes` (executable signature) rather than the BNF name `concat`.
 */
export type BuiltinFn =
  // §5.3.1 builtins
  | "select_address"
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
  | "unfold_v3_path"
  // Curve Router NG output-token resolver
  | "curve_route_last_token"
  // Curve V1/V2/NG coins[i]/coins[j] resolver
  | "select_from_literal_array"
  // Aerodrome Slipstream CL packed path decoder (int24 tickSpacing)
  | "unfold_slipstream_path"
  // Aerodrome Universal Router V2_SWAP packed-path endpoint resolver
  | "unfold_velo_v2_path"
  // UR/V4 action recipient sentinel resolver (0x..01 → from, 0x..02 → to)
  | "map_recipient";

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
  "curve_route_last_token",
  "select_from_literal_array",
  "unfold_slipstream_path",
  "unfold_velo_v2_path",
  "map_recipient",
]);

// ----- EmitRule strategies -----

export type EmitRule =
  | SingleEmit
  | OpcodeStreamDispatch
  | EnumTaggedDispatch
  | MulticallRecurse
  | ArrayEmit;

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

/**
 * Legacy typed array fan-out shape. Runtime v3 manifests use
 * `strategy: "array_emit"` plus raw `emit.body` templates. Generalises
 * `single_emit`:
 * the field tree is built once per array element with a synthetic `element`
 * arg (and optional `parallel_paths` rows) bound to the current index.
 */
export interface ArrayEmit {
  strategy: "array_emit";
  category: string;
  action: string;
  /** JsonPath to the tuple-array argument (must start with "$."). */
  array_path: string;
  /** Hard cap on element count (DoS guard). 1..=64. */
  max_elements: number;
  /**
   * Optional parallel arrays — synthetic arg name → JsonPath of another
   * array of equal length, index-synchronised with `array_path`.
   */
  parallel_paths?: Record<string, string>;
  /** Per-element field map. */
  fields: Record<string, ValueExpr>;
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
// EVM addresses: exactly `"0x" + 40 hex` (EIP-55 checksum or lowercase).
// Bridge table would be corrupted by non-address `to` strings.
const ADDRESS_RE = /^0x[0-9a-fA-F]{40}$/;
const MAX_TRANSFORM_ARGS = 4; // per BNF "max_4"
// `array_emit` JsonPaths must be rooted at `$.` — matches the Rust
// `eval::evaluate_json_path` contract which strips the `$.` prefix.
const JSONPATH_PREFIX = "$.";
// `array_emit.max_elements` ceiling — mirrors the Rust defence-in-depth cap.
const MAX_ARRAY_ELEMENTS = 64;

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

/**
 * Chain ids must be positive integers (EVM chain ids start at 1; 0 is a
 * sentinel that has no on-chain meaning and must not become a bridge key).
 */
function reqChainIdArray(v: unknown, path: string): number[] {
  const arr = reqIntegerArray(v, path);
  arr.forEach((item, i) => {
    if (item < 1) {
      throw new BundleParseError(
        `${path}[${i}]: expected positive chain id (>= 1), got ${item}`,
      );
    }
  });
  return arr;
}

/**
 * A bundle's `to` list must contain valid EVM addresses — non-address strings
 * would silently land in the bridge table and never match any legitimate tx.
 * Accepts lowercase and EIP-55 checksum; the bridge normalises downstream.
 */
function reqAddressArray(v: unknown, path: string): string[] {
  const arr = reqStringArray(v, path);
  arr.forEach((item, i) => {
    if (!ADDRESS_RE.test(item)) {
      throw new BundleParseError(
        `${path}[${i}]: expected EVM address "0x" + 40 hex, got "${item}"`,
      );
    }
  });
  return arr;
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

    case "array_emit": {
      const arrayPath = reqString(obj.array_path, `${path}.array_path`);
      if (!arrayPath.startsWith(JSONPATH_PREFIX)) {
        throw new BundleParseError(
          `${path}.array_path: expected JsonPath starting with "${JSONPATH_PREFIX}", got "${arrayPath}"`,
        );
      }
      const maxElements = obj.max_elements;
      if (
        typeof maxElements !== "number" ||
        !Number.isInteger(maxElements) ||
        maxElements < 1 ||
        maxElements > MAX_ARRAY_ELEMENTS
      ) {
        throw new BundleParseError(
          `${path}.max_elements: expected integer in [1, ${MAX_ARRAY_ELEMENTS}]`,
        );
      }
      // `parallel_paths` is optional — a plain string→JsonPath map. Each
      // value must be a "$." rooted path; the empty/omitted case is allowed.
      let parallelPaths: Record<string, string> | undefined;
      if ("parallel_paths" in obj) {
        const rawParallel = reqObj(
          obj.parallel_paths,
          `${path}.parallel_paths`,
        );
        const parsed: Record<string, string> = {};
        for (const [k, raw] of Object.entries(rawParallel)) {
          const parallelPath = reqString(
            raw,
            `${path}.parallel_paths.${k}`,
          );
          if (!parallelPath.startsWith(JSONPATH_PREFIX)) {
            throw new BundleParseError(
              `${path}.parallel_paths.${k}: expected JsonPath starting with "${JSONPATH_PREFIX}", got "${parallelPath}"`,
            );
          }
          parsed[k] = parallelPath;
        }
        parallelPaths = parsed;
      }
      const arrayEmit: ArrayEmit = {
        strategy: "array_emit",
        category: reqString(obj.category, `${path}.category`),
        action: reqString(obj.action, `${path}.action`),
        array_path: arrayPath,
        max_elements: maxElements,
        fields: parseFields(obj.fields, `${path}.fields`),
      };
      if (parallelPaths !== undefined) {
        arrayEmit.parallel_paths = parallelPaths;
      }
      return arrayEmit;
    }

    default:
      throw new BundleParseError(
        `${path}.strategy: unknown strategy "${strategy}"`,
      );
  }
}

function parseRequires(v: unknown, path: string): Requires {
  const obj = reqObj(v, path);
  // Both capability fields are optional — default to [] when absent so
  // older bundles without these fields still parse.
  const adapterCaps =
    "adapter_capabilities" in obj
      ? reqStringArray(
          obj.adapter_capabilities,
          `${path}.adapter_capabilities`,
        )
      : [];
  const hostCaps =
    "host_capabilities" in obj
      ? reqStringArray(obj.host_capabilities, `${path}.host_capabilities`)
      : [];
  return {
    imperative: reqStringArray(obj.imperative, `${path}.imperative`),
    adapter_capabilities: adapterCaps,
    host_capabilities: hostCaps,
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

  // Address-agnostic (selector-only) match — keyed by (chain_id, selector) with
  // NO per-address `to`. Standard NFT setApprovalForAll. Handled first because
  // it carries `chain_ids` but (unlike the v1 legacy shape) no `to`, so the v1
  // branch's `reqAddressArray(obj.to)` would otherwise reject it.
  if (obj.address_agnostic === true) {
    const result: BundleMatch = { selector, address_agnostic: true };
    result.chain_ids = reqChainIdArray(obj.chain_ids, `${path}.chain_ids`);
    return result;
  }

  const hasV2 = obj.chain_to_addresses !== undefined;
  const hasV1 = obj.chain_ids !== undefined || obj.to !== undefined;
  if (!hasV2 && !hasV1) {
    throw new BundleParseError(
      `${path}: must have "chain_to_addresses" (v2), "chain_ids"+"to" (v1 legacy), or "address_agnostic"`,
    );
  }

  const result: BundleMatch = { selector };
  if (hasV2) {
    const m = reqObj(obj.chain_to_addresses, `${path}.chain_to_addresses`);
    const c2a: Record<string, string[]> = {};
    for (const key of Object.keys(m)) {
      const cid = Number(key);
      if (!Number.isInteger(cid) || cid < 1) {
        throw new BundleParseError(
          `${path}.chain_to_addresses["${key}"]: key must stringify positive integer`,
        );
      }
      c2a[key] = reqAddressArray(m[key], `${path}.chain_to_addresses["${key}"]`);
    }
    if (Object.keys(c2a).length === 0) {
      throw new BundleParseError(
        `${path}.chain_to_addresses: must have at least one chain entry`,
      );
    }
    result.chain_to_addresses = c2a;
  }
  if (hasV1) {
    result.chain_ids = reqChainIdArray(obj.chain_ids, `${path}.chain_ids`);
    result.to = reqAddressArray(obj.to, `${path}.to`);
  }
  return result;
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
 * `BundleParseError` on any shape violation. Pure shape check — semantic
 * validation (ABI / Tier-B imperatives) lives elsewhere.
 *
 * Rejects `schema_version === "3"` so the v3 path (`parseBundleV3`) handles
 * those without v1/v2 silently swallowing the hierarchical bundle shape.
 * Bundles that omit `schema_version` or carry `"2"` pass through normally.
 */
export function parseBundle(input: unknown): AdapterFunctionBundle {
  const obj = reqObj(input, "$");

  // Reject v3 explicitly so a routing bug (calling parseBundle with a v3
  // payload) surfaces a clear error instead of cascading parse failures.
  const schemaVersionRaw = obj.schema_version;
  if (typeof schemaVersionRaw === "string" && schemaVersionRaw === "3") {
    throw new BundleParseError(
      `$.schema_version: v3 bundles must be parsed via parseBundleV3 (got "${schemaVersionRaw}")`,
    );
  }

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

// ---------------------------------------------------------------------------
// v3 schema — hierarchical ActionBody
// ---------------------------------------------------------------------------
//
// v3 bundles ship the hierarchical `emit.body` tree consumed directly by the
// WASM `action_builder`. The SW validates only routing-critical fields
// (id / type / schema_version / match / abi_fragment); deep `emit` validation
// lives in `declarative_install_v3_json` and the build-time `build-index.ts`.

export type V3BundleType = "adapter_action";

export interface V3TypedData {
  domain_name: string;
  verifying_contract: string;
  primary_type: string;
  types: Record<string, Array<{ name: string; type: string }>>;
}

export interface V3BundleMatch {
  /** "0x" + 8 hex chars. Same wire shape as v1/v2. */
  selector: string;
  /** v2-style explicit chain → addresses map. Mutually exclusive with the source form. */
  chain_to_addresses?: Record<string, string[]>;
  /**
   * v2 ERC-standard auto-enumerate marker (e.g. "tokens:erc20"). Build-time
   * `build-index.ts` expands this against `tokens/<chainId>/*.json` to
   * produce concrete callkeys. At runtime the SW carries the raw source
   * label through to WASM untouched — install / route only see the
   * hydrated bundle from the registry, never this hint.
   */
  chain_to_addresses_source?: string;
  /** Companion to `chain_to_addresses_source`. */
  chain_ids?: number[];
  /**
   * Address-agnostic (selector-only) match — keyed by (chain_id, selector)
   * ALONE, no per-address callkey. Standard NFT setApprovalForAll. Requires
   * `chain_ids`; mutually exclusive with chain_to_addresses(_source).
   */
  address_agnostic?: true;
  /** Optional EIP-712 typed-data section for sign-only bundles (Permit2 et al). */
  typed_data?: V3TypedData;
}

export interface V3Bundle {
  type: V3BundleType;
  id: string;
  publisher?: string;
  schema_version: "3";
  match: V3BundleMatch;
  abi_fragment: {
    function_name: string;
    /** JSON ABI — opaque at the SW layer; alloy decodes inside WASM. */
    abi: unknown;
  };
  /**
   * Hierarchical emit body — pass-through at the SW layer. The WASM
   * `declarative_install_v3_json` / `declarative_route_request_v3_json`
   * pair consumes the raw `serde_json::Value` so the SW never has to
   * model the shape directly.
   */
  emit: unknown;
  /** Optional `multicall_recurse` recurse config — also pass-through. */
  recurse?: unknown;
  /**
   * Optional manifest-level requires (capabilities list). Retained as
   * pass-through so the WASM bridge can read it later without an SW
   * schema bump.
   */
  requires?: unknown;
}

/**
 * Iterate `(chainId, address)` pairs from a v3 bundle match. Mirrors
 * {@link matchEntries} for v2 but operates on the {@link V3BundleMatch}
 * shape directly. `chain_to_addresses_source` bundles are returned with an
 * empty pair list — the SW path expects the registry to have already
 * hydrated the explicit map by the time the callkey response arrives, so
 * the only case left is a defensive zero-pair iteration.
 */
export function matchEntriesV3(m: V3BundleMatch): Array<[number, string]> {
  const v2 = m.chain_to_addresses;
  if (v2) {
    const keys = Object.keys(v2);
    if (keys.length > 0) {
      const out: Array<[number, string]> = [];
      for (const k of keys) {
        const cid = Number(k);
        for (const a of v2[k]) out.push([cid, a]);
      }
      return out;
    }
  }
  return [];
}

function parseV3Match(v: unknown, path: string): V3BundleMatch {
  const obj = reqObj(v, path);
  const selector = reqString(obj.selector, `${path}.selector`);
  if (!SELECTOR_RE.test(selector)) {
    throw new BundleParseError(
      `${path}.selector: expected "0x" + 8 hex chars, got "${selector}"`,
    );
  }

  const hasExplicit = obj.chain_to_addresses !== undefined;
  const hasSource = obj.chain_to_addresses_source !== undefined;
  const isAgnostic = obj.address_agnostic === true;
  if (!hasExplicit && !hasSource && !isAgnostic) {
    throw new BundleParseError(
      `${path}: must have "chain_to_addresses", "chain_to_addresses_source", or "address_agnostic"`,
    );
  }

  const result: V3BundleMatch = { selector };

  // Address-agnostic (selector-only) — standard NFT setApprovalForAll. Keyed by
  // (chain_id, selector); carries chain_ids but NO per-address `to`, so return
  // early before the explicit / source branches.
  if (isAgnostic) {
    result.address_agnostic = true;
    result.chain_ids = reqChainIdArray(obj.chain_ids, `${path}.chain_ids`);
    return result;
  }

  if (hasExplicit) {
    const m = reqObj(obj.chain_to_addresses, `${path}.chain_to_addresses`);
    const c2a: Record<string, string[]> = {};
    for (const key of Object.keys(m)) {
      const cid = Number(key);
      if (!Number.isInteger(cid) || cid < 1) {
        throw new BundleParseError(
          `${path}.chain_to_addresses["${key}"]: key must stringify positive integer`,
        );
      }
      c2a[key] = reqAddressArray(
        m[key],
        `${path}.chain_to_addresses["${key}"]`,
      );
    }
    if (Object.keys(c2a).length === 0) {
      throw new BundleParseError(
        `${path}.chain_to_addresses: must have at least one chain entry`,
      );
    }
    result.chain_to_addresses = c2a;
  }

  if (hasSource) {
    result.chain_to_addresses_source = reqString(
      obj.chain_to_addresses_source,
      `${path}.chain_to_addresses_source`,
    );
    if ("chain_ids" in obj) {
      result.chain_ids = reqChainIdArray(obj.chain_ids, `${path}.chain_ids`);
    }
  }

  if ("typed_data" in obj) {
    const td = reqObj(obj.typed_data, `${path}.typed_data`);
    const rawTypes = reqObj(td.types, `${path}.typed_data.types`);
    const types: Record<string, Array<{ name: string; type: string }>> = {};
    for (const [k, raw] of Object.entries(rawTypes)) {
      const arr = reqArray(raw, `${path}.typed_data.types.${k}`);
      types[k] = arr.map((entry, i) => {
        const fieldObj = reqObj(
          entry,
          `${path}.typed_data.types.${k}[${i}]`,
        );
        return {
          name: reqString(
            fieldObj.name,
            `${path}.typed_data.types.${k}[${i}].name`,
          ),
          type: reqString(
            fieldObj.type,
            `${path}.typed_data.types.${k}[${i}].type`,
          ),
        };
      });
    }
    result.typed_data = {
      domain_name: reqString(
        td.domain_name,
        `${path}.typed_data.domain_name`,
      ),
      verifying_contract: reqString(
        td.verifying_contract,
        `${path}.typed_data.verifying_contract`,
      ),
      primary_type: reqString(
        td.primary_type,
        `${path}.typed_data.primary_type`,
      ),
      types,
    };
  }

  return result;
}

function parseV3AbiFragment(
  v: unknown,
  path: string,
): { function_name: string; abi: unknown } {
  const obj = reqObj(v, path);
  return {
    function_name: reqString(obj.function_name, `${path}.function_name`),
    abi: obj.abi,
  };
}

/**
 * Parse arbitrary JSON into a {@link V3Bundle}. v1/v2 payloads — including
 * payloads with `schema_version` absent or "2" — yield `null` so the
 * caller can fall back to {@link parseBundle}.
 *
 * The validator is intentionally lighter than `parseBundle`: only the
 * routing-critical fields (`type`, `id`, `schema_version`, `match`,
 * `abi_fragment.function_name`) are validated structurally. The
 * hierarchical `emit` tree, optional `recurse`, and optional `requires`
 * flow through unchanged — `declarative_install_v3_json` / `build-index.ts`
 * own the deep schema validation, and any inline validation here would
 * have to duplicate them.
 *
 * Throws {@link BundleParseError} when `schema_version === "3"` but the
 * payload is structurally broken (e.g. missing `id`); this is the
 * "matched v3 but invalid" branch that callers MUST surface as a fault
 * instead of silently downgrading to the v1/v2 path.
 */
export function parseBundleV3(input: unknown): V3Bundle | null {
  if (!isPlainObject(input)) return null;
  const obj = input;

  const schemaVersionRaw = obj.schema_version;
  if (typeof schemaVersionRaw !== "string" || schemaVersionRaw !== "3") {
    return null;
  }

  const typeRaw = obj.type;
  if (typeof typeRaw !== "string" || typeRaw !== "adapter_action") {
    throw new BundleParseError(
      `$.type: v3 bundles must declare "adapter_action" (got ${typeof typeRaw === "string" ? `"${typeRaw}"` : typeof typeRaw})`,
    );
  }

  const id = reqString(obj.id, "$.id");
  const match = parseV3Match(obj.match, "$.match");
  const abi_fragment = parseV3AbiFragment(obj.abi_fragment, "$.abi_fragment");

  if (!("emit" in obj)) {
    throw new BundleParseError("$.emit: required for v3 bundles");
  }

  const result: V3Bundle = {
    type: "adapter_action",
    id,
    schema_version: "3",
    match,
    abi_fragment,
    emit: obj.emit,
  };

  if (typeof obj.publisher === "string") {
    result.publisher = obj.publisher;
  }
  if ("recurse" in obj) {
    result.recurse = obj.recurse;
  }
  if ("requires" in obj) {
    result.requires = obj.requires;
  }

  return result;
}
