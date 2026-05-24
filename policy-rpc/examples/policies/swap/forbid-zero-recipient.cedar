@id("user/forbid-zero-recipient")
@severity("deny")
@reason("Swap recipient must not be the zero address")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  context.recipient == "0x0000000000000000000000000000000000000000"
};
