-- 006_hyperliquid_account_kind — allow the `hyperliquid_account` position kind.
--
-- 003_positions defined `kind` with a CHECK enumerating the original 5 variants.
-- The new PositionKind::HyperliquidAccount serializes to kind="hyperliquid_account",
-- which that CHECK rejects. SQLite cannot ALTER/drop a CHECK in place, so we
-- rebuild the (leaf) table with the widened CHECK, preserving all columns, the
-- UNIQUE constraint, the FK, and the three indexes. data_json is unchanged.

CREATE TABLE positions_new (
  id                       INTEGER PRIMARY KEY AUTOINCREMENT,
  wallet_id                INTEGER NOT NULL REFERENCES wallets(id) ON DELETE CASCADE,
  position_id              TEXT    NOT NULL,
  protocol                 TEXT    NOT NULL,
  chain                    TEXT,
  kind                     TEXT    NOT NULL CHECK(kind IN (
    'lending_account', 'perp_position',
    'airdrop_claim', 'launchpad_allocation', 'vesting_schedule',
    'hyperliquid_account'
  )),
  market                   TEXT,
  summary                  TEXT,
  data_json                TEXT    NOT NULL,
  primitives_synced_at     INTEGER NOT NULL,
  primitives_source_json   TEXT    NOT NULL,
  UNIQUE(wallet_id, protocol, position_id, chain)
);

INSERT INTO positions_new
  (id, wallet_id, position_id, protocol, chain, kind,
   market, summary, data_json, primitives_synced_at, primitives_source_json)
  SELECT id, wallet_id, position_id, protocol, chain, kind,
         market, summary, data_json, primitives_synced_at, primitives_source_json
  FROM positions;

DROP TABLE positions;
ALTER TABLE positions_new RENAME TO positions;

CREATE INDEX idx_positions_wallet      ON positions(wallet_id);
CREATE INDEX idx_positions_kind        ON positions(wallet_id, kind);
CREATE INDEX idx_positions_protocol    ON positions(protocol);
