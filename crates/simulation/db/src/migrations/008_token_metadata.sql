-- 007 — token metadata (logo / website / description / coingecko id).
--
-- Phase 10. The orchestrator fills these from CoinGecko's `/coins/{platform}/contract/{address}`
-- endpoint when a wallet is first added; UI uses them to render token icons
-- and tooltips. Lookup is best-effort — unknown tokens stay NULL.

ALTER TABLE tokens ADD COLUMN logo_url TEXT;
ALTER TABLE tokens ADD COLUMN website_url TEXT;
ALTER TABLE tokens ADD COLUMN description TEXT;
ALTER TABLE tokens ADD COLUMN coingecko_id TEXT;
ALTER TABLE tokens ADD COLUMN metadata_synced_at INTEGER;
