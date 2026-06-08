-- Action-based `category` for market policy listings.
--
-- Distinct from `domain` (which is protocol-flavoured and drives card colour):
-- `category` answers "what action does this policy guard?" and is derived from
-- the policy manifest's `trigger.action.tag`. The 12-value taxonomy:
--   approvals · signing · transfer · swap · derivatives · perps ·
--   liquidity · lending · rewards · governance · intents · others
-- NULL for sets (packages span categories).

ALTER TABLE market_listings ADD COLUMN IF NOT EXISTS category TEXT;

CREATE INDEX IF NOT EXISTS idx_listings_category
  ON market_listings(category, status) WHERE category IS NOT NULL;
