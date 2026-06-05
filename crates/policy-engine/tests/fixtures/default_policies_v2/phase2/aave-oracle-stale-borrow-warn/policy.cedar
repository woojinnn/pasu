// scopeball default v2 policy — explorer id: AAVE-14 (borrow while the price
// oracle is stale / in L2 sequencer grace).
//
// EXTERNAL freshness fact. Borrowing on a frozen (stale) oracle or during L2
// sequencer-down grace can open a dangerous position priced off old data. The
// price LiveField's synced_at/ttl/confidence plus the sequencer-uptime sentinel
// express this freshness, surfaced via the manifest's `external.oracle_freshness`
// record method as `context.custom.oracleIsStale` (Bool) and
// `context.custom.oracleAgeSecs` (Long). Warn when stale OR older than the user's
// freshness window (3600s here). Defined-not-wired → `optional` (fail-open).
@id("aave-oracle-stale-borrow-warn")
@severity("warn")
@reason("Borrowing while the price oracle is stale or in L2 grace can misprice your position")
forbid(principal, action == Lending::Action::"Borrow", resource)
when {
  context has custom
  && context.custom has oracleIsStale
  && context.custom has oracleAgeSecs
  && (context.custom.oracleIsStale || context.custom.oracleAgeSecs > 3600)
};
