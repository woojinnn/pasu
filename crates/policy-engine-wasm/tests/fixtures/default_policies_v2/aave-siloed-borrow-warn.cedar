// dambi default v2 policy — explorer id: AAVE-10 (borrowing a siloed asset
// locks you into single-asset borrowing).
//
// RESERVE-METADATA fact (NOT pure action-field). Some reserves are configured
// 'siloed' so borrowing them blocks borrowing anything else — an unexpected
// constraint. The shipped ReserveState base context carries utilization / caps /
// frozen / paused but NOT the siloed flag, so it can't be read as an action-field;
// the manifest declares `reserve.is_siloed`, projected into
// `context.custom.reserveIsSiloed` (Bool). Defined-not-wired → `optional`
// (fail-open). Pure heads-up — warn, not deny.
@id("aave-siloed-borrow-warn")
@severity("warn")
@reason("This asset is siloed — borrowing it locks you out of borrowing other assets")
forbid(principal, action == Lending::Action::"Borrow", resource)
when {
  context has custom
  && context.custom has reserveIsSiloed
  && context.custom.reserveIsSiloed
};
