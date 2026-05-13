# schema_v101 작업 지침 (CLAUDE.md)

본 디렉터리에서 Schema 또는 docs 를 수정하는 모든 작업자가 따라야 하는 지침. 핵심 설계 원칙과 글쓰기 규칙을 명시한다.

---

## 1. Schema 의 존재 의의

본 schema 는 **User Policy 평가의 근거** 로 쓰이는 정형 데이터 표면이다. Raw transaction 을 Decoder/Adapter 가 풀어준 후, 그 결과 중 **정책 평가에 의미 있는 정보만** 본 schema 에 매핑한다.

판단 기준은 단 한 줄:

> **이 필드 위에 사용자가 정책 규칙을 쓸 수 있는가?**

- Yes → schema 에 포함
- No → 제외 (calldata 의 raw bytes, route hops, decodedCalls 등)

---

## 2. 핵심 설계 원칙

1. **화이트리스트 기반**: 직접 매핑한 함수만 schema 인스턴스 생성. 그래서 `confidence` 필드 없음 — schema 에 들어왔다 = 이미 verified.
2. **Action ↔ Category 직교 차원**: 같은 action 이 여러 category 에 등장 가능 (`swap` 이 dex/liquid_staking/rwa 모두). action 은 의미 단위 식별자, category 는 protocol 맥락 태그.
3. **Oracle / User Portfolio 데이터는 schema 표면에 부재**: USD 환산값, 잔고, 가격은 schema 인스턴스 생성 *이후* 별도 enrichment 단계에서 attach. `UsdValuation` 같은 타입 schema 에 두지 않음.
4. **보편성 (Universal field)** — 두 층위로 검토:
   - **(a) 의미적 universal**: 모든 protocol family 에서 같은 의미를 지님. (예: swap 의 fee — V2 30 bps 고정, V3 pool config, Curve pool admin 변수, 모두 "이 swap 에서 발생하는 fee" 의미 동일.)
   - **(b) 소스적 universal**: 모든 protocol 의 calldata 에서 같은 위치 / 형식으로 채워짐.

   본 schema 는 **(a) 만 만족하면 포함**. (b) 가 부족하면 §3.5 의 `host:*` 라벨로 source 다양성 명시 — 예: `feeBps`, `slippageBps` 는 의미는 universal, source 는 protocol 별 다름 → 포함. (a) 도 부족한 protocol-specific raw 정보 (V3 packed path 등) 는 향후 Extension Schema 영역.
5. **정책 평가에 의미 있는 필드만**: 다른 schema 필드에서 derive 가능한 값은 schema 에 두지 않는다. 예: `isUnlimited = amount.kind === "unlimited"`, `Validity.expiresInSeconds = expiresAt - root.blockTimestamp`, `fields._kind = ActionEnvelope.action`, `AssetRef.chainId = root.chainId`. 모두 policy DSL 의 helper 또는 직접 비교 영역.

---

## 3. description 작성 가이드라인

JSON Schema 의 `description` 은 사용자/정책 작성자가 해당 필드를 이해하는 1차 자료. 다음 규칙을 따른다.

### 3.1 정보 우선순위 (간결성)

1. **무엇인가** — 1 문장. 이 필드가 표현하는 것.
2. **왜 schema 에 있는가** — 1 문장. 정책 평가에서 어떻게 쓰이는가.
3. (선택) **어떻게 채워지는가** — host 보강 / calldata 직접 / derived.

전체 description 은 **2~3 문장 안에** 끝낸다. 장황한 호환성 노트, 변경 이력은 git log / 별도 docs 로.

### 3.2 모호 표기 금지

| 나쁜 예 | 좋은 예 |
|---|---|
| "host 가 (chainId, address) → 매핑" | "host 가 (chainId, address) 쌍을 보고 protocol 식별자로 매핑함" |
| "정책 'X 차단' 분기" | "정책 예시: 'mainnet 외 차단'" — 이 필드로 어떤 검사를 하는지 명확히 |
| "Z 측면" | 구체적 동작 명시 |
| "X-style Y" | 정확한 명칭으로 풀어쓰기 |

화살표 `→`, 약어, "측면", "기반", "분기" 같은 짧은 표현은 의미가 모호하면 풀어쓴다.

### 3.3 영어 단어 + 한국어 조사

영어 단어를 한국어 문장에 섞을 때 조사는 **단어의 한국식 발음 기준** 으로 정확히 쓴다.

| 단어 | 발음 끝 | 조사 |
|---|---|---|
| asset (에셋) | ㅅ (자음) | 은 / 을 / 과 |
| address (어드레스) | ㅅ (자음) | 은 / 을 / 과 |
| remove (리무브) | 으 → 모음 | 는 / 를 / 와 |
| approve (어프루브) | 으 → 모음 | 는 / 를 / 와 |
| swap (스왑) | ㅂ (자음) | 은 / 을 / 과 |
| pool (풀) | ㄹ (자음) | 은 / 을 / 과 |
| amount (어마운트) | ㅡ → 모음 | 는 / 를 / 와 |

확실치 않으면 다른 자연스러운 어법으로 우회한다 (예: "X 의 경우", "X 인 경우").

### 3.4 약어 / 외부 용어

처음 등장 시 풀어쓴다.

- "OZ" → "OpenZeppelin (OZ)"
- "NPM" → "NonfungiblePositionManager (NPM)"
- "BPT" → "Balancer Pool Token (BPT)"

이후 등장은 약어 사용 가능.

### 3.5 host 보강 필드 출처 명시

host 가 채우는 필드는 description 끝에 출처 라벨을 단다.

- `host:registry` — token registry, contract label 같은 정적 매핑 (예: `AssetRef.symbol/decimals`, `spenderLabel`, `pool.label`)
- `host:onchain` — 단일 RPC 조회 (예: `currentAllowance`, NFT.ownerOf 검증, pool.address derive)
- `host:quote` — swap quote / aggregator API 단일 lookup. single integer 같은 가벼운 값 한정 (예: `slippageBps`). Oracle 과 달리 staleness 메타 불필요 — schema 표면에 포함 가능
- `host:oracle` — 가격 oracle (USD 환산값, staleness 메타 동반). **v1.0.1 범위 외** — schema 표면 부재, 별도 enrichment 단계로 위임

### 3.6 Action top-level description 작성 룰

`actions/<name>.json` 의 최상위 description 은 다음 3 문장 구조 권장:

1. **동작 한 줄** — 이 action 이 무엇을 하는가. (예: "단일 tokenIn 을 단일 tokenOut 으로 교환하는 동작.")
2. **매핑 protocol 함수 한 줄** — 어떤 protocol 의 어떤 함수들이 이 shape 로 정규화되는가. (예: "Uniswap V2/V3/V4 swap, Curve exchange, Balancer Vault.swap 등.")
3. **(선택) cross-category 등장 한 줄** — §2.2 의 Action ⊥ Category 직교 원칙 적용 — 다른 category 에서도 의미적으로 같은 동작이 등장하는 케이스 명시. (예: "Liquid Staking 의 Lido stETH ↔ wstETH, RWA 의 LBTC ↔ WBTC mint, 일부 Lending 의 collateral swap routing 등에서도 발생.")

3 번은 해당 action 이 실제로 다른 category 에 등장할 때만 추가. swap / approve 처럼 cross-category 흔한 action 은 권장. wrap / unwrap / mint_liquidity_nft 처럼 특정 맥락에 한정되는 action 은 생략.

---

## 4. 새 필드 추가 시 체크리스트

다음 모두 통과해야 schema 에 추가:

- [ ] 이 필드 위에 사용자가 정책 규칙을 쓸 수 있는가?
- [ ] 모든 protocol family 에서 의미가 일관적인가? (Universal 또는 omit-allowed 명확)
- [ ] 다른 필드에서 derive 가능하지 않은가? (derive 가능하면 policy DSL helper 영역)
- [ ] Oracle / Portfolio enrichment 단계의 데이터가 아닌가?
- [ ] description 이 위 §3 가이드라인 통과?

5 항목 통과 시에만 PR.

---

## 5. 디렉터리 구조

```
schema_v101/
├── CLAUDE.md                 ← 본 문서
├── contracts/                ← reference contracts (git clone, read-only)
├── schema/
│   ├── root.json             ← 최상위 컨테이너 (action enum 32종)
│   ├── common/_common.json   ← Address / Hex / DecimalString
│   │                           / AssetRef / AmountConstraint / Validity (6종)
│   └── actions/                  ← 32 action 을 5 category subdir 로 organized
│       ├── misc/                 ← 10 protocol-agnostic action
│       │   ├── wrap.json
│       │   ├── unwrap.json
│       │   ├── approve.json                ← ERC-20 / Permit2 amount 기반 (on-chain)
│       │   ├── set_approval_for_all.json   ← ERC-721/1155 collection 전체 boolean 위임
│       │   ├── transfer.json               ← ERC-20/721/1155 직접 이전
│       │   ├── permit.json                 ← EIP-712 서명 (EIP-2612 / Permit2 PermitSingle / PermitTransferFrom)
│       │   ├── claim_rewards.json          ← 누적 보상 회수 (V3/V4 NPM collect / Aave / Compound / Curve / Pendle)
│       │   ├── sign_message.json           ← EIP-712 typed data envelope-only 안전판
│       │   ├── delegate.json               ← ERC20Votes voting power 위임 (Compound / UNI / ENS / Aave / ARB / OP)
│       │   └── vote.json                   ← on-chain governance castVote (OZ Governor / Compound Bravo / Aave)
│       ├── dex/                  ← 7 DEX action (swap + 6-way liquidity)
│       │   ├── swap.json
│       │   ├── add_liquidity.json          ← fungible LP (V2/Curve/Balancer)
│       │   ├── remove_liquidity.json
│       │   ├── mint_liquidity_nft.json     ← V3/V4 NPM 신규 발행
│       │   ├── burn_liquidity_nft.json     ← V3/V4 NPM 소각
│       │   ├── increase_liquidity.json     ← V3/V4 기존 NFT liquidity ↑
│       │   └── decrease_liquidity.json     ← V3/V4 기존 NFT liquidity ↓
│       ├── lending/              ← 9 lending action
│       │   ├── supply.json                 ← Aave/Compound/Morpho/Euler/Fluid/Compound V2
│       │   ├── withdraw.json
│       │   ├── borrow.json                 ← HF check + onBehalf credit delegation
│       │   ├── repay.json                  ← repayKind: debt_asset / atoken_direct
│       │   ├── liquidate.json              ← 4 dialect: pool_share/protocol_absorb/socializable/single_asset
│       │   ├── flash_loan.json             ← 1-tx 차입+상환 (Aave / Morpho)
│       │   ├── set_authorization.json      ← 4 scope: all/debt_only/manager_role/position_manager_role
│       │   ├── sign_authorization.json     ← set_authorization 의 EIP-712 sign 변형 (3 scope)
│       │   └── revoke.json                 ← 권한 *받은* 측의 자기-반납 (4 kind)
│       ├── staking/              ← 3 liquid staking lifecycle
│       │   ├── stake.json                  ← LST 발급 (Lido / Rocket Pool / Mantle mETH / ether.fi / Frax)
│       │   ├── request_unstake.json        ← LST → ticket cooldown
│       │   └── claim_unstake.json          ← ticket 청구 → base asset 수령
│       └── restaking/            ← 3 restaking lifecycle
│           ├── restake.json                ← LRT/share 발급 (EigenLayer / Renzo / Kelp / Symbiotic / Karak)
│           ├── request_restake_withdrawal.json  ← LRT/share → escrow 진입
│           └── claim_restake_withdrawal.json    ← escrow 청구 → asset 수령
├── storage/                  ← 백업 / legacy (schema 디렉터리 외부 — 헷갈림 방지)
│   └── lending_liam_original/  ← liam 의 lending 원본 9 JSON (sync 전 백업)
└── docs/
    ├── root-schema.md
    ├── conventions.md        ← 재사용 패턴 / action 분리 / 새 category 결정
    ├── misc-actions.md       ← 10 misc action (wrap/unwrap/approve/set_approval_for_all/transfer/permit/claim_rewards/sign_message/delegate/vote)
    ├── dex-actions.md        ← DEX 7 action (swap + 6-way liquidity)
    ├── lending-actions.md    ← Lending 9 action (supply/withdraw/borrow/repay/liquidate/flash_loan/set_authorization/sign_authorization/revoke)
    ├── staking-actions.md    ← Liquid Staking + Restaking 6 action (stake/request_unstake/claim_unstake/restake/request_restake_withdrawal/claim_restake_withdrawal)
    ├── 01_bridge_request.md  ← bridge_deposit/bridge_claim 추가 회의용
    ├── 02_vault_request.md   ← ERC-4626 vault_deposit/withdraw 회의용
    └── 03_nft_request.md     ← nft_mint/nft_order 회의용
```

`schema/` 와 `docs/` 는 항상 동기화 — schema 변경 시 docs 의 JSON 예시 / 필드 표 / 정책 예시 모두 갱신.

---

## 6. 작업 흐름 (필드 단위 변경)

기존 action 에 필드 추가 / 제거 / 의미 변경 시:

1. 새 필드 / 변경 의도 → §4 체크리스트 통과 확인
2. schema 수정 (action 1개씩, 또는 _common.json 1개씩)
3. `python3 -c "import json; json.load(open('schema/<file>.json'))"` 로 JSON parse 검증
4. 대응 docs 갱신 (JSON 예시, 필드 표, 정책 예시 3 곳)
5. 최종 grep — schema 와 docs 간 stale reference 없는지 확인

action 단위 / category 단위 변경은 §8 / §9 참조.

---

## 7. JSON Schema 파일 구조 convention

모든 action / common / root schema 파일은 다음 형식을 따른다.

### 7.1 Top-level key 순서 (모든 파일 공통)

```jsonc
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "schema/...",                       // file-path 스타일 (작업 dir 이름 노출 금지)
  "title": "...",                             // PascalCase (예: SwapAction)
  "description": "...",                       // 2~3 문장, §3 가이드라인 준수

  "type": "object",
  "properties": { ... },
  "required": [...]
}
```

`$defs` 가 있으면 `required` 다음, 파일 끝. (root.json 의 `ActionEnvelope`, _common.json 의 primitives 만 사용.)

### 7.2 `$id` 패턴

- root: `schema/root.json`
- common: `schema/common/_common.json`
- action: `schema/actions/<action_name>.json` (snake_case)

**중요**: `$id` 에 작업 dir 이름 (예: `schema_v101/`) 노출 금지 — 보안상 정보 유출. 항상 `schema/...` prefix 만 사용. AJV 의 `$ref` resolution 은 relative path 로 동작하므로 prefix 변경 무관.

### 7.3 Naming convention

| 항목 | 컨벤션 | 예시 |
|---|---|---|
| action 이름 (action enum / 파일명 / $id) | **snake_case** | `add_liquidity`, `mint_liquidity_nft` |
| property 이름 | **camelCase** | `tokenIn`, `slippageBps`, `feeBps` |
| enum 값 | **snake_case** (동사·명사) 또는 **kebab-case** (정합 가능한 도메인 — Validity.source) | `proportional`, `tx-deadline` |
| title | **PascalCase + "Action" 접미사** | `SwapAction`, `BurnLiquidityNftAction` |

### 7.4 `additionalProperties` 정책

명시하지 않음 (= JSON Schema 기본 `true`, 즉 permissive). 화이트리스트 기반 설계라 host 가 정의된 필드만 생산 — strict false 의 실익이 작고 후속 minor bump 비용만 큼.

### 7.5 들여쓰기 / 빈 줄

- 2-space 들여쓰기
- properties 간 한 줄 빈 줄로 그룹 가독성 확보 (선택)

### 7.6 `required` 배열 작성 룰

다음 모두 통과하면 required, 하나라도 어긋나면 optional:

1. 매핑된 모든 protocol 의 모든 함수에서 *항상 채워지는가*?
2. 정책 평가 시 *매번 검사 의미 있는가*?
3. host 가 단일 RPC / registry 로 무조건 채울 수 있는가?

(자세한 분기 사례는 `docs/conventions.md` §3.4)

---

## 8. 새 action 추가 절차

기존 6 action enum (`swap` / `add_liquidity` / ... / `approve`) 에 신규 action 을 더할 때:

1. **§4 체크리스트 통과 확인** — 필드 단위와 동일하게 action 단위도 정책 가치 / universal / 비 derived / 비 Oracle 검사.
2. **action 분리 단위 결정** — `docs/conventions.md` §2 의 *wallet 자산 변화 형태별* 기준으로 신규 action 이 필요한지 vs 기존 action 의 discriminator 로 흡수할지.
3. **root.json 의 `ActionEnvelope.action` enum 갱신** — 신규 값 추가.
4. **`schema/actions/<name>.json` 신설** — §7 의 파일 구조 따름. `docs/conventions.md` §1 의 재사용 패턴 우선 활용.
5. **대응 docs 갱신**:
   - 기존 category 안의 신규 action → 해당 category docs (예: `dex-actions.md`) 에 § 추가
   - 새 category 의 신규 action → `docs/<category>-actions.md` 신설 (§9 참조)
   - `root-schema.md` §4.5 의 action enum 목록 / cross-category 매트릭스 갱신
6. **본 CLAUDE.md §5 디렉터리 구조** 의 action 목록 갱신.
7. **schemaVersion minor bump** (예: 1.0.1 → 1.0.2).
8. JSON parse + grep 으로 stale reference 0 확인.

---

## 9. 새 category 작업 큰 흐름

새 category (예: lending, restaking, yield) 전체 작업 시 — 처음부터 끝까지의 단계.

### 9.1 단계 (6 step)

1. **reference contracts 확보** — Defillama TVL 상위 EVM protocol 2~3 개 git clone → `contracts/`.
2. **사용자-facing 함수 인벤토리 작성** — 각 protocol 의 사용자-호출 가능한 함수 / opcode 목록. (decoder 단계의 결과 기준.)
3. **action 의미 단위 분리 결정** — `docs/conventions.md` §2 의 *wallet 자산 변화 형태별* 분리 원칙 + §3.2 의 의미 매트릭스 작성. 어느 함수가 어느 action 에 매핑되는지 결정. 기존 action 재사용 / 신규 action 신설 비율 검토.
4. **각 action 의 필드 선정** — §4 체크리스트로 각 필드 검증. `docs/conventions.md` §1 패턴 우선 활용. universal 안 되면 protocol-specific 필드는 Extension Schema 영역으로 미루기.
5. **schema 작성** — `schema/actions/<name>.json` (§7 구조). `_common.json` 변경이 필요하면 별도 검토.
6. **docs 작성** — `docs/<category>-actions.md` 신설 (또는 기존 docs 갱신, `docs/conventions.md` §3.3 룰).

### 9.2 동반 갱신 항목

- `root.json` 의 `ActionEnvelope.category` enum (신규 카테고리면)
- `root.json` 의 `ActionEnvelope.action` enum (신규 action 이면)
- `CLAUDE.md` §5 디렉터리 구조
- `docs/root-schema.md` §4.5 의 action × category 매트릭스
- `docs/conventions.md` §3.2 의 의미 매트릭스 (신규 카테고리의 action 매핑 추가)
- `schemaVersion` minor bump

### 9.3 검증

- `python3 -c "import json; json.load(open(...))"` 모든 schema 통과
- grep stale reference (이전 버전 흔적, 다른 category 의 description 잔존) 0건
- `docs/conventions.md` §1 패턴 일관 사용 확인 (예: 새 action 의 토큰 위치는 모두 `AssetRef`)

---

## 10. 관련 문서

- `docs/conventions.md` — **재사용 패턴 카탈로그 / action 분리 가이드 / 새 category 결정 사항.** 본 문서가 "원칙" 이라면 conventions.md 는 "원칙의 데이터 적용". 새 작업 시작 시 함께 읽을 것.
- `docs/root-schema.md` — Root Schema 가이드.
- `docs/dex-actions.md` — DEX 7 action 가이드 (swap + 6-way liquidity).
- `docs/misc-actions.md` — wrap / unwrap / approve 가이드.
