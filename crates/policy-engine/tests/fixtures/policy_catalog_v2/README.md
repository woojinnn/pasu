# Policy catalog v2 — precedence-bucket tree

**240** research-grounded wallet pre-sign 보안 정책 (v2 ActionBody 모델 대상). 실유스케이스 리서치
(Pocket Universe / Blockaid / Blowfish / MetaMask / Rabby / Fireblocks / Coinbase + OFAC / FATF / Chainalysis)
→ ScopeBall expressibility 로 prune → Cedar-compile 검증된 corpus. shipped default 가 **아니다**
(`default_policies_v2/` 9 bundle 은 별개; 이 227 은 auto-install/enforce 되지 않음).

## 구조 — precedence 4-bucket tree

정책은 단일 트리에 산다. 4 bucket 은 `compliance > protocol > wallet > action` **우선순위**로 정렬,
**first-match-wins** 로 배치가 결정론적·배타적. (4 bucket 은 두 축 — *specificity* + *source* — 이 섞여
있어 우선순위 규칙으로 모호성 제거.)

```
policy_catalog_v2/
├─ README.md                 # ← 이 파일 (인덱스)
├─ _methods/                 # 공유 aggregator-method 구현스펙 17개 (manifest "method" 문자열이 링크 키)
├─ compliance/  [1]  규제·제재·관할 mandate (50; ~6 distinct mechanism × screening fan-out — §하단)
├─ protocol/    [2]  protocol-specific SEMANTICS 의존 (69)
├─ wallet/      [3]  per-wallet 유저/operator config (60)
└─ action/      [4]  protocol-agnostic 보안 best-practice (61)
```

**배치 규칙(보강2)**: `protocol/` 은 protocol-specific action tag (hl_*/set_e_mode/delegate_to/vote_for_gauge/
pt_swap/permit2_*/sign_intent_order …) 에만. generic tag (swap/erc20_*/borrow/open_position) 는 venue 라도
`action/`·`wallet/`. precedence 탈락 속성은 cedar `// tags:` 로 보존 (action-family/venue/could-be-compliance).

## policy set = 폴더(파일 2개) + 공유 `_methods/`

| 위치 | 파일 | 역할 |
|---|---|---|
| `<bucket>/<sub>/<id>/` | `policy.cedar` | Cedar 규칙 + rationale 주석 (blocks/why/bucket/tags/methods) |
| 〃 | `manifest.json` | trigger(`action.tag`/`action.domain`) + `policy_rpc` method + `id`(== leaf dir) |
| `_methods/` | `<method>.md` | aggregator method 구현스펙 (RPC 서버가 어떻게 구현) — method당 1벌 공유 (DRY) |

## 검증

- `cargo test -p policy-engine --test policy_catalog_v2` — 트리 재귀 walk, 227 set 전부 parse + `validate`
  + **Cedar-compile**(`compose_per_policy`→`build_from_per_policy`). 실패는 전부 한 번에 보고(collect-all).
- `cargo test -p policy-engine-integration-tests --test real_tx_catalog_mapping` — 실 tx → production 디코더
  → 이 카탈로그로 평가 (corpus + USDC deny + live Etherscan).

## 분포

**240** = action 61 / wallet 60 / protocol 69 / compliance 50. **66 deny / 174 warn**. **122 enrichment / 118 static**.

### action/ (61)
approval 18 · swap 10 · transfer 8 · lending 6 · batch 5 · permission 3 · perp 3 · token 3 · nft 2 · yield 2 · staking 1

### wallet/ (60)
usd-cap 16 · cooldown 9 · recipient-allowlist 8 · fraction-of-holdings 6 · venue-allowlist 4 · batch 3 · stat-window 3 ·
spender-allowlist 2 · (singletons) chain-allowlist · chain-denylist · contract-denylist · perp-config · recipient-denylist ·
spender-denylist · token-allowlist · token-denylist · venue-denylist

### protocol/ (69)
hyperliquid 17 · aave 9 · curve 8 · eigenlayer 8 · pendle 8 · compound-morpho 5 · intent-venue 5 · permit2 5 · lido 4

### compliance/ (50)
sanctions 21 · risk 16 · issuer-freeze 6 · reporting 4 · jurisdiction 3

> **트리 자체가 인덱스다** (227개 flat 나열 대신). 한 정책 찾기: `find <bucket>/<sub> -name policy.cedar`.
> 교차 질의: `grep -rl 'action-family:min-out-zero' --include='*.cedar'` (precedence 가 쪼갠 동일 best-practice 재봉합),
> `grep -rl 'method:address.sanctions' --include='*.cedar'` (한 메서드 의존 정책 전부), `grep -rl 'could-be-compliance'`.

## 어그리게이터 메서드 (17 canonical — dedup: 같은 role = 같은 메서드)

`_methods/<name>.md` 에 구현스펙 1벌. **REGISTERED** = `schema/method-catalog.json` 등록(엔진 dispatch),
**aspirational** = 참조되나 미등록 → 해당 enrichment 정책은 method wired 까지 **dormant**(`context.custom has <leaf>`
guard false → inert, false verdict 없음).

| method | status | role | 참조수 |
|---|---|---|---|
| `oracle.usd_value` | REGISTERED | (asset,amount)→USD | 22 |
| `clock.now` | REGISTERED | unix time | 9 |
| `portfolio.input_fraction_bps` | REGISTERED | amount as % of holdings | 6 |
| `portfolio.balance` | REGISTERED | actor balance of asset | 2 |
| `stat_window.snapshot` | REGISTERED | rolling-window outflow aggregate | 2 |
| `stat_window.swap_stats` | REGISTERED | rolling-window swap aggregate | 1 |
| `approval.allowance` | REGISTERED | on-chain allowance covers amount | 1 |
| `address.reputation` | aspirational | scam/drainer heuristic flag | 16 |
| `address.sanctions` | aspirational | OFAC SDN/EU/UN legal-list membership | 13 |
| `address.activity` | aspirational | address age / contract-ness / tx count | 11 |
| `address.category` | aspirational | risk typology (mixer/darknet/ransomware/pep/…) | 11 |
| `token.metadata` | aspirational | verified / fee-on-transfer / rebasing | 5 |
| `lending.health_factor` | aspirational | post-action health factor | 5 |
| `address.frozen` | aspirational | stablecoin issuer blocklist on-chain view | 4 |
| `vault.share_state` | aspirational | ERC-4626 first-depositor inflation | 2 |
| `address.similarity` | aspirational | address-poisoning lookalike | 1 |
| `pool.liquidity` | aspirational | pool 24h vol / tvl | 1 |

`address.reputation`(scam heuristic) ≠ `address.sanctions`(legal list) ≠ `address.category`(typology) — **3 distinct
role, 분리 유지**. 구현 우선순위(최대 정책 활성): oracle.usd_value → clock.now → address.reputation → address.sanctions →
address.activity/category → portfolio/token.metadata → 나머지.

## Compliance — mechanism vs surface (정직한 구분)

static analyzer 의 규제 표면에서 **genuinely-distinct mechanism 은 ~6** (sanctions-list / risk-category /
issuer-freeze / USD-threshold / jurisdiction-pack / scam-reputation). 50 정책은 이 6 메커니즘을 **action surface
별로 fan-out** 한 것 — 각 행은 별개 bind-site (sanctioned 수취인을 transfer·swap·approve·permit2·nft·lending·perp·
hl·yield·restaking·airdrop·staking·launchpad 각각에서 차단)이지 동일 정책의 cosmetic 복제가 **아니다**. 즉 50 은
**커버리지 폭**이고 mechanism 수는 6 으로 정직히 유지. fabrication(동일 정책 rename)은 하지 않았다. 구조적 한계:
jurisdiction 은 on-chain 추론 불가(install-time baked-set pack), Travel-Rule/CTR 는 VASP 의무(informational warn),
OFAC near-match/PEP 는 provider-dependent(warn). 상세 = `compliance/README.md`.

## Authoring 권위 + 절차

- **VOCAB 권위**: trigger tag 는 `crates/policy-engine/src/schema/per_policy.rs` `RESOLVER_TABLE`
  (`(domain, action_tag)`; HL 은 `hl_` prefix, `unknown`/`multicall` 은 `action.domain` 으로 trigger, `set_e_mode`).
  context field 는 `schema/policy-schema/actions/**/*.cedarschema`.
- 신규 정책: ① manifest trigger(RESOLVER_TABLE tag) → ② precedence 로 bucket → ③ `<bucket>/<sub>/<id>/`(id==leaf) →
  ④ cedar + 헤더 주석/tags → ⑤ method 가 `_methods/` 에 있는지(없으면 추가) → ⑥ `policy_catalog_v2` gate green.

## Out of scope (정적 디코드로 표현 불가 — 작성 안 함)

flash-loan/sandwich intent, reentrancy/state-diff, honeypot bytecode scan, EIP-712 render-spoof,
EIP-7702 set_code(디코드 tag 부재), Seaport consideration[]/offer[] 세부, tx-envelope(gasPrice/msg.value),
per-leg batch recipient/amount(Multicall context 는 `{domain,action}` summary 만). execution trace / 렌더링 데이터 의존.
