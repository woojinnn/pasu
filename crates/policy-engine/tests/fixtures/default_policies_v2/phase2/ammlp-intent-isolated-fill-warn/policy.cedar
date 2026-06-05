// scopeball default v2 policy — explorer id: AMMLP-6 (loose buy_min with no
// solver competition → isolated, filler-favourable fill).
//
// Intent orders rely on solver competition for a good price; with near-zero
// competing orders, a single solver can take your loose `buyMin` at a cheap
// price. `competingOrders` is a base `Long` on `Amm::SignIntentOrderContext`, but
// the "buyMin << market" half needs an oracle, and the two must be COMBINED — so
// the method `intent.isolated_fill_risk` joins the competition depth with the
// market gap and emits the Bool `isolatedFillRisk` (true only when competition is
// thin AND the floor is loose, which keeps false positives down). `custom` and
// the call are optional, so the path is `has`-guarded.
@id("ammlp-intent-isolated-fill-warn")
@severity("warn")
@reason("This order has a loose floor and almost no competing solvers — it may fill cheap to a single filler")
forbid(principal, action == Amm::Action::"SignIntentOrder", resource)
when {
  context has custom
  && context.custom has isolatedFillRisk
  && context.custom.isolatedFillRisk
};
