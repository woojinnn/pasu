@id("user/allowance-must-cover-input")
@severity("warn")
@reason("Existing allowance is below the swap input; an approve is required")
forbid (principal, action == Action::"swap", resource)
when {
  context has "allowanceCoversInput"
  && !context.allowanceCoversInput
};
