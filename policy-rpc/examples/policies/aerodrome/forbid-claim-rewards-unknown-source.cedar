@id("default/aerodrome/forbid-claim-rewards-unknown-source")
@severity("warn")
@reason("claim_rewards without a known source address indicates adapter fault — claiming from an unresolved contract bypasses source-allow-list policies")
forbid (
  principal,
  action == Action::"claim_rewards",
  resource
) when {
  context.sourceAddress == ""
};
