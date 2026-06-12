/**
 * policy-server 메서드 카탈로그 — "+ 새 보강 필드 만들기" 모달의 메서드 선택지.
 *
 * 실제로 서버가 구현한 메서드 2개(oracle.usd_value, token.normalize_to_nano —
 * manifest-gen/registry.ts의 hand-authored 항목과 같은 계약) + 형태를 보여주기
 * 위한 예시(mock) 항목들. 서버의 메서드 디스커버리 API가 생기면 이 정적 목록을
 * 그 응답으로 대체한다.
 *
 * 파라미터 값 문법은 manifest `policy_rpc[].params`와 동일: `$.`로 시작하면
 * plan-time 셀렉터(`$.root.*` = tx/체인, `$.action.*` = lowered 액션 컨텍스트),
 * 아니면 리터럴.
 */

import { i18n } from "../../../i18n";
import type { CustomType, ParamSpec } from "../../../editor-v9/manifest-gen";

export interface MethodSpec {
  /** Opaque policy-server method name (manifest `policy_rpc[].method`). */
  method: string;
  /** Output type the projection yields (`custom_context` spelling). */
  type: CustomType;
  /** Default `$.result.*` projection. */
  projection: string;
  /** Param template — key 고정, 값(셀렉터/리터럴)은 모달에서 편집 가능. */
  params: Record<string, ParamSpec>;
  /** True = 서버 미구현 예시(형태 시연용). 모달에 "(예시)"로 표시. */
  mock?: boolean;
}

/** 메서드 표시 이름 — i18n 키 `editor:methods.<method>.label` (점은 `_`로). */
export function methodLabel(m: MethodSpec): string {
  return i18n.t(`editor:methods.${m.method.replace(/\./g, "_")}.label`);
}

/** 메서드 설명 — i18n 키 `editor:methods.<method>.desc`. */
export function methodDesc(m: MethodSpec): string {
  return i18n.t(`editor:methods.${m.method.replace(/\./g, "_")}.desc`);
}

export const METHOD_CATALOG: MethodSpec[] = [
  {
    method: "oracle.usd_value",
    type: "decimal",
    projection: "$.result.usd",
    params: {
      chain_id: "$.root.chain_id",
      asset: "$.action.tokenIn.key.address",
      amount: "$.action.direction.amountIn",
    },
  },
  {
    method: "token.normalize_to_nano",
    type: "Long",
    projection: "$.result.nano",
    params: {
      amount: "$.action.direction.amountIn",
      chain_id: "$.root.chain_id",
      asset: "$.action.tokenIn.key.address",
    },
  },
  {
    method: "address.risk_score",
    type: "Long",
    projection: "$.result.score",
    params: {
      chain_id: "$.root.chain_id",
      address: "$.action.spender",
    },
    mock: true,
  },
  {
    method: "intent.pending_exposure_usd",
    type: "decimal",
    projection: "$.result.usd",
    params: {
      chain_id: "$.root.chain_id",
      wallet: "$.root.from",
    },
    mock: true,
  },
  {
    method: "token.price_change_24h",
    type: "decimal",
    projection: "$.result.pct",
    params: {
      chain_id: "$.root.chain_id",
      asset: "$.action.tokenIn.key.address",
    },
    mock: true,
  },
];
