-- 002_approvals — ERC20 / setApprovalForAll / Permit2 권한 컬렉션.
--
-- spec §4.4 의 ApprovalSet 매핑. wallet 단위 sparse 3개 테이블.
-- ERC721 *per-token* approve 는 token_holdings.approved_to 에 이미 nested.

-- ─── ERC20 allowance: (wallet, chain, token contract, spender) ──────────
CREATE TABLE approvals_erc20 (
  wallet_id     INTEGER NOT NULL REFERENCES wallets(id) ON DELETE CASCADE,
  chain         TEXT    NOT NULL,                  -- "eip155:1"
  token_address TEXT    NOT NULL,                  -- 0x... (token contract)
  spender       TEXT    NOT NULL,                  -- 0x... (allowance grantee)
  amount        TEXT    NOT NULL,                  -- U256 decimal string
  is_unlimited  INTEGER NOT NULL DEFAULT 0,        -- 1 = U256::MAX 또는 sufficiently_high
  last_set_at   INTEGER NOT NULL,                  -- unix sec
  PRIMARY KEY (wallet_id, chain, token_address, spender)
);

CREATE INDEX idx_approvals_erc20_spender   ON approvals_erc20(spender);
CREATE INDEX idx_approvals_erc20_unlimited ON approvals_erc20(is_unlimited) WHERE is_unlimited = 1;

-- ─── setApprovalForAll (ERC721 / ERC1155): (wallet, chain, collection, operator) ──
CREATE TABLE approvals_set_for_all (
  wallet_id  INTEGER NOT NULL REFERENCES wallets(id) ON DELETE CASCADE,
  chain      TEXT    NOT NULL,
  collection TEXT    NOT NULL,                     -- 0x... (NFT/1155 collection contract)
  operator   TEXT    NOT NULL,                     -- 0x... (operator address)
  set_at     INTEGER,                              -- unix sec, 옵션
  PRIMARY KEY (wallet_id, chain, collection, operator)
);

-- ─── Permit2 allowance: (wallet, chain, token, spender) → (amount, expiration, nonce) ──
CREATE TABLE approvals_permit2 (
  wallet_id     INTEGER NOT NULL REFERENCES wallets(id) ON DELETE CASCADE,
  chain         TEXT    NOT NULL,
  token_address TEXT    NOT NULL,
  spender       TEXT    NOT NULL,
  amount        TEXT    NOT NULL,                  -- U256 decimal
  expiration    INTEGER NOT NULL,                  -- unix sec (0 = expired/never)
  nonce         INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (wallet_id, chain, token_address, spender)
);

CREATE INDEX idx_approvals_permit2_expiration ON approvals_permit2(expiration);
