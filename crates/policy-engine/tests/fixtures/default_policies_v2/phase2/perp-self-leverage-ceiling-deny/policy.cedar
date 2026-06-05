// 자기방위 스위트 — 자가 레버리지 천장. 사용자가 콜드 상태에서 정한 최대 레버리지(여기선 설치값 5배)를
// 넘는 진입을 차단. perp.order_leverage(구현됨, jp-perp-leverage-cap-2x-deny와 동일 패턴)가 orderLeverage를
// decimal로 투영. threshold(5배)는 사용자 precommit config(설치 시 선택) — 데모 빌드의 고정값.
// 오늘 빌드가능(implemented method). state 불필요.
@id("perp-self-leverage-ceiling-deny")
@severity("deny")
@reason("이 진입은 당신이 스스로 정한 레버리지 천장(5배)을 넘습니다. 자기방위 스위트가 막습니다 — 천장은 맑은 정신일 때 정한 값입니다.")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
when {
  context has custom
  && context.custom has orderLeverage
  && context.custom.orderLeverage.greaterThan(decimal("5.0000"))
};
