-- 004_pending_txs — 서명만 하고 아직 체인에 안 올라간 의도들.
--
-- state_deltas 의 status='pending' 과 구분:
--   * state_deltas.pending = 이미 mempool 에 올라간 onchain tx (tx_hash 존재)
--   * pending_txs         = offchain signature 만, 아직 누군가 resolve/relay 해야
--
-- 예: UniswapX intent, Permit2, Safe 멀티시그 사전 서명, 1inch fusion order.

CREATE TABLE pending_txs (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  wallet_id       INTEGER NOT NULL REFERENCES wallets(id) ON DELETE CASCADE,

  -- 식별
  sig_hash        TEXT NOT NULL,                     -- 서명 자체의 hash (EIP-712 digest 등)
  nature          TEXT NOT NULL,                     -- "uniswapx_intent" / "permit2" / "safe_pre_sign" / "1inch_fusion"
  chain           TEXT,                              -- 적용될 체인 (옵션, off-chain venue 면 NULL)

  -- 의도 정보
  action_json     TEXT NOT NULL,                     -- 서명한 의도의 전체 spec
  deadline        INTEGER,                           -- unix sec, expire 시각
  nonce_key       TEXT,                              -- Permit2 word/bit 등 replay 방지 키

  -- 관찰
  signed_at       INTEGER NOT NULL,
  matched_tx_hash TEXT,                              -- 매칭되어 체인 올라간 후 채워짐 (state_deltas FK 처럼)
  expired_at      INTEGER,                           -- 발견된 만료 시점
  cancelled_at    INTEGER,                           -- 사용자가 명시적으로 취소

  status          TEXT NOT NULL CHECK(status IN (
    'awaiting',     -- 서명만 했고 매처 / 리졸버 / 다른 서명자 대기
    'matched',      -- onchain 으로 실현됨 (matched_tx_hash 채워짐)
    'expired',      -- deadline 지남
    'cancelled'     -- 사용자 취소
  )) DEFAULT 'awaiting',

  UNIQUE(wallet_id, sig_hash)
);

CREATE INDEX idx_pending_txs_wallet_status ON pending_txs(wallet_id, status);
CREATE INDEX idx_pending_txs_deadline      ON pending_txs(deadline) WHERE deadline IS NOT NULL;
CREATE INDEX idx_pending_txs_matched       ON pending_txs(matched_tx_hash) WHERE matched_tx_hash IS NOT NULL;
