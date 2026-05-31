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

  -- 식별
  position_id              TEXT    NOT NULL,             -- protocol 이 부여한 id 또는 우리가 생성
  protocol                 TEXT    NOT NULL,             -- "aave-v3" / "hyperliquid" / "uniswapx" 등
  chain                    TEXT,                          -- "eip155:1" 또는 NULL (off-chain venue)
  kind                     TEXT    NOT NULL CHECK(kind IN (
    'lending_account', 'perp_position',
    'airdrop_claim', 'launchpad_allocation', 'vesting_schedule',
    'hyperliquid_account'
  )),

  -- 검색 / UI 헤더에 빠르게 노출할 필드
  market                   TEXT,                          -- "ETH-USD" / "USDC pool" / "ARB airdrop"
  summary                  TEXT,                          -- "long 5x, HF 1.92" 같은 1줄 요약

  -- variant-specific data 통째 (Rust 의 PositionKind variant payload)
  data_json                TEXT    NOT NULL,

  -- 메타
  primitives_synced_at     INTEGER NOT NULL,              -- unix sec
  primitives_source_json   TEXT    NOT NULL,              -- DataSource JSON

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
