-- 001_initial — scopeball Phase 1 schema.
--
-- 사용자당 1 DB 파일 모델. 한 DB 안에:
--   * user_profile (singleton)
--   * wallets / wallet_chains
--   * tokens (글로벌 카탈로그)
--   * token_holdings (지갑별 sparse)
--   * block_heights
--   * state_deltas (live / backfill 통합 lifecycle 로그)
--
-- approvals_* / positions_* 는 Phase 2 에서 별도 마이그레이션으로 추가.

-- ───────────────────────────────────────────────────────────────────────
-- user_profile — 이 파일을 소유한 사용자 (singleton, id = 1)
-- ───────────────────────────────────────────────────────────────────────
CREATE TABLE user_profile (
  id            INTEGER PRIMARY KEY CHECK (id = 1),
  user_id       TEXT    NOT NULL UNIQUE,        -- "google:117234567890" 등 OAuth provider:sub
  email         TEXT,
  display_name  TEXT,
  settings_json TEXT    NOT NULL DEFAULT '{}',  -- 사용자 설정 (retention 등)
  created_at    INTEGER NOT NULL                -- unix sec
);

-- ───────────────────────────────────────────────────────────────────────
-- wallets — 사용자가 보유 / 추적하는 EVM 주소들
-- ───────────────────────────────────────────────────────────────────────
CREATE TABLE wallets (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  address     TEXT    NOT NULL UNIQUE,         -- 0x... 소문자 정규화
  label       TEXT,                            -- "main" / "cold" / "watching:vitalik"
  is_owned    INTEGER NOT NULL DEFAULT 1,      -- 1 = 본인 (서명 가능), 0 = 추적만
  created_at  INTEGER NOT NULL,
  archived_at INTEGER                          -- soft delete
);

CREATE INDEX idx_wallets_address ON wallets(address);

-- 한 wallet 이 추적하는 chain 들 (CAIP-2).
CREATE TABLE wallet_chains (
  wallet_id INTEGER NOT NULL REFERENCES wallets(id) ON DELETE CASCADE,
  chain     TEXT    NOT NULL,                   -- "eip155:1" 등
  PRIMARY KEY (wallet_id, chain)
);

-- ───────────────────────────────────────────────────────────────────────
-- tokens — 글로벌 카탈로그 (지갑 무관). USDC 를 5개 지갑이 들고 있어도 tokens row 1개.
--
-- token_hash = BLAKE3(canonical_json(TokenKey))[..16].
-- TokenKey enum 의 variant 마다 nullable 컬럼이 다르게 채워짐.
-- ───────────────────────────────────────────────────────────────────────
CREATE TABLE tokens (
  token_hash     BLOB    PRIMARY KEY,          -- 16 bytes
  standard       TEXT    NOT NULL,             -- "native" | "erc20" | "erc721" | "erc1155"
  chain          TEXT    NOT NULL,             -- "eip155:1"
  address        TEXT,                         -- erc20 의 contract 주소
  contract       TEXT,                         -- erc721 / erc1155 의 collection 주소
  token_id       TEXT,                         -- erc721 / erc1155 의 token id (U256 decimal)
  symbol_cache   TEXT,                         -- "USDC" — 조회 편의 (sync 가 갱신)
  decimals_cache INTEGER,                      -- 6 — 조회 편의
  first_seen_at  INTEGER NOT NULL
);

CREATE INDEX idx_tokens_standard_chain ON tokens(standard, chain);

-- ───────────────────────────────────────────────────────────────────────
-- token_holdings — (wallet, token) 의 잔고 + price LiveField (하이브리드)
-- ───────────────────────────────────────────────────────────────────────
CREATE TABLE token_holdings (
  wallet_id      INTEGER NOT NULL REFERENCES wallets(id)  ON DELETE CASCADE,
  token_hash     BLOB    NOT NULL REFERENCES tokens(token_hash),

  -- Balance (Fungible | Owned)
  balance_form     TEXT NOT NULL CHECK(balance_form IN ('fungible', 'owned')),
  balance_amount   TEXT,                       -- U256 decimal string, fungible 만
  committed_form   TEXT NOT NULL CHECK(committed_form IN ('fungible', 'owned')),
  committed_amount TEXT,

  -- ERC721 개별 NFT approve
  approved_to TEXT,

  -- price_usd: LiveField<Price> — 평탄화된 핵심 컬럼 + JSON source
  price_value           TEXT,                    -- "0.99955" 등
  price_synced_at       INTEGER,                 -- unix sec
  price_ttl_sec         INTEGER,
  price_confidence_bp   INTEGER,                 -- Confidence.deviation_bp (u32)
  price_source_json     TEXT,                    -- DataSource JSON (kind + variant data)

  -- meta
  last_synced_at         INTEGER NOT NULL,
  primitives_source_json TEXT    NOT NULL,     -- 잔고 출처 (DataSource JSON)

  PRIMARY KEY (wallet_id, token_hash)
);

-- 모든 사용자 의 stale price 쿼리 가능.
CREATE INDEX idx_holdings_stale_price
  ON token_holdings(price_synced_at)
  WHERE price_synced_at IS NOT NULL;

-- ───────────────────────────────────────────────────────────────────────
-- block_heights — wallet 별 chain 별 마지막 관찰 block
-- ───────────────────────────────────────────────────────────────────────
CREATE TABLE block_heights (
  wallet_id   INTEGER NOT NULL REFERENCES wallets(id) ON DELETE CASCADE,
  chain       TEXT    NOT NULL,
  height      INTEGER NOT NULL,                -- u64 까지 i64 로 안전
  observed_at INTEGER NOT NULL,
  PRIMARY KEY (wallet_id, chain)
);

-- ───────────────────────────────────────────────────────────────────────
-- state_deltas — 모든 tx 시도의 lifecycle 로그
--
-- source = 'live' (익스텐션이 가로챔) | 'backfill' (chain scan 으로 발견)
-- status:
--   'live' 의 경우:
--     'predicted' → 시뮬레이션만, 사용자 사인 안 함 (verdict 에 따라 deny 도 여기)
--     'pending'   → 사인 + 브로드캐스트, 멤풀
--     'confirmed' → 블록 확정 + state 테이블 변경 완료
--     'failed'    → revert / 누락
--   'backfill' 의 경우:
--     'historical' → 과거 chain 에서 가져옴
--   공통:
--     'rolled_back' → 리오그 등으로 무효화
-- ───────────────────────────────────────────────────────────────────────
CREATE TABLE state_deltas (
  id                            INTEGER PRIMARY KEY AUTOINCREMENT,
  wallet_id                     INTEGER NOT NULL REFERENCES wallets(id) ON DELETE CASCADE,

  source                        TEXT    NOT NULL CHECK(source IN ('live', 'backfill')),
  status                        TEXT    NOT NULL,

  -- 시각
  created_at                    INTEGER NOT NULL,    -- 첫 관찰 / 시뮬 시각
  signed_at                     INTEGER,             -- 사인 시각 (live 만)
  confirmed_at                  INTEGER,             -- 블록 timestamp

  -- action 식별
  action_domain                 TEXT    NOT NULL,    -- "lending" / "amm" / ...
  action_kind                   TEXT    NOT NULL,    -- "borrow" / "swap" / ...
  submitter                     TEXT    NOT NULL,
  nature_kind                   TEXT    NOT NULL,    -- "onchain_tx" | "offchain_sig"
  chain                         TEXT,
  nonce                         INTEGER,
  action_json                   TEXT    NOT NULL,    -- 전체 Action JSON

  -- predicted (live 만 — reducer + Cedar 결과)
  predicted_delta_json          TEXT,
  predicted_verdict             TEXT,                -- "allow" | "warn" | "deny"
  predicted_verdict_reasons_json TEXT,

  -- realized (live confirmed / backfill)
  tx_hash                       TEXT,
  sig_hash                      TEXT,
  realized_block_number         INTEGER,
  realized_delta_json           TEXT,

  -- 실패 / 롤백 사유
  failure_reason                TEXT,
  rolled_back_reason            TEXT,

  -- replay 검증용
  pre_state_hash                BLOB,
  post_state_hash               BLOB
);

CREATE INDEX idx_deltas_wallet_time ON state_deltas(wallet_id, created_at DESC);
CREATE INDEX idx_deltas_status      ON state_deltas(status);
CREATE INDEX idx_deltas_source      ON state_deltas(source);
CREATE INDEX idx_deltas_tx_hash     ON state_deltas(tx_hash) WHERE tx_hash IS NOT NULL;
