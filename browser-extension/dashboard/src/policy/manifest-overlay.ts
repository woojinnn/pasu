import type { ExtensionClient } from "@scopeball/sdk";
import type { OverlayField } from "./builder-wasm";

/**
 * Cedar type spellings the WASM overlay knows how to surface. The Rust
 * side accepts both scalar primitives (`Long`, `String`, `Bool`,
 * `decimal`, `Set<String>`, `Set<Long>`) AND record aliases
 * (`UsdValuation`, `WindowStats`, `Validity`, …) which it expands into
 * per-leaf fields via `policy_builder::aliases::record_leaves`.
 *
 * Anything outside this whitelist (e.g. a new `HookPermissions` alias
 * we haven't taught the builder about yet, or a manifest author typo)
 * is dropped at the TS boundary so we don't ship traffic the Rust side
 * will silently ignore. Mirror this set 1:1 with
 * `parse_overlay_cedar_type` + `record_leaves` over there.
 */
export const OVERLAY_KNOWN_TYPES: ReadonlySet<string> = new Set([
  // Scalar primitives — `parse_overlay_cedar_type` whitelist.
  "Long",
  "String",
  "Bool",
  "decimal",
  "Set<String>",
  "Set<Long>",
  // Record aliases — `record_leaves` whitelist. Without these, USD
  // valuations and 24h-stats fields from the bundled starter manifest
  // never make it past the TS filter, and the builder picker shows
  // only the scalar custom fields (Phase 8 carry-over bug).
  "UsdValuation",
  "WindowStats",
  "Validity",
  "AssetRef",
  "AmountConstraint",
  "AssetRefWithAmountConstraint",
  "TickRange",
  "Pool",
]);

/**
 * Pull manifest-installed custom fields for `action` out of the engine's
 * enriched schema and return them as a builder-WASM overlay. Returns
 * `undefined` when there's nothing to overlay so the caller drops the
 * overlay code path entirely (cheaper, easier to read in tests).
 *
 * Best-effort: if `getEnrichedSchema()` fails (e.g. no manifests
 * installed yet, transport error) we return `undefined` and the caller
 * proceeds against the static schema only.
 */
export async function loadOverlay(
  client: ExtensionClient,
  action: string,
): Promise<OverlayField[] | undefined> {
  try {
    const enriched = await client.getEnrichedSchema();
    const fields = enriched.customContexts?.[action] ?? [];
    const overlay = fields
      .filter((f) => OVERLAY_KNOWN_TYPES.has(f.cedar_type))
      .map<OverlayField>((f) => ({
        field: f.field,
        cedarType: f.cedar_type,
      }));
    return overlay.length > 0 ? overlay : undefined;
  } catch {
    return undefined;
  }
}

/**
 * Read the installed manifest for `action`. Returns `undefined` when no
 * manifest is installed for that action, or when the lookup fails. Used
 * by save flows that want to thread the manifest into `putRaw` so the
 * persistence layer keeps the policy ↔ manifest pairing alongside the
 * rule text.
 */
export async function loadInstalledManifest(
  client: ExtensionClient,
  action: string,
): Promise<unknown | undefined> {
  try {
    const { manifest } = await client.getManifest(action);
    return manifest ?? undefined;
  } catch {
    return undefined;
  }
}
