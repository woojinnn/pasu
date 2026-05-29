-- 003_positions — protocol-tracked 권리/상태 (token 형태 아님).
--
-- spec §5 의 Position 의 모든 variant 를 한 테이블에 generic JSON 으로 저장:
--   * LendingAccount (Aave / Compound / Morpho)
--   * PerpPosition (Hyperliquid / dYdX / GMX)
--   * AirdropClaim
--   * LaunchpadAllocation
--   * VestingSchedule
--
-- variant 별 디테일이 다양해 column 평탄화 대신 JSON. 자주 조회되는
-- (protocol, kind, market) 만 검색용 인덱스.

CREATE TABLE positions (
  id                       INTEGER PRIMARY KEY AUTOINCREMENT,
  wallet_id                INTEGER NOT NULL REFERENCES wallets(id) ON DELETE CASCADE,

  -- 식별
  position_id              TEXT    NOT NULL,             -- protocol 이 부여한 id 또는 우리가 생성
  protocol                 TEXT    NOT NULL,             -- "aave-v3" / "hyperliquid" / "uniswapx" 등
  chain                    TEXT,                          -- "eip155:1" 또는 NULL (off-chain venue)
  kind                     TEXT    NOT NULL CHECK(kind IN (
    'lending_account', 'perp_position',
    'airdrop_claim', 'launchpad_allocation', 'vesting_schedule'
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

CREATE INDEX idx_positions_wallet      ON positions(wallet_id);
CREATE INDEX idx_positions_kind        ON positions(wallet_id, kind);
CREATE INDEX idx_positions_protocol    ON positions(protocol);
