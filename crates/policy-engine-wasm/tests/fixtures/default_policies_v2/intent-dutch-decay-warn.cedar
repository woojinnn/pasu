// dambi default v2 policy — explorer id: UNI-12 (UniswapX Dutch order's
// guaranteed floor decays far below the expected fill).
//
// A Dutch order's terms worsen over time; if the guaranteed floor (`buyMin`) is
// far below the expected fill, the filler takes it cheap. The current
// `SignIntentOrder` schema has no startAmount/endAmount/decay-time fields, so the
// decay CURVE itself is unobservable — but `buyMin` and `expectedFillPrice` are
// present as HEX/decimal Strings. The method `intent.fill_vs_floor` computes the
// floor's shortfall from the expected fill (only for `orderKind == "dutch"`) and
// emits the enriched `Long` `fillVsFloorBp`. `custom` and the call are optional,
// so the path is `has`-guarded. 500 bp (5%) shortfall threshold here.
@id("intent-dutch-decay-warn")
@severity("warn")
@reason("This Dutch order's guaranteed floor is well below the expected fill price")
forbid(principal, action == Amm::Action::"SignIntentOrder", resource)
when {
  context has custom
  && context.custom has fillVsFloorBp
  && context.custom.fillVsFloorBp > 500
};
