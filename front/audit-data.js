// audit-data.js — placeholder verdict records for the Audit screen.
// ⚠️ All addresses / dApps / policy names / selectors are PLACEHOLDERS for layout only.
// Schema (proposed; see spec 데이터 전제):
//   ts        epoch ms
//   verdict   'pass' | 'warn' | 'fail'
//   origin    dApp hostname
//   method    RPC method
//   fn        decoded function name (display)
//   contract  { addr, symbol }
//   selector  { sig, decoded }   selector hex + decoded key args
//   policy    { name, severity }
//   reason    one-sentence rule-perspective rationale
//   decision  (warn only) 'trusted' | 'cancelled'   ← warn.userTrusted

const MIN = 60 * 1000, HOUR = 60 * MIN, DAY = 24 * HOUR;
const T0 = Date.now();

const AUDIT_RECORDS = [
  { id: 'v001', ts: T0 - 2 * MIN, verdict: 'fail',
    origin: 'app.uniswap.org', method: 'eth_sendTransaction', fn: 'approve',
    contract: { addr: '0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D', symbol: 'USDC' },
    selector: { sig: '0x095ea7b3', decoded: 'spender=0x7a25···488D, amount=MAX_UINT256' },
    policy: { name: 'rule#approve.cap', severity: 'fail' },
    reason: { ko: 'amount == MAX_UINT256 → 무제한 토큰 승인은 정책상 차단', en: 'amount == MAX_UINT256 → unlimited approval is blocked by policy' } },

  { id: 'v002', ts: T0 - 9 * MIN, verdict: 'warn', decision: 'trusted',
    origin: 'app.aave.com', method: 'eth_sendTransaction', fn: 'supply',
    contract: { addr: '0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2', symbol: 'aWETH' },
    selector: { sig: '0x617ba037', decoded: 'asset=WETH, amount=12.0' },
    policy: { name: 'rule#gas.guard', severity: 'warn' },
    reason: { ko: 'gasPrice 82 gwei > 임계 65 gwei → 검토 요청', en: 'gasPrice 82 gwei > threshold 65 gwei → review requested' } },

  { id: 'v003', ts: T0 - 21 * MIN, verdict: 'pass',
    origin: 'app.uniswap.org', method: 'eth_sendTransaction', fn: 'swap',
    contract: { addr: '0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45', symbol: 'UNI-RTR' },
    selector: { sig: '0x04e45aaf', decoded: 'tokenIn=USDC, tokenOut=WETH, amountIn=4,200' },
    policy: { name: 'rule#allowlist', severity: 'pass' },
    reason: { ko: '대상 컨트랙트가 허용 목록에 포함 → 통과', en: 'target contract is on the allowlist → pass' } },

  { id: 'v004', ts: T0 - 36 * MIN, verdict: 'warn', decision: 'cancelled',
    origin: 'opensea.io', method: 'eth_sendTransaction', fn: 'setApprovalForAll',
    contract: { addr: '0x495f947276749Ce646f68AC8c248420045cb7b5e', symbol: 'OS-NFT' },
    selector: { sig: '0xa22cb465', decoded: 'operator=0x1E00···aF3, approved=true' },
    policy: { name: 'rule#nft.approval', severity: 'warn' },
    reason: { ko: '전체 컬렉션 승인 요청 → 사용자 확인 필요', en: 'set-approval-for-all on a collection → user confirmation required' } },

  { id: 'v005', ts: T0 - 52 * MIN, verdict: 'pass',
    origin: 'app.aave.com', method: 'eth_sendTransaction', fn: 'repay',
    contract: { addr: '0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2', symbol: 'aUSDC' },
    selector: { sig: '0x573ade81', decoded: 'asset=USDC, amount=1,500' },
    policy: { name: 'rule#daily.limit', severity: 'pass' },
    reason: { ko: '일일 누적 송금액이 한도 이내 → 통과', en: 'daily cumulative below limit → pass' } },

  { id: 'v006', ts: T0 - 1 * HOUR - 14 * MIN, verdict: 'fail',
    origin: 'claim-rewards.xyz', method: 'eth_sendTransaction', fn: 'transfer',
    contract: { addr: '0x8c5fEcDC472E27Dc7C8a4Ed7C4dEAD9b5C2e3a91', symbol: '?' },
    selector: { sig: '0xa9059cbb', decoded: 'to=0x8c5f···a91, value=1.2 ETH' },
    policy: { name: 'rule#blocklist', severity: 'fail' },
    reason: { ko: '목적지 주소가 블록리스트(tier-1)에 매치 → 차단', en: 'destination matches blocklist (tier-1) → blocked' } },

  { id: 'v007', ts: T0 - 1 * HOUR - 40 * MIN, verdict: 'pass',
    origin: 'app.1inch.io', method: 'eth_signTypedData_v4', fn: 'Permit',
    contract: { addr: '0x111111125421cA6dc452d289314280a0f8842A65', symbol: 'USDT' },
    selector: { sig: '—', decoded: 'EIP-712 Permit · spender=0x1111···2A65' },
    policy: { name: 'rule#sig.scheme', severity: 'pass' },
    reason: { ko: '서명 스킴이 EIP-712 표준에 부합 → 통과', en: 'signature scheme matches EIP-712 → pass' } },

  { id: 'v008', ts: T0 - 2 * HOUR - 5 * MIN, verdict: 'warn', decision: 'trusted',
    origin: 'app.gmx.io', method: 'eth_sendTransaction', fn: 'increasePosition',
    contract: { addr: '0x489ee077994B6658eAfA855C308275EAd8097C4A', symbol: 'GMX-VAULT' },
    selector: { sig: '0xf2b9fdb8', decoded: 'sizeDelta=25,000, leverage=12x' },
    policy: { name: 'rule#leverage.cap', severity: 'warn' },
    reason: { ko: '레버리지 12x > 권장 10x → 검토 요청', en: 'leverage 12x > recommended 10x → review requested' } },

  { id: 'v009', ts: T0 - 3 * HOUR - 22 * MIN, verdict: 'pass',
    origin: 'app.uniswap.org', method: 'eth_sendTransaction', fn: 'swap',
    contract: { addr: '0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45', symbol: 'UNI-RTR' },
    selector: { sig: '0x04e45aaf', decoded: 'tokenIn=WETH, tokenOut=DAI, amountIn=3.1' },
    policy: { name: 'rule#allowlist', severity: 'pass' },
    reason: { ko: '대상 컨트랙트가 허용 목록에 포함 → 통과', en: 'target contract is on the allowlist → pass' } },

  { id: 'v010', ts: T0 - 5 * HOUR - 11 * MIN, verdict: 'fail',
    origin: 'free-mint-airdrop.app', method: 'eth_sendTransaction', fn: 'approve',
    contract: { addr: '0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2', symbol: 'WETH' },
    selector: { sig: '0x095ea7b3', decoded: 'spender=0x9F3a···D21 (unverified), amount=MAX_UINT256' },
    policy: { name: 'rule#allowlist', severity: 'fail' },
    reason: { ko: '출처 도메인이 허용 목록에 없음 → 차단', en: 'origin domain not on allowlist → blocked' } },

  { id: 'v011', ts: T0 - 8 * HOUR - 3 * MIN, verdict: 'pass',
    origin: 'app.lido.fi', method: 'eth_sendTransaction', fn: 'submit',
    contract: { addr: '0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84', symbol: 'stETH' },
    selector: { sig: '0xa1903eab', decoded: 'referral=0x0000···0000, value=4.0 ETH' },
    policy: { name: 'rule#daily.limit', severity: 'pass' },
    reason: { ko: '일일 누적 송금액이 한도 이내 → 통과', en: 'daily cumulative below limit → pass' } },

  { id: 'v012', ts: T0 - 14 * HOUR - 28 * MIN, verdict: 'warn', decision: 'cancelled',
    origin: 'app.curve.fi', method: 'eth_sendTransaction', fn: 'add_liquidity',
    contract: { addr: '0xbEbc44782C7dB0a1A60Cb6fe97d0b483032FF1C7', symbol: '3pool' },
    selector: { sig: '0x4515cef3', decoded: 'amounts=[50k,50k,50k], min_mint=148k' },
    policy: { name: 'rule#slippage.guard', severity: 'warn' },
    reason: { ko: '예상 슬리피지 1.8% > 임계 1.0% → 검토 요청', en: 'expected slippage 1.8% > threshold 1.0% → review requested' } },

  { id: 'v013', ts: T0 - 20 * HOUR - 50 * MIN, verdict: 'pass',
    origin: 'app.aave.com', method: 'eth_sendTransaction', fn: 'withdraw',
    contract: { addr: '0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2', symbol: 'aUSDC' },
    selector: { sig: '0x69328dec', decoded: 'asset=USDC, amount=800' },
    policy: { name: 'rule#daily.limit', severity: 'pass' },
    reason: { ko: '일일 누적 송금액이 한도 이내 → 통과', en: 'daily cumulative below limit → pass' } },

  { id: 'v014', ts: T0 - 1 * DAY - 4 * HOUR, verdict: 'fail',
    origin: 'app.uniswap.org', method: 'eth_sendTransaction', fn: 'transfer',
    contract: { addr: '0xdAC17F958D2ee523a2206206994597C13D831ec7', symbol: 'USDT' },
    selector: { sig: '0xa9059cbb', decoded: 'to=0x4B0a···F19, value=85,000' },
    policy: { name: 'rule#daily.limit', severity: 'fail' },
    reason: { ko: '단건 송금액이 일일 한도(50,000)를 초과 → 차단', en: 'single transfer exceeds daily limit (50,000) → blocked' } },

  { id: 'v015', ts: T0 - 2 * DAY - 7 * HOUR, verdict: 'warn', decision: 'trusted',
    origin: 'app.gmx.io', method: 'eth_sendTransaction', fn: 'increasePosition',
    contract: { addr: '0x489ee077994B6658eAfA855C308275EAd8097C4A', symbol: 'GMX-VAULT' },
    selector: { sig: '0xf2b9fdb8', decoded: 'sizeDelta=40,000, leverage=11x' },
    policy: { name: 'rule#leverage.cap', severity: 'warn' },
    reason: { ko: '레버리지 11x > 권장 10x → 검토 요청', en: 'leverage 11x > recommended 10x → review requested' } },

  { id: 'v016', ts: T0 - 3 * DAY - 1 * HOUR, verdict: 'pass',
    origin: 'app.1inch.io', method: 'eth_sendTransaction', fn: 'swap',
    contract: { addr: '0x1111111254EEB25477B68fb85Ed929f73A960582', symbol: '1INCH-RTR' },
    selector: { sig: '0x12aa3caf', decoded: 'srcToken=DAI, dstToken=USDC, amount=10,000' },
    policy: { name: 'rule#allowlist', severity: 'pass' },
    reason: { ko: '대상 컨트랙트가 허용 목록에 포함 → 통과', en: 'target contract is on the allowlist → pass' } },

  { id: 'v017', ts: T0 - 4 * DAY - 9 * HOUR, verdict: 'fail',
    origin: 'metadrop-claim.io', method: 'eth_sendTransaction', fn: 'setApprovalForAll',
    contract: { addr: '0x495f947276749Ce646f68AC8c248420045cb7b5e', symbol: 'OS-NFT' },
    selector: { sig: '0xa22cb465', decoded: 'operator=0xBd3f···c02 (drainer-flagged), approved=true' },
    policy: { name: 'rule#blocklist', severity: 'fail' },
    reason: { ko: 'operator 주소가 드레이너 블록리스트에 매치 → 차단', en: 'operator matches drainer blocklist → blocked' } },

  { id: 'v018', ts: T0 - 5 * DAY - 2 * HOUR, verdict: 'pass',
    origin: 'app.ens.domains', method: 'eth_sendTransaction', fn: 'register',
    contract: { addr: '0x253553366Da8546fC250F225fe3d25d0C782303b', symbol: 'ENS-CTRL' },
    selector: { sig: '0x74694a2b', decoded: 'name="acme", duration=1y' },
    policy: { name: 'rule#sig.scheme', severity: 'pass' },
    reason: { ko: '표준 호출 · 정책 위반 없음 → 통과', en: 'standard call · no policy violation → pass' } },
];

// distinct dropdown sources derived from data
const AUDIT_ORIGINS = [...new Set(AUDIT_RECORDS.map(r => r.origin))].sort();
const AUDIT_POLICIES = [...new Set(AUDIT_RECORDS.map(r => r.policy.name))].sort();

window.AUDIT_RECORDS = AUDIT_RECORDS;
window.AUDIT_ORIGINS = AUDIT_ORIGINS;
window.AUDIT_POLICIES = AUDIT_POLICIES;
