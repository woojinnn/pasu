// scopeball default v2 policy — explorer id: B1 / ALLOC (allocation band).
// SAME fold fact `portfolio.group_pct`, group_axis=category: warn when buying
// into a category bucket that is already over its target allocation band. The
// "band max" is user config (target-allocation state is a later enrichment).
@id("alloc-bucket-overweight-warn")
@severity("warn")
@reason("This category bucket is already over its target allocation band")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has bucketPctAfter
  && context.custom.bucketPctAfter.greaterThan(decimal("50.0000"))
};
