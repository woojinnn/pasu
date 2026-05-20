// Method catalog — the contract metadata for each RPC method.
//
// Each method exports a `MethodCatalogEntry` alongside its execution
// function so the registry can:
//   1. Validate inputs/outputs against a declared shape.
//   2. Expose a `GET /v1/methods` endpoint that returns the full
//      catalog (params + return types), which the dashboard's
//      manifest editor uses to drive its dropdowns. Without this the
//      editor has to treat every field as free text and trusts the
//      user not to typo a method name or a param key.
//
// Catalog entries are intentionally JSON-serialisable (no functions,
// no symbols) so we can ship the bundled copy as a static asset
// (`schema/method-catalog.json`) AND emit it through the HTTP
// endpoint without a separate serialisation layer.

/**
 * Cedar-aligned type spellings the manifest layer recognises. The
 * scalar set matches `policy_engine::schema::aliases` (without
 * `Set<Long>`, which never appears as a method return today). The
 * record set matches `policy_builder::aliases::record_leaves`, so
 * when the dashboard surfaces a method whose return type is one of
 * these, the builder's overlay knows how to expand it into per-leaf
 * predicate slots.
 *
 * Adding a NEW record spelling means updating BOTH this list AND
 * `policy_builder::aliases::record_leaves` — keep them in lockstep.
 */
export type CatalogScalarType =
  | "Long"
  | "String"
  | "Bool"
  | "decimal"
  | "Set<String>";

export type CatalogRecordType =
  | "UsdValuation"
  | "WindowStats"
  | "Validity"
  | "AssetRef"
  | "AmountConstraint"
  | "AssetRefWithAmountConstraint"
  | "TickRange"
  | "Pool";

export type CatalogType = CatalogScalarType | CatalogRecordType;

/**
 * Declaration for one method parameter. `defaultSelector` is a hint the
 * dashboard preloads into the manifest editor's selector picker — e.g.
 * `oracle.usd_value`'s `chain_id` param almost always wants
 * `$.root.chain_id`, so we set that as the default to save the user a
 * click. `enum_` (suffix to dodge the JS keyword) constrains the
 * accepted operand set; the manifest editor renders these as a
 * dropdown instead of a free-text/selector picker.
 */
export interface MethodParam {
  /** Cedar-aligned type the param accepts. */
  type: CatalogType;
  /** When `false`, the manifest may omit this param. */
  required: boolean;
  /** Human-readable description shown above the param input. */
  description?: string;
  /**
   * JSONPath selector preloaded into the manifest editor's value
   * input. Pure UX hint — the engine doesn't enforce it.
   */
  defaultSelector?: string;
  /**
   * Closed enum of accepted string values. When present the editor
   * renders a dropdown of these literals and the daemon-side
   * validator rejects anything else. Useful for `source: "coingecko"
   * | "chainlink" | ...` style selectors.
   */
  enum_?: readonly string[];
  /** Default literal value when the param is optional and omitted. */
  default?: string | number | boolean;
}

/**
 * Describes what one call to the method puts back into the policy
 * context. Two flavours:
 *  - `record`: the method returns a known Cedar record (e.g.
 *    `UsdValuation`). Manifest authors usually slot the whole record
 *    under one `outputs[].field` with `from: "$.result"`.
 *  - `scalar`: the method returns a primitive nested somewhere inside
 *    the JSON-RPC `result`. The `from` selector tells the manifest
 *    editor where to pull the value from; the type tells the builder
 *    what operators apply.
 */
export type MethodReturn =
  | { kind: "record"; type: CatalogRecordType }
  | {
      kind: "scalar";
      type: CatalogScalarType;
      /**
       * Default `outputs[].from` selector. Relative to the JSON-RPC
       * `result` object, so `$.result.bps` means "the `bps` key of
       * the response body".
       */
      from: string;
    };

export interface MethodCatalogEntry {
  /** Method name, e.g. `"oracle.usd_value"`. */
  name: string;
  /** One-liner shown in the method-picker dropdown. */
  description?: string;
  /**
   * Parameters in declaration order. Insertion order is preserved by
   * the UI so the most-frequently-edited params (e.g. the `asset`
   * selector) can sit at the top.
   */
  params: Record<string, MethodParam>;
  /** Shape of `result` the method puts on the wire. */
  returns: MethodReturn;
  /**
   * `"bundled"` when the catalog entry ships with the canonical
   * daemon binary; `"plugin"` when it was contributed by an
   * in-process plugin file; `"sidecar"` when it was discovered by
   * forwarding `/v1/methods` to a configured external endpoint.
   * Future work — for now always `"bundled"`.
   */
  origin: "bundled" | "plugin" | "sidecar";
}

/**
 * Whole-daemon catalog. Keyed by method name so lookups are O(1).
 */
export interface MethodCatalog {
  methods: Record<string, MethodCatalogEntry>;
}

// ── Plugin protocol (Phase 8.5 / PR 2) ─────────────────────────────
//
// Two extension paths plug into this catalog from outside the bundled
// method set:
//
//   1. In-process plugins ("X-2") — JS modules dropped into a plugins
//      directory. Daemon scans on startup, imports each, and registers
//      its `name` + `execute` next to the bundled methods.
//   2. Sidecar plugins ("X-1") — separate HTTP daemons that implement
//      `GET /v1/methods` and `POST /v1/rpc`. Configuration maps a
//      method-name prefix to a sidecar URL; the registry forwards
//      matching RPC calls.
//
// Both produce `MethodCatalogEntry` rows with `origin = "plugin"` or
// `"sidecar"` so the dashboard's manifest editor can badge them
// distinctly (e.g. a 🔌 icon for user-contributed sources).

import type { JsonObject } from "../types.js";

/**
 * One in-process plugin module — what a `plugins/*.js` (or `.mjs`)
 * file is expected to export as its default export.
 *
 * Plugin authors write:
 * ```ts
 * import type { InProcessPlugin } from "policy-rpc/methods/catalog";
 * const plugin: InProcessPlugin = {
 *   catalog: { name: "risk.score", params: {...}, returns: {...}, origin: "plugin" },
 *   async execute(params) { return { value: 42 }; },
 * };
 * export default plugin;
 * ```
 *
 * The plugin loader validates the shape and refuses to register
 * malformed modules — see `plugin-loader.ts`.
 */
export interface InProcessPlugin {
  /** Catalog entry describing params + returns. `origin` SHOULD be `"plugin"`. */
  catalog: MethodCatalogEntry;
  /** Execute the method. Same contract as registry's internal dispatcher. */
  execute(params: unknown): Promise<JsonObject>;
}

/**
 * Sidecar configuration entry — declares where to forward RPC calls
 * whose method name starts with a given prefix.
 *
 * The daemon queries each sidecar's `GET /v1/methods` at startup to
 * learn what method names the sidecar actually serves. Sidecars that
 * are down at startup are logged and skipped; the daemon still boots
 * (best-effort discovery, fail-open).
 */
export interface SidecarConfig {
  /** Human-readable label shown in logs and the catalog UI. */
  name: string;
  /** Base URL of the sidecar daemon, e.g. `"http://localhost:9001"`. */
  url: string;
  /**
   * Method-name prefix this sidecar owns, e.g. `"risk."`. Any catalog
   * entry the sidecar publishes whose `name` doesn't start with this
   * prefix is rejected — keeps the URL→namespace mapping explicit and
   * stops a misconfigured sidecar from shadowing bundled methods.
   */
  methodPrefix: string;
}
