-- Marketplace: user-published policies and sets.
--
-- Install model is copy-to-editor: receiving a listing copies its content into
-- the caller's local extension store; there is no ongoing link between the
-- installed copy and the source listing. `market_installs` is therefore an
-- event log used for popularity counts, not a state table.
--
-- Listings come in two kinds: 'policy' (a single Cedar policy) and 'set' (a
-- bundle whose member policies are snapshotted inline into each set version).
-- A set version is self-contained — its members carry their own cedar_text,
-- so member policies do not need to exist as standalone listings.

CREATE TABLE IF NOT EXISTS market_listings (
  id              UUID PRIMARY KEY,
  slug            TEXT NOT NULL UNIQUE,
  kind            TEXT NOT NULL,
  publisher_id    TEXT NOT NULL REFERENCES users(user_id),
  publisher_tier  TEXT NOT NULL DEFAULT 'community',
  display_name    JSONB NOT NULL,
  description     JSONB,

  -- Policy-only metadata. NULL for sets.
  domain          TEXT,
  intents         JSONB,
  severity        TEXT,

  status          TEXT NOT NULL DEFAULT 'published',
  current_version TEXT,
  forked_from     UUID REFERENCES market_listings(id),
  created_at      BIGINT NOT NULL,
  updated_at      BIGINT NOT NULL,

  CHECK (kind IN ('policy', 'set')),
  CHECK (publisher_tier IN ('official', 'verified', 'community')),
  CHECK (status IN ('pending', 'published', 'archived', 'rejected')),
  CHECK (severity IS NULL OR severity IN ('deny', 'warn')),
  CHECK (
    (kind = 'policy' AND domain IS NOT NULL AND severity IS NOT NULL)
    OR kind = 'set'
  )
);
CREATE INDEX idx_listings_kind_status ON market_listings(kind, status);
CREATE INDEX idx_listings_publisher ON market_listings(publisher_id);
CREATE INDEX idx_listings_domain ON market_listings(domain, status) WHERE domain IS NOT NULL;

-- Immutable per-version content. SemVer enforced both as a regex on `version`
-- and as separate INTEGER columns so ORDER BY major,minor,patch is index-friendly
-- (text sort of "10.0.0" vs "2.0.0" would otherwise be wrong).
CREATE TABLE IF NOT EXISTS market_listing_versions (
  listing_id   UUID NOT NULL REFERENCES market_listings(id) ON DELETE CASCADE,
  version      TEXT NOT NULL,
  major        INTEGER NOT NULL,
  minor        INTEGER NOT NULL,
  patch        INTEGER NOT NULL,

  -- Policy version body. Mutually exclusive with `members`.
  cedar_text   TEXT,
  manifest     JSONB,
  policy_tree  TEXT,

  -- Set version body: inline member snapshots. Shape per entry:
  --   { "slug": string, "displayName": string,
  --     "cedar_text": string, "manifest"?: any }
  members      JSONB,

  changelog    JSONB,
  published_at BIGINT NOT NULL,
  PRIMARY KEY (listing_id, version),

  CHECK (version ~ '^[0-9]+\.[0-9]+\.[0-9]+$'),
  CHECK (
    (cedar_text IS NOT NULL AND members IS NULL)
    OR (cedar_text IS NULL AND members IS NOT NULL)
  )
);
CREATE INDEX idx_versions_order
  ON market_listing_versions(listing_id, major DESC, minor DESC, patch DESC);

-- Install event log: every "받기" click writes one row. user_id is NOT NULL
-- because the dashboard requires OAuth before market access. The same user
-- installing the same listing twice (e.g. after deleting their local copy)
-- writes a second row — that's intentional, so popularity counts reflect
-- demand, not unique users (use COUNT(DISTINCT user_id) for unique-installer
-- counts).
CREATE TABLE IF NOT EXISTS market_installs (
  id           UUID PRIMARY KEY,
  listing_id   UUID NOT NULL REFERENCES market_listings(id) ON DELETE CASCADE,
  version      TEXT NOT NULL,
  user_id      TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
  installed_at BIGINT NOT NULL
);
CREATE INDEX idx_installs_listing ON market_installs(listing_id);
CREATE INDEX idx_installs_user ON market_installs(user_id);

CREATE TABLE IF NOT EXISTS market_reviews (
  id            UUID PRIMARY KEY,
  listing_id    UUID NOT NULL REFERENCES market_listings(id) ON DELETE CASCADE,
  user_id       TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
  version       TEXT NOT NULL,
  rating        SMALLINT NOT NULL CHECK (rating BETWEEN 1 AND 5),
  body          JSONB NOT NULL,
  helpful_count INTEGER NOT NULL DEFAULT 0,
  created_at    BIGINT NOT NULL,
  UNIQUE (listing_id, user_id)
);
CREATE INDEX idx_reviews_listing ON market_reviews(listing_id);

CREATE TABLE IF NOT EXISTS market_review_helpful (
  review_id UUID NOT NULL REFERENCES market_reviews(id) ON DELETE CASCADE,
  user_id   TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
  voted_at  BIGINT NOT NULL,
  PRIMARY KEY (review_id, user_id)
);

-- "Watch" — user wants to see new versions of this listing. Notification
-- delivery is out of scope of the schema; consumer reads the subscription list
-- and dispatches over its preferred channel (SSE / email / extension toast).
CREATE TABLE IF NOT EXISTS market_watches (
  user_id       TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
  listing_id    UUID NOT NULL REFERENCES market_listings(id) ON DELETE CASCADE,
  subscribed_at BIGINT NOT NULL,
  PRIMARY KEY (user_id, listing_id)
);

CREATE TABLE IF NOT EXISTS market_reports (
  id          UUID PRIMARY KEY,
  listing_id  UUID REFERENCES market_listings(id) ON DELETE CASCADE,
  review_id   UUID REFERENCES market_reviews(id) ON DELETE CASCADE,
  reporter_id TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
  reason      TEXT NOT NULL,
  details     TEXT,
  status      TEXT NOT NULL DEFAULT 'open',
  created_at  BIGINT NOT NULL,
  CHECK (status IN ('open', 'resolved')),
  CHECK (listing_id IS NOT NULL OR review_id IS NOT NULL)
);
CREATE INDEX idx_reports_open ON market_reports(status, created_at) WHERE status = 'open';
