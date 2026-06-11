/**
 * The "무엇을 검사하나요?" (trigger) action list, derived from the schema so the
 * picker offers EVERY action the engine knows — not a hand-curated subset.
 *
 * Source of truth: {@link SCHEMA_ACTIONS} (codegen from the .cedarschema files).
 * This module only adds Korean labels + domain grouping on top.
 */

import { SCHEMA_ACTIONS } from "./schema-catalog.generated";

/** A selectable action for the trigger dropdown. `entityType`/`id` map to the
 *  Cedar scope `action == entityType::"id"`. */
export interface KnownAction {
  entityType: string;
  id: string;
  label: string;
  /** Korean domain group, for `<optgroup>` rendering. */
  group: string;
}

/** Korean group label per namespace + a stable display order. */
const GROUP_KO: Record<string, string> = {
  Token: "토큰",
  Amm: "스왑·유동성 (AMM)",
  Perp: "선물 (Perp)",
  HyperliquidCore: "Hyperliquid",
  Lending: "대출",
  Yield: "일드 (Pendle)",
  Staking: "스테이킹",
  LiquidStaking: "리퀴드 스테이킹",
  Restaking: "리스테이킹",
  Governance: "거버넌스",
  Airdrop: "에어드랍",
  Launchpad: "런치패드",
  Marketplace: "NFT 마켓",
  Permission: "권한",
  Core: "기타",
};
const GROUP_ORDER = [
  "Token", "Amm", "Perp", "HyperliquidCore", "Lending", "Yield",
  "Staking", "LiquidStaking", "Restaking", "Governance", "Airdrop",
  "Launchpad", "Marketplace", "Permission", "Core",
];

/** Korean labels for the common actions; the long tail humanises its id. */
const LABEL_KO: Record<string, string> = {
  "Amm::Swap": "스왑",
  "Amm::AddLiquidity": "유동성 추가",
  "Amm::RemoveLiquidity": "유동성 제거",
  "Amm::CollectFees": "수수료 수령",
  "Amm::GsmSwap": "GSM 스왑",
  "Amm::SignIntentOrder": "인텐트 주문 서명",
  "Amm::PreSignIntentOrder": "인텐트 사전서명",
  "Amm::SettleIntentOrder": "인텐트 정산",
  "Amm::CancelIntentOrder": "인텐트 취소",
  "Token::Erc20Transfer": "토큰 전송",
  "Token::Erc20Approve": "토큰 승인",
  "Token::Erc20Permit": "토큰 Permit 서명",
  "Token::RevokeApproval": "승인 취소",
  "Token::NftTransfer": "NFT 전송",
  "Token::NftApprove": "NFT 승인",
  "Token::NftSetApprovalForAll": "NFT 전체 승인",
  "Token::Permit2Approve": "Permit2 승인",
  "Token::Permit2SignAllowance": "Permit2 허용량 서명",
  "Token::Permit2SignTransfer": "Permit2 전송 서명",
  "Token::Permit2TransferFrom": "Permit2 전송",
  "Token::WrapNative": "네이티브 랩핑",
  "Token::UnwrapNative": "네이티브 언랩",
  "Perp::OpenPosition": "포지션 오픈",
  "Perp::ClosePosition": "포지션 종료",
  "Perp::IncreasePosition": "포지션 증가",
  "Perp::DecreasePosition": "포지션 감소",
  "Perp::ChangeLeverage": "레버리지 변경",
  "Perp::AdjustMargin": "마진 조정",
  "Perp::ChangeMarginMode": "마진 모드 변경",
  // Unified order placement (limit / stop / twap) — replaces the former
  // Perp::PlaceLimitOrder + Perp::PlaceStopOrder. The order kind is a condition
  // field (orderType.kind), not separate actions.
  "Perp::PlaceOrder": "주문 넣기",
  "Perp::CancelOrder": "주문 취소",
  "Perp::ClaimFunding": "펀딩 수령",
  // Hyperliquid-native actions that survived the perp/token migration (the rest
  // route to Perp / Token::Erc20Transfer / Staking / Core::Unknown).
  "HyperliquidCore::HlSendAsset": "HL 자산 전송",
  "HyperliquidCore::HlTokenDelegate": "HL 토큰 위임",
  "HyperliquidCore::HlWithdraw": "HL 출금",
  "Lending::Supply": "예치",
  "Lending::Withdraw": "출금",
  "Lending::Borrow": "차입",
  "Lending::Repay": "상환",
  "Lending::Liquidate": "청산",
  "Lending::EnableCollateral": "담보 활성화",
  "Lending::DisableCollateral": "담보 비활성화",
  "Lending::SetEMode": "eMode 설정",
  "Lending::SwapRateMode": "금리 모드 변경",
  "Lending::DelegateBorrow": "차입 위임",
  "Lending::SetAuthorization": "권한 설정",
  "Lending::BuyCollateral": "담보 매수",
  "Lending::PeripheryOperation": "주변 작업",
  "Governance::Delegate": "투표권 위임",
  "Governance::Vote": "투표",
  "Governance::Propose": "제안",
  "Governance::Queue": "큐 등록",
  "Governance::Execute": "실행",
  "Governance::Cancel": "취소",
  "Governance::StartVote": "투표 시작",
  "Governance::CloseVote": "투표 종료",
  "Governance::ActivateVoting": "투표 활성화",
  "Governance::UpdateRepresentative": "대표자 변경",
  "Governance::RedeemCancellationFee": "취소 수수료 회수",
  "Airdrop::Claim": "에어드랍 청구",
  "Airdrop::Delegate": "에어드랍 위임",
  "Staking::Stake": "스테이킹",
  "Staking::Lock": "락업",
  "Staking::Unlock": "언락",
  "Staking::ClaimRewards": "보상 수령",
  "Staking::Cooldown": "쿨다운",
  "Staking::Redeem": "리딤",
  "Staking::GaugeDeposit": "게이지 예치",
  "Staking::GaugeWithdraw": "게이지 출금",
  "Staking::VoteForGauge": "게이지 투표",
  "Staking::IncreaseLockAmount": "락업 수량 증가",
  "Staking::IncreaseLockTime": "락업 기간 증가",
  "LiquidStaking::Stake": "리퀴드 스테이킹",
  "LiquidStaking::Wrap": "랩핑",
  "LiquidStaking::Unwrap": "언랩",
  "LiquidStaking::RequestWithdrawal": "출금 요청",
  "LiquidStaking::ClaimWithdrawal": "출금 청구",
  "LiquidStaking::TransferShares": "셰어 전송",
  "Restaking::Deposit": "리스테이킹 예치",
  "Restaking::DelegateTo": "오퍼레이터 위임",
  "Restaking::Undelegate": "위임 해제",
  "Restaking::Redelegate": "재위임",
  "Restaking::QueueWithdrawal": "출금 큐",
  "Restaking::CompleteWithdrawal": "출금 완료",
  "Restaking::RegisterOperator": "오퍼레이터 등록",
  "Yield::PtSwap": "PT 스왑",
  "Yield::YtSwap": "YT 스왑",
  "Yield::MintPy": "PY 발행",
  "Yield::RedeemPy": "PY 리딤",
  "Yield::MintSy": "SY 발행",
  "Yield::RedeemSy": "SY 리딤",
  "Yield::ClaimYield": "일드 수령",
  "Yield::AddMarketLiquidity": "마켓 유동성 추가",
  "Yield::RemoveMarketLiquidity": "마켓 유동성 제거",
  "Yield::SignLimitOrder": "지정가 주문 서명",
  "Yield::CancelLimitOrder": "지정가 주문 취소",
  "Launchpad::Commit": "참여(커밋)",
  "Launchpad::WithdrawCommit": "커밋 회수",
  "Launchpad::ClaimAllocation": "할당 청구",
  "Launchpad::ClaimVested": "베스팅 청구",
  "Launchpad::Refund": "환불",
  "Marketplace::SignOrder": "주문 서명",
  "Marketplace::FulfillOrder": "주문 체결",
  "Marketplace::CancelOrder": "주문 취소",
  "Permission::ProtocolAuthorization": "프로토콜 권한 부여",
  "Core::Multicall": "멀티콜",
  "Core::Unknown": "알 수 없는 거래",
};

/** Split a PascalCase id into spaced words as a last-resort label. */
function humanize(id: string): string {
  return id.replace(/([a-z0-9])([A-Z])/g, "$1 $2");
}

/** Namespaces hidden from the trigger picker. */
const EXCLUDED_NS = new Set(["Yield"]);
const PICKABLE_ACTIONS = SCHEMA_ACTIONS.filter(([ns]) => !EXCLUDED_NS.has(ns));

/** Every schema action, labelled + grouped. */
export const KNOWN_ACTIONS: KnownAction[] = PICKABLE_ACTIONS.map(([ns, id]) => ({
  entityType: `${ns}::Action`,
  id,
  label: LABEL_KO[`${ns}::${id}`] ?? humanize(id),
  group: GROUP_KO[ns] ?? ns,
}));

/** Actions bucketed by domain group, in display order, for `<optgroup>`. */
export const ACTION_GROUPS: { group: string; actions: KnownAction[] }[] = (() => {
  const byNs = new Map<string, KnownAction[]>();
  for (const [ns] of PICKABLE_ACTIONS) if (!byNs.has(ns)) byNs.set(ns, []);
  for (const a of KNOWN_ACTIONS) {
    const ns = a.entityType.split("::")[0];
    byNs.get(ns)!.push(a);
  }
  const order = [...GROUP_ORDER, ...[...byNs.keys()].filter((n) => !GROUP_ORDER.includes(n))];
  return order
    .filter((ns) => byNs.has(ns))
    .map((ns) => ({
      group: GROUP_KO[ns] ?? ns,
      actions: byNs.get(ns)!.slice().sort((a, b) => a.label.localeCompare(b.label, "ko")),
    }));
})();
