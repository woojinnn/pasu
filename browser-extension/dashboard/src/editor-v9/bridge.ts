/**
 * Thin re-exports of the dashboard's cedar SW bridge so editor-v9 callers can
 * import from a single local module ("./bridge") instead of crossing into the
 * shared cedar/ package directly. Lets us swap the underlying implementation
 * later (e.g. inline a wasm worker) without touching every callsite.
 */

export { textToBlocks, blocksToText, fetchFieldCatalog } from "../cedar";
export type { SchemaDescriptor } from "../cedar/blocks";
