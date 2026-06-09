@id("hl-confirm-usd-send")
@severity("warn")
@reason("Sending USDC to another account on Hyperliquid — confirm before sending")
forbid(
  principal,
  action == Token::Action::"Erc20Transfer",
  resource
);
