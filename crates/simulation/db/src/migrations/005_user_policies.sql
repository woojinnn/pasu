-- 005_user_policies — Cedar 정책 텍스트 저장소.
--
-- 사용자가 policy-builder 또는 직접 .cedar 텍스트 작성 → 여기에 INSERT.
-- 정책 평가 시 enabled=1 인 row 들을 전부 PolicySet 으로 조합.

CREATE TABLE user_policies (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  name        TEXT    NOT NULL,                 -- 사람용 이름 ("max HF guard")
  description TEXT,                              -- 옵션 — 정책 설명
  cedar_text  TEXT    NOT NULL,                  -- 실제 .cedar 본문
  severity    TEXT    NOT NULL DEFAULT 'deny',   -- 'deny' | 'warn' | 'info'
  enabled     INTEGER NOT NULL DEFAULT 1,        -- 0 = 비활성 (보관만)
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
);

CREATE INDEX idx_user_policies_enabled ON user_policies(enabled) WHERE enabled = 1;
