/* Scopeball Market — 🎯 가치 카피 NLG (authored headline가 없을 때의 fallback)
   입력: policy { domain, intents[], severity }, locale 'ko'|'en'
   출력: 위험/이유 중심 한 줄. intent-first 공개 원칙(무엇을·왜 먼저). */
(function () {
  // 의도(why)별 위험 한 줄 — 마켓의 핵심 "왜"
  var INTENT_RISK = {
    slippage:   { ko: "느슨한 슬리피지 한도는 불리한 체결가를 부른다", en: "Loose slippage caps invite bad fills" },
    sandwich:   { ko: "넓은 가격 밴드는 샌드위치 MEV의 먹잇감", en: "Wide price bands feed sandwich MEV" },
    liquidation:{ ko: "얇은 건전성은 급락 한 번에 강제 청산", en: "A thin health factor is one dip from liquidation" },
    drainer:    { ko: "악성 승인 한 번이면 지갑이 통째로 빈다", en: "One malicious approval drains the whole wallet" },
    phishing:   { ko: "위장된 서명 요청은 피싱의 입구", en: "Disguised signature requests are the phishing door" },
    approval:   { ko: "방치된 승인은 갈수록 커지는 공격면", en: "Stale approvals are an ever-growing attack surface" },
    unlimited:  { ko: "무제한 승인은 한 번 털리면 전액 손실", en: "An unlimited approval means total loss if exploited" },
    compliance: { ko: "제재 대상 주소와의 거래는 컴플라이언스 위반", en: "Touching sanctioned addresses breaks compliance" },
    depeg:      { ko: "디페그 징후를 놓치면 스테이블 가치가 무너진다", en: "Miss the de-peg and the stable value collapses" },
    recipient:  { ko: "낯선 수령자는 영구 손실로 가는 지름길", en: "Unknown recipients are a shortcut to permanent loss" },
    overtrade:  { ko: "충동적 반복 거래는 수수료와 실수만 쌓인다", en: "Impulsive repeat trades only pile up fees and mistakes" }
  };
  // 의도가 없는 정책의 도메인 기반 일반 위험
  var DOMAIN_RISK = {
    swap:      { ko: "스왑 한 건이 조용히 가치를 새게 한다", en: "A single swap can quietly leak value" },
    perp:      { ko: "과도한 레버리지는 변동성에 무방비", en: "Excess leverage leaves you exposed to volatility" },
    lending:   { ko: "담보 건전성을 방치하면 청산 위험이 커진다", en: "Neglected collateral health raises liquidation risk" },
    security:  { ko: "기본 위생 하나가 지갑 전체를 지킨다", en: "One hygiene rule guards the whole wallet" },
    nft:       { ko: "NFT 승인·민팅에 숨은 함정", en: "Hidden traps in NFT approvals and mints" },
    airdrop:   { ko: "클레임을 가장한 악성 트랜잭션", en: "Malicious transactions disguised as a claim" },
    portfolio: { ko: "무절제한 자기관리는 손실을 부른다", en: "Undisciplined self-custody invites loss" },
    ammlp:     { ko: "유동성 공급에 숨은 비대칭 위험", en: "Asymmetric risk hidden in liquidity provision" },
    bridge:    { ko: "브릿지 구간은 탈취의 사각지대", en: "Bridges are a blind spot for theft" },
    sale:      { ko: "세일·런치패드의 과열된 베팅", en: "Overheated bets on sales and launchpads" },
    staking:   { ko: "스테이킹 해제·전환의 숨은 리스크", en: "Hidden risk in unstaking and conversions" },
    gov:       { ko: "거버넌스 행동의 의도치 않은 결과", en: "Unintended consequences of governance actions" }
  };

  function marketNLG(policy, locale) {
    var lang = locale === "en" ? "en" : "ko";
    var ints = policy.intents || [];
    var base;
    if (ints.length && INTENT_RISK[ints[0]]) base = INTENT_RISK[ints[0]][lang];
    else base = (DOMAIN_RISK[policy.domain] || DOMAIN_RISK.security)[lang];
    return base;
  }

  // 작동 시점(trigger) — domain 기반 추정 문구 (데이터에 trigger 필드 없음 → NLG)
  var DOMAIN_TRIGGER = {
    swap:      { ko: "스왑 서명 시", en: "On swap signing" },
    perp:      { ko: "포지션 개설·증액 시", en: "On opening or increasing a position" },
    lending:   { ko: "차입·인출 서명 시", en: "On borrow or withdraw signing" },
    security:  { ko: "트랜잭션 서명 시", en: "On any transaction signing" },
    nft:       { ko: "NFT 승인·구매 시", en: "On NFT approval or purchase" },
    airdrop:   { ko: "클레임 트랜잭션 시", en: "On claim transactions" },
    portfolio: { ko: "전송·스왑 시", en: "On transfers and swaps" },
    ammlp:     { ko: "유동성 추가·제거 시", en: "On adding or removing liquidity" },
    bridge:    { ko: "브릿지 전송 시", en: "On bridge transfers" },
    sale:      { ko: "세일 참여 서명 시", en: "On sale participation" },
    staking:   { ko: "스테이킹·언스테이킹 시", en: "On staking or unstaking" },
    gov:       { ko: "투표·위임 서명 시", en: "On voting or delegation" }
  };
  function marketTrigger(policy, locale) {
    var lang = locale === "en" ? "en" : "ko";
    return (DOMAIN_TRIGGER[policy.domain] || DOMAIN_TRIGGER.security)[lang];
  }

  window.marketNLG = marketNLG;
  window.marketTrigger = marketTrigger;
})();
