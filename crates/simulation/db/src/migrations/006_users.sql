-- Phase 4.2 (Auth): global users table.
--
-- This migration is applied ONLY to the global DB (`~/.scopeball/global.db`),
-- never to per-user DBs. The split is intentional: per-user DBs only hold
-- that user's wallet state and have no reason to know about other users.
--
-- `user_id` is a deterministic short hash of `email` (see auth::jwt::derive_user_id),
-- so re-logging-in always returns the same id.

CREATE TABLE users (
  user_id        TEXT    PRIMARY KEY,        -- e.g. "u_3a7f8c91b2d4"
  email          TEXT    NOT NULL UNIQUE,    -- canonical, lower-cased
  provider       TEXT    NOT NULL,           -- "google" | "github" | …
  created_at     INTEGER NOT NULL,           -- unix sec
  last_login_at  INTEGER NOT NULL            -- unix sec, bumped on every successful auth
);

CREATE INDEX idx_users_email ON users(email);
