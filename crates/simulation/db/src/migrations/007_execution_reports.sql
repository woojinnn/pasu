-- 007_execution_reports — post-policy execution lifecycle audit.
--
-- `evaluate` produces a prediction; this table records what happened after the
-- policy decision point (wallet signed, tx submitted, venue accepted/rejected).
-- Canonical wallet state is still updated only by authoritative chain/venue
-- sync. `wallet_id` is nullable because Hyperliquid agent-key requests can be
-- observed from a `/exchange` response before the extension can attribute the
-- venue account to a tracked master wallet.

CREATE TABLE execution_reports (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  wallet_id       INTEGER REFERENCES wallets(id) ON DELETE SET NULL,

  evaluation_id   TEXT,
  action_index    INTEGER,

  stage           TEXT NOT NULL CHECK(stage IN ('wallet', 'onchain', 'venue', 'failure')),
  outcome_kind    TEXT NOT NULL CHECK(outcome_kind IN (
                    'wallet_rejected',
                    'wallet_signed',
                    'onchain_submitted',
                    'onchain_confirmed',
                    'venue_submitted',
                    'venue_accepted',
                    'venue_rejected',
                    'failed'
                  )),

  chain           TEXT,
  tx_hash         TEXT,
  signature       TEXT,
  venue           TEXT,
  venue_order_id  TEXT,
  client_order_id TEXT,
  reason          TEXT,

  raw_json        TEXT NOT NULL,
  metadata_json   TEXT NOT NULL DEFAULT '{}',

  created_at      INTEGER NOT NULL,
  reconciled_at   INTEGER
);

CREATE INDEX idx_execution_reports_wallet_unreconciled
  ON execution_reports(wallet_id, created_at)
  WHERE wallet_id IS NOT NULL AND reconciled_at IS NULL;

CREATE INDEX idx_execution_reports_evaluation
  ON execution_reports(evaluation_id)
  WHERE evaluation_id IS NOT NULL;

CREATE INDEX idx_execution_reports_tx_hash
  ON execution_reports(tx_hash)
  WHERE tx_hash IS NOT NULL;

CREATE INDEX idx_execution_reports_venue_order
  ON execution_reports(venue, venue_order_id)
  WHERE venue_order_id IS NOT NULL;
