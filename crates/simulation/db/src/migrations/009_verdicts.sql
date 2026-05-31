-- 009_verdicts — Cedar policy verdict audit log.
--
-- One row per (state_delta × matched policy). A single TX evaluation may
-- produce multiple rows (e.g., "Slippage > 50bp" + "Recipient blocked" both
-- fire on one swap). Rows are immutable except for `user_decision`, which
-- the dashboard sets when a user manually approves or cancels a `warn`.
--
-- Denormalised columns (`policy_name`, `contract_symbol`, etc.) are kept
-- so historical audit rows stay readable even after the source policy is
-- renamed or deleted. The FK to `user_policies` is `ON DELETE SET NULL`
-- to preserve that contract.

CREATE TABLE verdicts (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,

  -- Links back to the source delta + wallet. delta_id may be NULL for
  -- verdicts derived from off-chain signature flows that don't land in
  -- state_deltas (rare; Phase 2 v1 always sets it).
  delta_id        INTEGER REFERENCES state_deltas(id) ON DELETE CASCADE,
  wallet_id       INTEGER NOT NULL REFERENCES wallets(id) ON DELETE CASCADE,
  policy_id       INTEGER REFERENCES user_policies(id) ON DELETE SET NULL,

  -- Severity = the policy's configured severity (deny/warn/info).
  -- Verdict   = the runtime outcome (pass/warn/fail).
  severity        TEXT    NOT NULL CHECK(severity IN ('deny', 'warn', 'info')),
  verdict         TEXT    NOT NULL CHECK(verdict  IN ('pass', 'warn', 'fail')),

  ts              INTEGER NOT NULL,                  -- unix sec, evaluation time

  -- Origin / decoding context (denormalised — survives policy renames).
  dapp_origin     TEXT,                              -- "app.uniswap.org"
  method          TEXT,                              -- "eth_sendTransaction"
  decoded_fn      TEXT,                              -- "swapExactTokensForTokens"
  contract_addr   TEXT,
  contract_symbol TEXT,
  selector_sig    TEXT,                              -- "0x38ed1739"
  selector_decoded TEXT,
  policy_name     TEXT,                              -- "Max slippage 0.5%"

  -- Reason shown to the user, both locales (Decision #8: server ships both).
  reason_ko       TEXT,
  reason_en       TEXT,

  -- User's resolution for warn-level rows. NULL until they act.
  user_decision   TEXT CHECK(user_decision IN ('trusted', 'cancelled')),
  decided_at      INTEGER
);

CREATE INDEX idx_verdicts_wallet_ts ON verdicts(wallet_id, ts DESC);
CREATE INDEX idx_verdicts_verdict   ON verdicts(verdict);
CREATE INDEX idx_verdicts_origin    ON verdicts(dapp_origin) WHERE dapp_origin IS NOT NULL;
CREATE INDEX idx_verdicts_policy    ON verdicts(policy_id)   WHERE policy_id   IS NOT NULL;
CREATE INDEX idx_verdicts_delta     ON verdicts(delta_id)    WHERE delta_id    IS NOT NULL;
