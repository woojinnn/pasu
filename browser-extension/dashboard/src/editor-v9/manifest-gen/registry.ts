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

export const ENRICHMENT_FIELDS: EnrichmentRegistry = {
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

  // Input token amount in token-native nano, served IN-PROCESS by the host's
  // pure `token.normalize_to_nano`. `decimals` is a literal (USDC = 6), so a
  // policy using this MUST gate `tokenIn` to USDC by address — otherwise a
  // non-6-decimals token is mis-scaled. Generalizing needs decimals threaded
  // into the lowered context (a separate, larger change).
  inputAmountNano: {
    type: "Long",
    label: { ko: "입력 토큰 수량 (USDC, nano)", en: "Input token amount (USDC, nano)" },
    appliesTo: ["swap"],
    method: "token.normalize_to_nano",
    projection: "$.result.nano",
    params: {
      amount: "$.action.direction.amountIn",
      decimals: { literal: 6 },
    },
    note: "decimals=6 (USDC only) — gate this policy to USDC by tokenIn address.",
  },
};
