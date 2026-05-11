# schema/ — JSON Schema source-of-truth

본 디렉터리의 `*.json` 이 본 프로젝트의 **type 의 ground truth** 입니다
(ADR-001). TypeScript / Rust 타입은 `generated/` 에 codegen 되며, 직접
편집 금지.

## 작성 순서 (구현 agent 용)

`docs/04-design-overview.md` §9 의 T1~T6 단계 따라가기. 각 파일의 정확한
spec 은 `docs/05-schema-spec.md` 의 동일 번호 section.

```
T1  _common.json + envelope.json + normalized-request.json
T2  action.json
T3  action-fields/swap.json + liquidity.json
T4  action-fields/wrap.json + approval.json + permit.json + transfer.json + router-plan.json
T5  extensions/uniswap-v2.json + uniswap-v3.json + universal-router.json + uniswap-v4.json + balancer-vault.json + pancake-* (4)
T6  dispatch-table.json
```

## 검증

`docs/12-validation-plan.md` 단계 0 (schema self-validate) + 단계 1 (fixture
instance validate). `tools/validate.ts` 사용.

## invariants

- 모든 properties 에 `x-source` 라벨 (ADR-003)
- protocol-version-specific 필드는 `x-version` 추가
- decoder 매핑이 있는 필드는 `x-adapter-mapping` 추가
- DecimalString / IntDecimalString / Address / Hex / ConfidenceLevel 는
  `_common.json#/$defs/` 에 한 번만 정의 + 모든 곳에서 `$ref`
- discriminated union 은 `oneOf` + `discriminator` keyword + 보조 `_kind` const
