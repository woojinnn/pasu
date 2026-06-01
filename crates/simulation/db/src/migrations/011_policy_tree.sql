-- 011_policy_tree — Scratch-style block editor (v7) round-trip storage.
--
-- The v7 builder serializes its `Doc` (HatNode + LogicNode tree + drafts)
-- to JSON for persistence so the user can reopen a policy in the builder
-- exactly as they left it. `cedar_text` remains the compile target —
-- `policy_tree` is metadata only; the runtime still loads from cedar_text.
--
-- NULL when the policy was authored directly in the Code mode (textarea)
-- — opening that policy in the builder falls back to a "Code-only, no
-- block tree" warning and offers `cedar_text` as read-only.

ALTER TABLE user_policies ADD COLUMN policy_tree TEXT NULL;
