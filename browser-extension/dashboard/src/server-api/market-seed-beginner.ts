/**
 * ⚠️ 임시 시드 — PASU Beginner Pack V1 (Wallet Guardians 공식 docs 기반).
 * 출처: https://wallet-guardians.gitbook.io/wallet-guardians-docs/standard-packages/pasu-beginner-pack-v1
 *
 * 정책 서버(8788)에 아직 실제 listing 이 거의 없어 마켓 화면을 확인하기 위한
 * 데모용 데이터다. server-api/market 의 listListings/getListing 이 빈 결과를
 * 돌려줄 때만 폴백으로 끼워 넣는다(아래 mergeSeed). 실제 데이터가 올라오면
 * 이 파일과 호출부(market.ts 의 SEED 분기)를 통째로 지우면 된다.
 */
import type {
  ListingDetail,
  ListingSummary,
  Review,
  SetMember,
} from "./market";

const RELEASED = Date.UTC(2026, 5, 10) / 1000; // 2026-06-10
const NOW = RELEASED;

/** 패키지에 묶인 정책 5종(문서 표 그대로). cedar_text 는 데모 placeholder. */
interface SeedPolicy {
  slug: string;
  code: string; // TOKEN-001 등 문서상 ID
  nameKo: string;
  nameEn: string;
  category: string;
  severity: "deny" | "warn";
  lineKo: string;
  lineEn: string;
  installs: number;
}

const POLICIES: SeedPolicy[] = [
  {
    slug: "unapproved-contract-token-max-approval",
    code: "TOKEN-001",
    nameKo: "미승인 컨트랙트 토큰 무제한 승인",
    nameEn: "Unapproved Contract Token Max Approval",
    category: "Token",
    severity: "warn",
    lineKo: "미승인 컨트랙트가 토큰 무제한 승인을 요청하면 경고합니다.",
    lineEn: "Warn when unapproved contracts request unlimited token approvals.",
    installs: 1280,
  },
  {
    slug: "swap-asset-redirect",
    code: "AMM-001",
    nameKo: "스왑 자산 리다이렉트",
    nameEn: "Swap Asset Redirect",
    category: "DEX",
    severity: "warn",
    lineKo: "스왑한 자산이 제3자에게 전송되면 경고합니다.",
    lineEn: "Alert if swap assets are sent to third parties.",
    installs: 940,
  },
  {
    slug: "burn-address-transfer",
    code: "TOKEN-002",
    nameKo: "소각 주소 전송",
    nameEn: "Burn Address Transfer",
    category: "Token",
    severity: "warn",
    lineKo: "토큰이 소각 주소(0x00…00, 0x00…dead)로 전송되면 경고합니다.",
    lineEn: "Warn when tokens transfer to burn addresses.",
    installs: 760,
  },
  {
    slug: "unapproved-marketplace-delegation",
    code: "NFT-001",
    nameKo: "미승인 마켓플레이스 위임",
    nameEn: "Unapproved Marketplace Delegation",
    category: "NFT",
    severity: "warn",
    lineKo: "미승인 마켓플레이스로 NFT 위임이 일어나면 경고합니다.",
    lineEn: "Alert on NFT delegation to unapproved marketplaces.",
    installs: 610,
  },
  {
    slug: "unsupported-protocol",
    code: "OTHER-001",
    nameKo: "미지원 프로토콜",
    nameEn: "Unsupported Protocol",
    category: "Others",
    severity: "warn",
    lineKo: "미지원 프로토콜로 서명 요청이 오면 경고합니다.",
    lineEn: "Warn if signing requests use unsupported protocols.",
    installs: 430,
  },
  // ── [Token] Beginner Shield 전용 신규 정책 3종 (Wallet Guardians docs) ──
  {
    slug: "token-self-contract-transfer-warn",
    code: "TOKEN-003",
    nameKo: "토큰 컨트랙트 자기 전송",
    nameEn: "Token Contract Self-Transfer",
    category: "Token",
    severity: "warn",
    lineKo: "토큰을 그 토큰의 컨트랙트 주소로 보내면 경고합니다.",
    lineEn: "Warn when tokens are sent to their own contract address.",
    installs: 540,
  },
  {
    slug: "permit2-max-signature-warn",
    code: "TOKEN-004",
    nameKo: "Permit2 최대 서명",
    nameEn: "Permit2 Maximum Signature",
    category: "Token",
    severity: "warn",
    lineKo: "Permit2 최대 한도 서명 요청이 오면 경고합니다.",
    lineEn: "Warn on maximum Permit2 allowance signing requests.",
    installs: 690,
  },
  {
    slug: "malicious-address-approval-deny",
    code: "TOKEN-005",
    nameKo: "악성 주소 승인 차단",
    nameEn: "Malicious Address Approval",
    category: "Token",
    severity: "deny",
    lineKo: "알려진 악성 주소로의 토큰 승인 요청을 차단합니다.",
    lineEn: "Block token approval requests to known malicious addresses.",
    installs: 820,
  },
];

const PACKAGE_SLUG = "pasu-beginner-pack-v1";

/** [Token] Beginner Shield — Wallet Guardians 공식 docs 기반 두 번째 데모 패키지.
 * 출처: .../standard-packages/market-offered-packages/erc-20/token (= [Token] 기본 정책 모음) */
const TOKEN_SHIELD_SLUG = "token-beginner-shield";
const TOKEN_SHIELD_MEMBERS = [
  "unapproved-contract-token-max-approval", // TOKEN-001 (재사용)
  "burn-address-transfer", // TOKEN-002 (재사용)
  "token-self-contract-transfer-warn", // TOKEN-003
  "permit2-max-signature-warn", // TOKEN-004
  "malicious-address-approval-deny", // TOKEN-005
];

function seedCedar(p: SeedPolicy): string {
  return `// ${p.code} — ${p.nameEn}\n// severity: ${p.severity}\n// (데모 placeholder — 실제 Cedar 원문은 게시 시 주입됩니다)\npermit (\n  principal,\n  action == Action::"signTransaction",\n  resource\n) when {\n  context.flagged == true\n};`;
}

function policySummary(p: SeedPolicy): ListingSummary {
  return {
    id: `seed-${p.slug}`,
    slug: p.slug,
    kind: "policy",
    publisher_id: "seed-wallet-guardians",
    publisher_tier: "official",
    publisher_email: undefined,
    display_name: { en: p.nameEn, ko: p.nameKo },
    description: { en: p.lineEn, ko: p.lineKo },
    category: p.category,
    severity: p.severity,
    status: "published",
    current_version: "1.0.0",
    created_at: RELEASED,
    updated_at: NOW,
    install_count: p.installs,
    rating_avg: 4.8,
    rating_count: 36,
    is_installed: false,
  };
}

function packageSummary(): ListingSummary {
  return {
    id: `seed-${PACKAGE_SLUG}`,
    slug: PACKAGE_SLUG,
    kind: "set",
    publisher_id: "seed-wallet-guardians",
    publisher_tier: "official",
    publisher_email: undefined,
    display_name: { en: "PASU Beginner Pack V1", ko: "PASU 입문자 팩 V1" },
    description: {
      en: "Protection package for Web3 newcomers — token approvals, transfers, swaps, and NFT trading.",
      ko: "Web3 입문자를 위한 보호 패키지 — 토큰 승인·전송·스왑·NFT 거래를 한 번에 지킵니다.",
    },
    status: "published",
    current_version: "1.0.0",
    created_at: RELEASED,
    updated_at: NOW,
    install_count: 2150,
    rating_avg: 4.9,
    rating_count: 58,
    is_installed: false,
  };
}

function tokenShieldSummary(): ListingSummary {
  return {
    id: `seed-${TOKEN_SHIELD_SLUG}`,
    slug: TOKEN_SHIELD_SLUG,
    kind: "set",
    publisher_id: "seed-wallet-guardians",
    publisher_tier: "official",
    publisher_email: undefined,
    display_name: { en: "[Token] Beginner Shield", ko: "[Token] 기본 정책 모음" },
    description: {
      en: "Prevents common mistakes by new Web3 users during token approvals, transfers, and signatures.",
      ko: "Web3에 갓 입문한 사용자가 첫 승인·첫 송금·첫 서명 시 겪을 수 있는 사고를 방지합니다.",
    },
    status: "published",
    current_version: "1.0.0",
    created_at: RELEASED,
    updated_at: NOW,
    install_count: 1640,
    rating_avg: 4.8,
    rating_count: 41,
    is_installed: false,
  };
}

const SEED_REVIEWS: Review[] = [
  {
    id: "seed-rev-1",
    listing_id: `seed-${PACKAGE_SLUG}`,
    user_id: "seed-user-1",
    version: "1.0.0",
    rating: 5,
    body: {
      en: "Great starter pack — caught an unlimited approval right away.",
      ko: "입문용으로 딱이에요. 무제한 승인을 바로 잡아줬습니다.",
    },
    helpful_count: 24,
    created_at: NOW,
  },
  {
    id: "seed-rev-2",
    listing_id: `seed-${PACKAGE_SLUG}`,
    user_id: "seed-user-2",
    version: "1.0.0",
    rating: 5,
    body: { en: "Low false positives, easy to set up.", ko: "오탐도 거의 없고 설정이 쉬워요." },
    helpful_count: 11,
    created_at: NOW,
  },
];

const TOKEN_SHIELD_REVIEWS: Review[] = [
  {
    id: "seed-ts-rev-1",
    listing_id: `seed-${TOKEN_SHIELD_SLUG}`,
    user_id: "seed-user-3",
    version: "1.0.0",
    rating: 5,
    body: {
      en: "Perfect for my first wallet — blocked a malicious approval on day one.",
      ko: "첫 지갑에 딱이에요. 첫날 악성 승인을 바로 막아줬습니다.",
    },
    helpful_count: 18,
    created_at: NOW,
  },
  {
    id: "seed-ts-rev-2",
    listing_id: `seed-${TOKEN_SHIELD_SLUG}`,
    user_id: "seed-user-4",
    version: "1.0.0",
    rating: 4,
    body: {
      en: "Good basics. The Permit2 warning saved me once.",
      ko: "기본기가 탄탄해요. Permit2 경고 덕분에 한 번 살았습니다.",
    },
    helpful_count: 7,
    created_at: NOW,
  },
];

/** 주어진 slug 목록을 SetMember[] 로(순서 유지, 없는 slug 는 스킵). */
function membersFor(slugs: string[]): SetMember[] {
  return slugs
    .map((s) => POLICIES.find((p) => p.slug === s))
    .filter((p): p is SeedPolicy => !!p)
    .map((p) => ({
      slug: p.slug,
      display_name: p.nameKo,
      cedar_text: seedCedar(p),
      manifest: undefined,
    }));
}

/** PASU Beginner Pack 멤버 = 처음 정의된 5종(기존 동작 유지). */
const BEGINNER_PACK_MEMBERS = [
  "unapproved-contract-token-max-approval",
  "swap-asset-redirect",
  "burn-address-transfer",
  "unapproved-marketplace-delegation",
  "unsupported-protocol",
];

/** 시드 listing 요약 전체(패키지 2종 + 정책 전부). */
export function seedListings(): ListingSummary[] {
  return [packageSummary(), tokenShieldSummary(), ...POLICIES.map(policySummary)];
}

/** slug 로 시드 상세를 찾는다(없으면 null). */
export function seedDetail(slug: string): ListingDetail | null {
  if (slug === PACKAGE_SLUG) {
    return {
      ...packageSummary(),
      latest_version: {
        listing_id: `seed-${PACKAGE_SLUG}`,
        version: "1.0.0",
        major: 1,
        minor: 0,
        patch: 0,
        members: membersFor(BEGINNER_PACK_MEMBERS),
        published_at: RELEASED,
      },
      recent_reviews: SEED_REVIEWS,
    };
  }
  if (slug === TOKEN_SHIELD_SLUG) {
    return {
      ...tokenShieldSummary(),
      latest_version: {
        listing_id: `seed-${TOKEN_SHIELD_SLUG}`,
        version: "1.0.0",
        major: 1,
        minor: 0,
        patch: 0,
        members: membersFor(TOKEN_SHIELD_MEMBERS),
        published_at: RELEASED,
      },
      recent_reviews: TOKEN_SHIELD_REVIEWS,
    };
  }
  const p = POLICIES.find((x) => x.slug === slug);
  if (!p) return null;
  return {
    ...policySummary(p),
    latest_version: {
      listing_id: `seed-${p.slug}`,
      version: "1.0.0",
      major: 1,
      minor: 0,
      patch: 0,
      cedar_text: seedCedar(p),
      published_at: RELEASED,
    },
    recent_reviews: [],
  };
}
