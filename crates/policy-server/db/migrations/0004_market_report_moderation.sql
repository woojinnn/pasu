-- Add moderation audit fields to marketplace reports.
ALTER TABLE market_reports ADD COLUMN IF NOT EXISTS resolved_by TEXT REFERENCES users(user_id) ON DELETE SET NULL;
ALTER TABLE market_reports ADD COLUMN IF NOT EXISTS resolved_at BIGINT;

CREATE INDEX IF NOT EXISTS idx_reports_resolved_by
  ON market_reports(resolved_by, resolved_at)
  WHERE resolved_by IS NOT NULL;
