/**
 * Curated "what this package blocks" copy for the package detail summary.
 * Sourced from agentBase/policy-packages/*.md. Interim front-end map (mock
 * content, Korean) until the `docs` backend lands.
 */
export interface PackageBlock {
  t: string;
  d: string;
}
export interface PackageCopy {
  intro: string;
  blocks: PackageBlock[];
}

export const PACKAGE_COPY: Record<string, PackageCopy> = {
  "wallet-first-shield": {
    intro:
      "처음 Web3 지갑을 쓰는 사람이 가장 자주 당하는 치명적 사고 패턴을 한 묶음으로 막습니다. 서명 팝업의 의미를 몰라도 아래 함정이 자동으로 걸러집니다.",
    blocks: [
      { t: "무제한 승인", d: "한 번의 서명으로 토큰 전량 인출 권한이 새는 것" },
      { t: "수령처 바꿔치기", d: "스왑·전송 결과가 내가 아닌 공격자에게 가는 것" },
      { t: "소각 주소 오송금", d: "0x0·0x…dead로 보내 영구 분실되는 실수" },
      { t: "번들 속 숨은 승인", d: "‘리워드 받기’에 끼워 넣은 approve" },
      { t: "블라인드 서명", d: "내용을 알 수 없는 정체불명 서명" },
    ],
  },
  "no-mistake-swap": {
    intro:
      "스왑·LP에서 가장 흔한 사고는 ‘거래는 정상인데 받는 주소만 바뀐’ 경우입니다. 자금이 빠져나가는 모든 출구의 수령처가 정말 내 지갑인지 검사합니다.",
    blocks: [
      { t: "스왑 수령처 변조", d: "산 토큰이 공격자 주소로 가는 것" },
      { t: "LP 회수·수수료 수령처 변조", d: "유동성 회수금·수수료가 남에게" },
      { t: "자기 토큰 컨트랙트로 전송", d: "회수 불가 주소로의 오송금" },
      { t: "보유 대부분 일괄 유출", d: "한 번에 잔고 90% 이상 전송" },
    ],
  },
  "never-again": {
    intro:
      "언론에 보도된 실제 대형 탈취 사고에서 역설계한 정책집입니다. 각 정책은 ‘그 사고를 이 한 줄이 끊었을 것’이라는 형태로 설계됐습니다. (합산 $113.9M+)",
    blocks: [
      { t: "브릿지 무제한 승인", d: "LI.FI 해킹 $11.6M — 유한 승인만 했어도 피해 0" },
      { t: "Permit2 무기한 서명 피싱", d: "spWETH $32.4M · PEPE $1.39M" },
      { t: "주소 오염(address poisoning)", d: "WBTC $68M — 수령처 바꿔치기" },
      { t: "가짜·스푸핑 브릿지", d: "LINK $520K — 미등록 목적지 차단" },
    ],
  },
  "nft-vault-guard": {
    intro:
      "NFT 도난은 토큰을 하나씩 빼가는 게 아니라 setApprovalForAll로 컬렉션 전체를 한 번에 넘기는 단 한 번의 승인에서 시작됩니다(‘0원 리스팅 드레인’). 가짜 무료민트·에어드롭이 늘 요구하는 그 승인을 막습니다.",
    blocks: [
      { t: "컬렉션 통째 위임", d: "setApprovalForAll로 전체를 한 operator에게" },
      { t: "입찰용 무제한 WETH 승인", d: "오퍼 뒤에 숨은 무제한 approve" },
      { t: "NFT 소각 주소 분실", d: "되돌릴 수 없는 전송" },
      { t: "번들 속 숨은 승인", d: "multicall에 끼워 넣은 권한 부여" },
    ],
  },
  "leverage-safety": {
    intro:
      "Hyperliquid 선물은 자금이 즉시 빠지는 transfer가 아니라 ‘권한 위임·포지션 변경’ 서명이 많아 위험이 눈에 잘 안 띕니다. 위험 액션 6종을 확인 게이트로 잡고, 손실이 무한대인 숏 진입만 차단합니다.",
    blocks: [
      { t: "agent wallet 승인", d: "내 계정 거래권을 통째로 위임" },
      { t: "과도한 레버리지", d: "작은 역행에도 청산되는 설정" },
      { t: "무한 손실 숏 진입", d: "손실 상한이 없는 숏 — 차단(DENY)" },
      { t: "HL 자금 송금·출금·미상 액션", d: "한 번 더 확인" },
    ],
  },
  "claim-and-vote-guard": {
    intro:
      "이 영역 공격은 ‘자산이 줄지 않아 안심하게 되는’ 게 특징입니다. delegate는 잔고 변화가 0이라 유출 탐지를 그냥 통과하지만, 한 번의 서명으로 투표권·담보가 넘어갑니다.",
    blocks: [
      { t: "클레임 수령처 변조", d: "에어드롭이 공격자에게" },
      { t: "거버넌스·Aave 담보 위임 변조", d: "투표권·담보 탈취" },
      { t: "클레임 락업", d: "받자마자 묶이는 토큰" },
      { t: "merkle proof 정합성", d: "증명 빠진 가짜 클레임 서명" },
    ],
  },
};

export function packageCopy(slug: string): PackageCopy | undefined {
  return PACKAGE_COPY[slug];
}
