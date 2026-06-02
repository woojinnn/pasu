/**
 * # Cedar block IR — public API
 *
 * Everything a block editor needs to render and edit Cedar policies without
 * touching raw Cedar text or EST. See {@link PolicyIR} / {@link Expr} in `./ir`
 * for the full node reference and a rendering example.
 *
 * `policy_text_to_est_json` / `est_json_to_policy_text` below are the Rust **WASM
 * exports** (reached through the extension bridge); only the pure TS helpers
 * (`estToBlocks` / `blocksToEst` / schema) live in this module.
 *
 * ## Read — Cedar text → blocks
 * ```ts
 * import { estToBlocks, type PolicyIR } from "../cedar/blocks";
 *
 * const { policies } = JSON.parse(policy_text_to_est_json(cedarText)); // WASM
 * const ir: PolicyIR = estToBlocks(policies[0].est, null);             // null = no schema annotations
 * renderPolicy(ir);
 * ```
 *
 * ## Write — blocks → Cedar text
 * ```ts
 * import { blocksToEst } from "../cedar/blocks";
 *
 * const est = blocksToEst(ir);                                          // throws on unfilled holes
 * const { text } = JSON.parse(est_json_to_policy_text(JSON.stringify(est))); // WASM
 * ```
 *
 * ## Optional — schema-aware field styling
 * ```ts
 * import { descriptorFromCustomTypes } from "../cedar/blocks";
 *
 * // `custom_types` comes from the WASM preview_custom_schema_json export.
 * const schema = descriptorFromCustomTypes(custom_types);
 * const ir = estToBlocks(est, schema); // attr nodes now carry type/source for styling
 * ```
 */

export { estToBlocks } from "./estToBlocks";
export { blocksToEst } from "./blocksToEst";
export { makeHole, replaceNode, extractParams, fillParams, childExprs } from "./params";
export {
  type SchemaField,
  type SchemaDescriptor,
  type PreviewCustomType,
  attrPath,
  classify,
  descriptorFromCustomTypes,
} from "./schema";
export type {
  Effect,
  EntityRef,
  Slot,
  Scope,
  ActionScope,
  VarName,
  LitType,
  BinaryOp,
  UnaryOp,
  SourceKind,
  LikePattern,
  Expr,
  Condition,
  PolicyIR,
  Expected,
  HoleNode,
  ParamConstraints,
  ParamSpec,
  PolicyTemplate,
  ParamFillValue,
  ParamError,
} from "./ir";
