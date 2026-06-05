// scopeball default v2 policy — Morpho setAuthorization operator grant (TARGET FORM).
// Source: policy-factory cycle 2026-06-03-morpho (protocol scope = Morpho only).
//
// BLOCKED-BY-ACTION-MODEL: there is NO Lending::Action::"SetAuthorization" variant
// today, so this .cedar/manifest is the TARGET shape and will NOT pass the gate
// until the action variant + registry selectors + sign-resolver mapping land.
// See proposed_action.md (action-model + registry + sign-resolver supplement) and
// proposed_method.json (external operator allowlist).
//
// Demand (mechanism-confirmed, anticipatory):
//   - Morpho docs: setAuthorization "Sets the authorization for `authorized` to
//     manage `msg.sender`'s positions"; setAuthorizationWithSig EIP-712 sig "can
//     then be submitted onchain by _any_ address"
//     (docs.morpho.org/get-started/resources/contracts/morpho).
//   - This is a setApprovalForAll-grade authority across ALL the user's Morpho
//     positions; the gasless EIP-712 form looks like a harmless signature (no
//     visible approve tx) — the classic Permit/Permit2 phishing shape.
//   - NOTE: no in-the-wild Morpho-auth phishing campaign documented yet; current
//     Morpho phishing runs through frontend/approval drainers (ScamSniffer: Morpho
//     #2 most-impersonated brand, 31%, 2026). Telegram scamsniffer morpho-keyword
//     search returned 0 IOC on 2026-06-03. Ship as WARN (latent high-value vector).
//
// EXTERNAL. The operator-recognition Bool is produced off-wallet (allowlist of
// known Morpho contracts: bundlers/adapters/known integrations). cedar reads the
// grant direction (isAuthorized) as a base field + the custom Bool.
@id("morpho-set-authorization-operator-warn")
@severity("warn")
@reason("This grants an operator full control to move, borrow against and withdraw ALL your Morpho positions — like setApprovalForAll. Only authorize contracts you trust")
forbid(principal, action == Lending::Action::"SetAuthorization", resource)
when {
  context.isAuthorized
  && context has custom
  && context.custom has operatorNotRecognized
  && context.custom.operatorNotRecognized
};
