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
4. **보편성 (Universal field)**: 새 필드 추가 시 *모든 protocol family 에서 의미가 일관* 한지 확인. 한 family 에만 적용되는 raw 정보 (V3 packed path 등) 는 향후 Extension Schema 영역.
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

- `host:registry` — token registry, contract label 같은 정적 매핑
- `host:onchain` — 단일 RPC 조회 (allowance, NFT owner, pool config)
- `host:oracle` — **v1.0.1 범위 외** — 별도 enrichment 단계로 위임

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
│   ├── root.json             ← 최상위 컨테이너 (action enum 10종)
│   ├── common/_common.json   ← Address / Hex / DecimalString / IntDecimalString
│   │                           / AssetRef / AmountConstraint / Validity
│   └── actions/
│       ├── swap.json
│       ├── add_liquidity.json          ← fungible LP (V2/Curve/Balancer)
│       ├── remove_liquidity.json       ← fungible LP (V2/Curve/Balancer)
│       ├── mint_liquidity_nft.json     ← V3/V4 NPM 신규 발행
│       ├── burn_liquidity_nft.json     ← V3/V4 NPM 소각 (burnKind enum)
│       ├── increase_liquidity.json     ← V3/V4 NPM 기존 NFT liquidity ↑
│       ├── decrease_liquidity.json     ← V3/V4 NPM 기존 NFT liquidity ↓
│       ├── wrap.json
│       ├── unwrap.json
│       └── approve.json
└── docs/
    ├── root-schema.md
    ├── misc-actions.md       ← wrap / unwrap / approve
    └── dex-actions.md        ← swap + 6-way liquidity (fungible LP + NFT)
```

`schema/` 와 `docs/` 는 항상 동기화 — schema 변경 시 docs 의 JSON 예시 / 필드 표 / 정책 예시 모두 갱신.

---

## 6. 작업 흐름

1. 새 action 또는 필드 의도 → §4 체크리스트 통과 확인
2. schema 수정 (action 1개씩, 또는 _common.json 1개씩)
3. `python3 -c "import json; json.load(open('schema/<file>.json'))"` 로 JSON parse 검증
4. 대응 docs 갱신 (JSON 예시, 필드 표, 정책 예시 3 곳)
5. 최종 grep — schema 와 docs 간 stale reference 없는지 확인
