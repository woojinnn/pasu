# DB Migrations

`0001_init.sql` 부터 시작하는 forward-only SQL 마이그레이션.

## 계획된 마이그레이션

| 번호 | 내용 | 비고 |
|---|---|---|
| `0001_init.sql` | wallets, tokens, token_holdings, approvals_*, positions_*, pending_txs, state_deltas, global_live_fields, block_heights 9 테이블 + 인덱스 | 첫 릴리즈 |
| `0002_check_constraints.sql` | JSON1 `json_valid` CHECK + enum 컬럼 CHECK | data integrity |
| `0003_views.sql` | `current_balance` 등 자주 쓰는 view | optional |

## 규칙

- forward-only. revert 는 안 만든다 (sqlite-wasm 환경 고려).
- 각 마이그레이션은 idempotent 하게 작성 — `CREATE TABLE IF NOT EXISTS` 등.
- `migrate::run()` 가 `_meta_migrations` 테이블에 적용된 번호를 기록.
