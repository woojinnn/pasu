// blocks: sending more than the $10k USD cap of native value to a watched protocol
//         contract in a single un-decoded (catch-all) interaction.
// why:    per-protocol budget — a user can cap how much native value flows to a
//         specific (possibly newly-trusted) contract per action; combined with the
//         baked target set this gives a spend ceiling on an opaque interaction.
//         source: Fireblocks transaction-authorization USD limits.
// bucket: wallet/usd-cap
// tags: bucket:wallet cat:usd-cap enrichment method:oracle.usd_value action-family:usd-cap
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// VOCAB NOTE: Core::Unknown exposes only `target`/`chain`/`calldata`/`value` (VOCAB
// §4 cross-cutting). `value` is the native U256-hex value attached; the manifest
// binds it as the priced amount and `$.action.chain` lets the planner derive the
// native asset for that chain. The target set below is a manifest-baked allowlist of
// watched protocol contracts (replace the placeholders with real lowercase addresses);
// the cap only fires for those targets. The policy reads `context.target` (always
// present on Unknown) and the projected USD leaf, so it compiles.
// ENRICHMENT — dormant until the /v1/rpc dispatcher serves `oracle.usd_value`.
@id("per-protocol-spend-cap")
@severity("warn")
@reason("Large native spend to a watched contract (over $10k) — confirm the interaction")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  ["0x1111111111111111111111111111111111111111",
   "0x2222222222222222222222222222222222222222",
   "0x3333333333333333333333333333333333333333"].contains(context.target)
  && context has custom
  && context.custom has spendUsd
  && context.custom.spendUsd.greaterThan(decimal("10000.0000"))
};
