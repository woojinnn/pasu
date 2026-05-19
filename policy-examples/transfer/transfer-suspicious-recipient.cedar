@id("user/transfer-suspicious-recipient")
@severity("deny")
@reason("Transfer recipient 가 알려진 phishing / drainer 주소 set 에 포함 (PoC hardcoded list)")
forbid (
  principal,
  action == Action::"transfer",
  resource
) when {
  context.recipient == "0x0000000000000000000000000000000000000000" ||
  context.recipient == "0x000000000000000000000000000000000000dead" ||
  context.recipient == "0x1111111111111111111111111111111111111111"
};
