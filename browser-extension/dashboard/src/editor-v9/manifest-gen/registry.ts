/**
 * Enrichment-field registry — the binding the manifest generator needs but the
 * rest of the system lacks: "which method fills `context.custom.<field>`, with
 * what params, projected from where".
 *
 * Detection (`context.custom.X` in a policy), method param defaults, result
 * shapes, schema synthesis, and server-side execution all exist already; this
 * table is the one missing link. See docs/design/editor-manifest-autogen.md.
 *
 * Each entry is self-contained (all params spelled out) so the generator needs
 * no method-catalog merge. `type` is the Cedar `custom_context` spelling
 * (`decimal` lowercase for the decimal extension, `Long`/`Bool`/`String` for
 * primitives); the generator derives the capitalized projection `type`
 * (`Decimal`, …) from it.
 *
 * Selector roots (resolved at plan time): `$.root.*` = tx/chain, `$.action.*` =
 * the lowered action context (e.g. the swap context). A `{ literal: v }` param
 * is passed through verbatim instead of resolved.
 */

import { SEEDED_ENRICHMENT_FIELDS } from "./registry.generated";

/** A policy-RPC param value: a `$.`-selector, or a literal passed through. */
export type ParamSpec = string | { literal: number | string | boolean };

/** Cedar `custom_context` type spellings we support. */
export type CustomType = "decimal" | "Long" | "Bool" | "String";

export interface EnrichmentField {
  /** Cedar `custom_context` type spelling. */
  type: CustomType;
  /** Human label for the editor palette. */
  label: { ko: string; en: string };
  /** `action.tag` values this field can be used with (param selectors are
   *  action-shaped, so an entry is valid only for these actions). */
  appliesTo: string[];
  /** Enrichment method that produces the value. */
  method: string;
  /** `$.result.*` leaf projected into `context.custom.<field>`. */
  projection: string;
  /** Method params (selectors + literals). */
  params: Record<string, ParamSpec>;
  /** Honest caveat shown to authors (e.g. literal-decimals → asset-gate). */
  note?: string;
}

export type EnrichmentRegistry = Record<string, EnrichmentField>;

/** Hand-authored enrichment fields (selector-driven, server-backed). Merged with
 *  the fields lifted from the seeded default policies' manifests so the editor
 *  can regenerate a matching manifest for any of those policies. */
const HAND_AUTHORED_FIELDS: EnrichmentRegistry = {
  // Input USD value — fully selector-driven, served by the policy-server
  // `/evaluate` oracle.usd_value executor from synced price. Works as-is.
  inputUsd: {
    type: "decimal",
    label: { ko: "입력 USD 가치", en: "Input USD value" },
    appliesTo: ["swap"],
    method: "oracle.usd_value",
    projection: "$.result.usd",
    params: {
      chain_id: "$.root.chain_id",
      asset: "$.action.tokenIn.key.address",
      amount: "$.action.direction.amountIn",
    },
  },

  // Input token amount in token-native nano (`token_amount × 10^9`). Served by
  // `token.normalize_to_nano`: when `decimals` is omitted the host DEFERS to the
  // policy-server, which resolves the token's REAL decimals from the global token
  // registry by `(chain_id, asset)`. So this works for ANY token (USDC 6, ETH 18,
  // WBTC 8, …) with NO per-token gating — the old literal-6 USDC-only limitation
  // is gone.
  inputAmountNano: {
    type: "Long",
    label: { ko: "입력 토큰 수량 (nano)", en: "Input token amount (nano)" },
    appliesTo: ["swap"],
    method: "token.normalize_to_nano",
    projection: "$.result.nano",
    params: {
      amount: "$.action.direction.amountIn",
      chain_id: "$.root.chain_id",
      asset: "$.action.tokenIn.key.address",
    },
  },
};

/** All enrichment fields the editor knows. Hand-authored entries win on a
 *  name clash (none today). */
export const ENRICHMENT_FIELDS: EnrichmentRegistry = {
  ...SEEDED_ENRICHMENT_FIELDS,
  ...HAND_AUTHORED_FIELDS,
};
