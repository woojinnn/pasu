CREATE TABLE IF NOT EXISTS users (
  user_id TEXT PRIMARY KEY,
  email TEXT NOT NULL UNIQUE,
  provider TEXT NOT NULL,
  created_at BIGINT NOT NULL,
  last_login_at BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS wallets (
  user_id TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
  address TEXT NOT NULL,
  chains JSONB NOT NULL,
  label TEXT,
  owned BOOLEAN NOT NULL DEFAULT FALSE,
  archived BOOLEAN NOT NULL DEFAULT FALSE,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL,
  PRIMARY KEY (user_id, address)
);

CREATE TABLE IF NOT EXISTS wallet_states (
  user_id TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
  address TEXT NOT NULL,
  state_json JSONB NOT NULL,
  updated_at BIGINT NOT NULL,
  PRIMARY KEY (user_id, address)
);

CREATE TABLE IF NOT EXISTS sync_cursors (
  user_id TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
  wallet_address TEXT NOT NULL,
  source TEXT NOT NULL,
  cursor_json JSONB NOT NULL,
  updated_at BIGINT NOT NULL,
  PRIMARY KEY (user_id, wallet_address, source)
);
