/* Scopeball Market — Updates (신규 공개 / 버전 갱신 피드)
   버전은 단일 출처 Market.versionFor 에서만 읽음. version_bump.toVersion = 현재 버전(맵)과 일치.
   정직성: 모든 시드 "예시", diff/버전은 플레이스홀더. */

// ── UpdateItem 시드 ── (toVersion은 런타임에 versions 맵에서 주입 → detail과 항상 일치)
// { id, type, target:{type,ref}, fromVersion?, changelog{ko,en}, diffUrl, publishedAt }
const SEED_UPDATES_RAW = [
  // 오늘
  { id: "u01", type: "version_bump", target: { type: "policy", ref: "unknown-blind-sign-warning" }, fromVersion: "v2.1", publishedAt: "2026-06-03",
    changelog: { ko: "하드웨어 지갑 EIP-712 도메인 검사 강화", en: "Stronger EIP-712 domain check on hardware wallets" }, diffUrl: "#diff/unknown-blind-sign-warning" },
  { id: "u02", type: "version_bump", target: { type: "package", ref: "essentials" }, fromVersion: "v2.2", publishedAt: "2026-06-03",
    changelog: { ko: "무제한 승인 차단 정책 1건 편입", en: "Added one unlimited-approval block policy" }, diffUrl: "#diff/essentials" },
  { id: "u03", type: "new_release", target: { type: "policy", ref: "ammlp-remove-exit-asymmetry-warn" }, publishedAt: "2026-06-03",
    changelog: { ko: "비대칭 LP 청산(IL) 경고 신규 공개", en: "New: asymmetric LP exit (IL) warning" }, diffUrl: "#diff/ammlp-remove-exit-asymmetry-warn" },
  // 어제
  { id: "u04", type: "version_bump", target: { type: "policy", ref: "swap-price-impact-warn" }, fromVersion: "v1.7", publishedAt: "2026-06-02",
    changelog: { ko: "얇은 풀 프라이스 임팩트 추정 정확도 개선", en: "Better price-impact estimate on thin pools" }, diffUrl: "#diff/swap-price-impact-warn" },
  { id: "u05", type: "version_bump", target: { type: "package", ref: "drainer-shield" }, fromVersion: "v2.0", publishedAt: "2026-06-02",
    changelog: { ko: "Permit2 스펜더 검사 정책 편입", en: "Folded in a Permit2 spender check" }, diffUrl: "#diff/drainer-shield" },
  // 이번 주
  { id: "u06", type: "version_bump", target: { type: "policy", ref: "air-permit-on-held-token-deny" }, fromVersion: "v1.9", publishedAt: "2026-06-01",
    changelog: { ko: "permit 대상 토큰 보유분 판정 로직 교체", en: "Reworked held-balance detection for permits" }, diffUrl: "#diff/air-permit-on-held-token-deny" },
  { id: "u07", type: "new_release", target: { type: "policy", ref: "permit-allowance-horizon-warn" }, publishedAt: "2026-06-01",
    changelog: { ko: "퍼밋 유효기간 과다 경고 신규 공개", en: "New: excessive permit deadline warning" }, diffUrl: "#diff/permit-allowance-horizon-warn" },
  { id: "u08", type: "version_bump", target: { type: "policy", ref: "bridge-target-not-allowlisted-deny" }, fromVersion: "v1.6", publishedAt: "2026-05-31",
    changelog: { ko: "CCTP v2 도메인 허용목록 반영", en: "Picked up CCTP v2 domain allowlist" }, diffUrl: "#diff/bridge-target-not-allowlisted-deny" },
  { id: "u09", type: "new_release", target: { type: "policy", ref: "gov-delegatee-allowlist-deny" }, publishedAt: "2026-05-30",
    changelog: { ko: "거버넌스 위임 허용목록 차단 신규 공개", en: "New: governance delegate allowlist block" }, diffUrl: "#diff/gov-delegatee-allowlist-deny" },
  { id: "u10", type: "version_bump", target: { type: "policy", ref: "nft-untrusted-blur-root-deny" }, fromVersion: "v1.5", publishedAt: "2026-05-29",
    changelog: { ko: "Blur 신규 루트 서명 포맷 대응", en: "Handle Blur's new root signature format" }, diffUrl: "#diff/nft-untrusted-blur-root-deny" },
  // 이전
  { id: "u11", type: "version_bump", target: { type: "policy", ref: "aave-hf-floor-warn" }, fromVersion: "v1.4", publishedAt: "2026-05-27",
    changelog: { ko: "eMode 건전성 계수 보정", en: "Calibrated eMode health-factor coefficient" }, diffUrl: "#diff/aave-hf-floor-warn" },
  { id: "u12", type: "new_release", target: { type: "package", ref: "discipline-guard" }, publishedAt: "2026-05-25",
    changelog: { ko: "수령자·거래규율 가드 패키지 신규 공개", en: "New package: Recipient & Discipline Guard" }, diffUrl: "#diff/discipline-guard" },
  { id: "u13", type: "version_bump", target: { type: "package", ref: "liq-pack" }, fromVersion: "v1.9", publishedAt: "2026-05-24",
    changelog: { ko: "청산 임계 정책 2건 임계값 상향", en: "Raised thresholds on two liquidation policies" }, diffUrl: "#diff/liq-pack" },
  { id: "u14", type: "new_release", target: { type: "policy", ref: "bridge-min-out-haircut-warn" }, publishedAt: "2026-05-23",
    changelog: { ko: "브릿지 최소 수령액 헤어컷 경고 신규 공개", en: "New: bridge min-out haircut warning" }, diffUrl: "#diff/bridge-min-out-haircut-warn" },
  { id: "u15", type: "version_bump", target: { type: "policy", ref: "gas-cost-usd-cap-deny" }, fromVersion: "v1.3", publishedAt: "2026-05-22",
    changelog: { ko: "L2 가스 환산 오라클 교체", en: "Swapped the L2 gas-conversion oracle" }, diffUrl: "#diff/gas-cost-usd-cap-deny" },
  { id: "u16", type: "version_bump", target: { type: "policy", ref: "nft-setapprovalforall-conduit-warn" }, fromVersion: "v1.1", publishedAt: "2026-05-21",
    changelog: { ko: "Seaport conduit 키 검사 범위 확장", en: "Widened Seaport conduit-key checks" }, diffUrl: "#diff/nft-setapprovalforall-conduit-warn" },
];

// 변경 종류 태그 라벨
const CHANGE_KIND = {
  threshold: { ko: "임계값 조정", en: "Threshold" },
  "field-add": { ko: "필드 추가", en: "Field added" },
  scope: { ko: "적용 범위", en: "Scope" },
  fix: { ko: "버그 수정", en: "Fix" },
  new: { ko: "신규", en: "New" },
};
// 게시자 등급 메타 (공식 sage / 인증 slate / 커뮤니티 중립)
const PUBLISHER_META = {
  official: { ko: "공식", en: "Official", tone: "official" },
  verified: { ko: "인증", en: "Verified", tone: "verified" },
  community: { ko: "커뮤니티", en: "Community", tone: "community" },
};
// 항목별 출처/검증 메타 (단일 출처 — 시드에 병합). author.handle은 커뮤니티 핸들과 일치.
const OFFICIAL = { handle: "wdf", displayName: "Wallet Defense Force" };
const UPDATE_META = {
  u01: { author: OFFICIAL, publisher: "official", audited: true, changeKind: ["scope"] },
  u02: { author: OFFICIAL, publisher: "official", audited: true, changeKind: ["field-add"] },
  u03: { author: OFFICIAL, publisher: "official", audited: true, changeKind: ["new"] },
  u04: { author: { handle: "node_runner", displayName: "Node Runner" }, publisher: "verified", audited: true, changeKind: ["fix"] },
  u05: { author: OFFICIAL, publisher: "official", audited: true, changeKind: ["field-add"] },
  u06: { author: OFFICIAL, publisher: "official", audited: true, changeKind: ["fix"] },
  u07: { author: { handle: "vault.eth", displayName: "Vault" }, publisher: "verified", audited: true, changeKind: ["new"] },
  u08: { author: OFFICIAL, publisher: "official", audited: true, changeKind: ["scope"] },
  u09: { author: { handle: "cowswapper", displayName: "Cow Swapper" }, publisher: "community", audited: false, changeKind: ["new"] },
  u10: { author: OFFICIAL, publisher: "official", audited: true, changeKind: ["fix"] },
  u11: { author: { handle: "node_runner", displayName: "Node Runner" }, publisher: "verified", audited: true, changeKind: ["threshold"] },
  u12: { author: OFFICIAL, publisher: "official", audited: true, changeKind: ["new"] },
  u13: { author: OFFICIAL, publisher: "official", audited: true, changeKind: ["threshold"] },
  u14: { author: { handle: "frog.eth", displayName: "Frog" }, publisher: "community", audited: false, changeKind: ["new"] },
  u15: { author: OFFICIAL, publisher: "official", audited: true, changeKind: ["scope"] },
  u16: { author: OFFICIAL, publisher: "official", audited: true, changeKind: ["scope"] },
};

// 대상 해석 + toVersion·출처·검증 주입 (존재하지 않는 ref는 제외 → 안전)
function resolveUpdate(u) {
  const t = u.target;
  let name, domain, exists, evalClass;
  if (t.type === "package") {
    const pk = Market.PKG_BY_ID[t.ref];
    exists = !!pk; if (pk) { name = pk.name; domain = pk.primaryDomain; evalClass = "covered"; }
  } else {
    const p = Market.BY_SLUG[t.ref];
    exists = !!p; if (p) { name = p.name; domain = p.domain; evalClass = p.evalClass; }
  }
  const meta = UPDATE_META[u.id] || { author: OFFICIAL, publisher: "official", audited: true, changeKind: [] };
  return Object.assign({}, u, meta, { _exists: exists, _name: name, _domain: domain, evalClass: evalClass, toVersion: Market.versionFor(t.ref) });
}
const SEED_UPDATES = SEED_UPDATES_RAW.map(resolveUpdate).filter((u) => u._exists && u.toVersion);

// 날짜 버킷
function dateBucket(iso, locale) {
  const d = new Date(iso + "T00:00:00Z");
  const days = Math.round((CMTY_NOW - d) / 86400000);
  if (days <= 0) return locale === "en" ? "Today" : "오늘";
  if (days === 1) return locale === "en" ? "Yesterday" : "어제";
  if (days < 7) return locale === "en" ? "This week" : "이번 주";
  return locale === "en" ? "Earlier" : "이전";
}
const BUCKET_ORDER_KO = ["오늘", "어제", "이번 주", "이전"];
const BUCKET_ORDER_EN = ["Today", "Yesterday", "This week", "Earlier"];

// 플레이스홀더 Cedar unified diff 생성
function makeDiff(u) {
  const ref = u.target.ref;
  const p = u.target.type === "policy" ? Market.BY_SLUG[ref] : null;
  const intent = p && p.intents[0] ? p.intents[0] : (p ? p.domain : "risk");
  const dom = u._domain || "security";
  if (u.type === "new_release") {
    return [
      { s: "h", t: "--- /dev/null" },
      { s: "h", t: "+++ " + ref + " " + u.toVersion },
      { s: "+", t: 'permit (principal, action == Action::"signTransaction", resource)' },
      { s: "+", t: "when { resource.domain == \"" + dom + "\" }" },
      { s: "+", t: "unless { context.risk.\"" + intent + "\" >= 0.40 };" },
      { s: " ", t: "// severity: " + (p ? p.severity : "warn") },
    ];
  }
  return [
    { s: "h", t: "--- " + ref + " " + (u.fromVersion || "") },
    { s: "h", t: "+++ " + ref + " " + u.toVersion },
    { s: " ", t: "unless {" },
    { s: "-", t: "  context.risk.\"" + intent + "\" >= 0.40" },
    { s: "+", t: "  context.risk.\"" + intent + "\" >= 0.35" },
    { s: " ", t: "  resource.domain == \"" + dom + "\"" },
    { s: "-", t: "  // legacy threshold" },
    { s: "+", t: "  // tightened threshold (" + u.toVersion + ")" },
    { s: " ", t: "};" },
  ];
}

Object.assign(window, { SEED_UPDATES, SEED_UPDATES_RAW, dateBucket, BUCKET_ORDER_KO, BUCKET_ORDER_EN, makeDiff, CHANGE_KIND, PUBLISHER_META });
