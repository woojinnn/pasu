// scopeball default v2 policy — Lido stETH safety. Demand signal (Telegram / X +
// Lido help docs): rebasing stETH sent to a smart contract usually STOPS accruing
// rebases — most contracts hold a fixed token balance and the daily Lido rebase
// accrues to the contract, not to you — and can be stranded entirely if the
// contract has no withdrawal path. Lido itself recommends sending / bridging the
// NON-rebasing wstETH instead. Sources: help.lido.fi bridging guide; "Uniswap,
// 1inch, Sushi aren't designed for rebasable tokens" (Bitget academy / Coin Bureau).
//
// Pure-Bool composition of two SHIPPED facts (no new state surface):
//   - `token.swap_special_token` on the transferred token → `special` = the held
//     token is rebasing (TokenKind::Wrapped{peg_kind: Rebasing}; stETH).
//   - `external.recipient_is_contract` on the recipient → EOA-vs-contract by code
//     hash (a live on-chain lookup, outside wallet state).
// Cedar only ANDs the two Bools. Both calls are `optional` → until the providers
// (rebasing classification / code-hash host) ship, the `has` guards are false and
// the policy no-ops (fail-open) rather than SystemFail. Warn (not deny): some
// contracts ARE rebase-aware, so this is a strong nudge, not a hard block.
@id("lido-rebasing-steth-to-contract-warn")
@severity("warn")
@reason("You are sending rebasing stETH to a smart contract — most contracts do not credit Lido rebases, so you may stop earning rewards or strand the tokens. Send the non-rebasing wstETH instead.")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has tokenRebasing && context.custom.tokenRebasing
  && context.custom has recipientIsContract && context.custom.recipientIsContract
};
