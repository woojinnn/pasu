// dambi default v2 policy — explorer id: STK-02 (unstake withdrawal
// cooldown heads-up).
//
// STATE-DERIVED, not an action-field. Unstaking decodes as Core::Unknown (no
// dedicated staking domain), so the action alone can't tell you the position
// you're exiting is lock-bound. The state model already carries the lock shape
// on the stake receipt (TokenKind::StakeReceipt.unlock — Cliff|Linear|Cooldown)
// and which protocol it belongs to. NEW method `stake.unlock_eta` matches the
// unstake target to the held StakeReceipt and returns whether a lockup applies
// (`stakeHasLockup` Bool) and the seconds until withdrawable (`stakeUnlockEtaSecs`
// Long), both projected into context.custom. Lido 1–5 days, EigenLayer +7-day
// escrow — funds and rewards pause meanwhile. Defined-not-wired → `optional`
// (fail-open). Pure heads-up — warn on any locked unstake (no time threshold:
// the lockup itself is the signal).
@id("stk-unstake-cooldown-warn")
@severity("warn")
@reason("Unstaking here starts a withdrawal cooldown — your funds will be locked until the unlock time")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has stakeHasLockup
  && context.custom.stakeHasLockup
};
