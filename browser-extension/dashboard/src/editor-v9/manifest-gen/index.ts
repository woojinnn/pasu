/**
 * Auto-generate a policy's enrichment manifest from its IR. Public entry for the
 * editor save path. See docs/design/editor-manifest-autogen.md.
 */
export { generateManifest, collectCustomFields } from "./generate";
export type {
  GeneratedManifest,
  GenError,
  GenResult,
  ManifestOutput,
  ManifestRpc,
} from "./generate";
export {
  ENRICHMENT_FIELDS,
  type EnrichmentField,
  type EnrichmentRegistry,
  type ParamSpec,
  type CustomType,
} from "./registry";
