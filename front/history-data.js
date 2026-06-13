// history-data.js — placeholder verdict records for the 히스토리(History) screen.
// ⚠️ All addresses / dApps / policy names / selectors are PLACEHOLDERS for layout only.
// Schema (proposed; see spec 데이터 전제):
//   seq       monotonic ledger sequence (evidence / immutability cue)
//   ts        epoch ms (second-precise — evidence record, not a minute log)
//   verdict   'pass' | 'warn' | 'fail'
//   origin    dApp hostname
//   method    RPC method
//   fn        decoded function name (display)
//   contract  { addr, symbol }
//   selector  { sig, decoded }   selector hex + decoded key args
//   policy    { name, severity }
//   reason    one-sentence rule-perspective rationale
//   decision  (warn only) 'trusted' | 'cancelled'   ← warn.userTrusted

const SEC = 1000, MIN = 60 * SEC, HOUR = 60 * MIN, DAY = 24 * HOUR;
const T0 = Date.now();

const HISTORY_RECORDS = [
  { id: 'v001', seq: 48211, ts: T0 - 1 * MIN - 48 * SEC, verdict: 'fail',
    origin: 'app.uniswap.org', method: 'eth_sendTransaction', fn: 'approve',
    contract: { addr: '0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D', symbol: 'USDC' },
    selector: { sig: '0x095ea7b3', decoded: 'spender=0x7a25···488D, amount=MAX_UINT256' },
    policy: { name: 'rule#approve.cap', severity: 'fail' },
    reason: { ko: 'amount == MAX_UINT256 → 무제한 토큰 승인은 정책상 차단', en: 'amount == MAX_UINT256 → unlimited approval is blocked by policy' } },

  { id: 'v002', seq: 48207, ts: T0 - 8 * MIN - 31 * SEC, verdict: 'warn', decision: 'trusted',
    origin: 'app.aave.com', method: 'eth_sendTransaction', fn: 'supply',
    contract: { addr: '0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2', symbol: 'aWETH' },
    selector: { sig: '0x617ba037', decoded: 'asset=WETH, amount=12.0' },
    policy: { name: 'rule#gas.guard', severity: 'warn' },
    reason: { ko: 'gasPrice 82 gwei > 임계 65 gwei → 검토 요청', en: 'gasPrice 82 gwei > threshold 65 gwei → review requested' } },

  { id: 'v003', seq: 48201, ts: T0 - 20 * MIN - 6 * SEC, verdict: 'pass',
    origin: 'app.uniswap.org', method: 'eth_sendTransaction', fn: 'swap',
    contract: { addr: '0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45', symbol: 'UNI-RTR' },
    selector: { sig: '0x04e45aaf', decoded: 'tokenIn=USDC, tokenOut=WETH, amountIn=4,200' },
    policy: { name: 'rule#allowlist', severity: 'pass' },
    reason: { ko: '대상 컨트랙트가 허용 목록에 포함 → 통과', en: 'target contract is on the allowlist → pass' } },

  { id: 'v003b', seq: 48199, ts: T0 - 24 * MIN - 41 * SEC, verdict: 'pass',
    origin: 'app.lido.fi', method: 'eth_sendTransaction', fn: 'submit',
    contract: { addr: '0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84', symbol: 'stETH' },
    selector: { sig: '0xa1903eab', decoded: 'referral=0x0000···0000, value=2.0 ETH' },
    policy: { name: 'rule#daily.limit', severity: 'pass' },
    reason: { ko: '일일 누적 송금액이 한도 이내 → 통과', en: 'daily cumulative below limit → pass' } },

  { id: 'v004', seq: 48194, ts: T0 - 35 * MIN - 12 * SEC, verdict: 'warn', decision: 'cancelled',
    origin: 'opensea.io', method: 'eth_sendTransaction', fn: 'setApprovalForAll',
    contract: { addr: '0x495f947276749Ce646f68AC8c248420045cb7b5e', symbol: 'OS-NFT' },
    selector: { sig: '0xa22cb465', decoded: 'operator=0x1E00···aF3, approved=true' },
    policy: { name: 'rule#nft.approval', severity: 'warn' },
    reason: { ko: '전체 컬렉션 승인 요청 → 사용자 확인 필요', en: 'set-approval-for-all on a collection → user confirmation required' } },

  { id: 'v005', seq: 48188, ts: T0 - 51 * MIN - 53 * SEC, verdict: 'pass',
    origin: 'app.aave.com', method: 'eth_sendTransaction', fn: 'repay',
    contract: { addr: '0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2', symbol: 'aUSDC' },
    selector: { sig: '0x573ade81', decoded: 'asset=USDC, amount=1,500' },
    policy: { name: 'rule#daily.limit', severity: 'pass' },
    reason: { ko: '일일 누적 송금액이 한도 이내 → 통과', en: 'daily cumulative below limit → pass' } },

  { id: 'v006', seq: 48180, ts: T0 - 1 * HOUR - 14 * MIN - 9 * SEC, verdict: 'fail',
    origin: 'claim-rewards.xyz', method: 'eth_sendTransaction', fn: 'transfer',
    contract: { addr: '0x8c5fEcDC472E27Dc7C8a4Ed7C4dEAD9b5C2e3a91', symbol: '?' },
    selector: { sig: '0xa9059cbb', decoded: 'to=0x8c5f···a91, value=1.2 ETH' },
    policy: { name: 'rule#blocklist', severity: 'fail' },
    reason: { ko: '목적지 주소가 블록리스트(tier-1)에 매치 → 차단', en: 'destination matches blocklist (tier-1) → blocked' } },

  { id: 'v007', seq: 48173, ts: T0 - 1 * HOUR - 40 * MIN - 27 * SEC, verdict: 'pass',
    origin: 'app.1inch.io', method: 'eth_signTypedData_v4', fn: 'Permit',
    contract: { addr: '0x111111125421cA6dc452d289314280a0f8842A65', symbol: 'USDT' },
    selector: { sig: '—', decoded: 'EIP-712 Permit · spender=0x1111···2A65' },
    policy: { name: 'rule#sig.scheme', severity: 'pass' },
    reason: { ko: '서명 스킴이 EIP-712 표준에 부합 → 통과', en: 'signature scheme matches EIP-712 → pass' } },

  { id: 'v007b', seq: 48169, ts: T0 - 1 * HOUR - 52 * MIN - 4 * SEC, verdict: 'pass',
    origin: 'app.uniswap.org', method: 'eth_sendTransaction', fn: 'swap',
    contract: { addr: '0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45', symbol: 'UNI-RTR' },
    selector: { sig: '0x04e45aaf', decoded: 'tokenIn=DAI, tokenOut=USDC, amountIn=2,000' },
    policy: { name: 'rule#allowlist', severity: 'pass' },
    reason: { ko: '대상 컨트랙트가 허용 목록에 포함 → 통과', en: 'target contract is on the allowlist → pass' } },

  { id: 'v008', seq: 48161, ts: T0 - 2 * HOUR - 5 * MIN - 38 * SEC, verdict: 'warn', decision: 'trusted',
    origin: 'app.gmx.io', method: 'eth_sendTransaction', fn: 'increasePosition',
    contract: { addr: '0x489ee077994B6658eAfA855C308275EAd8097C4A', symbol: 'GMX-VAULT' },
    selector: { sig: '0xf2b9fdb8', decoded: 'sizeDelta=25,000, leverage=12x' },
    policy: { name: 'rule#leverage.cap', severity: 'warn' },
    reason: { ko: '레버리지 12x > 권장 10x → 검토 요청', en: 'leverage 12x > recommended 10x → review requested' } },

  { id: 'v009', seq: 48150, ts: T0 - 3 * HOUR - 22 * MIN - 15 * SEC, verdict: 'pass',
    origin: 'app.uniswap.org', method: 'eth_sendTransaction', fn: 'swap',
    contract: { addr: '0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45', symbol: 'UNI-RTR' },
    selector: { sig: '0x04e45aaf', decoded: 'tokenIn=WETH, tokenOut=DAI, amountIn=3.1' },
    policy: { name: 'rule#allowlist', severity: 'pass' },
    reason: { ko: '대상 컨트랙트가 허용 목록에 포함 → 통과', en: 'target contract is on the allowlist → pass' } },

  { id: 'v009b', seq: 48144, ts: T0 - 3 * HOUR - 49 * MIN - 2 * SEC, verdict: 'pass',
    origin: 'app.aave.com', method: 'eth_sendTransaction', fn: 'withdraw',
    contract: { addr: '0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2', symbol: 'aUSDC' },
    selector: { sig: '0x69328dec', decoded: 'asset=USDC, amount=400' },
    policy: { name: 'rule#daily.limit', severity: 'pass' },
    reason: { ko: '일일 누적 송금액이 한도 이내 → 통과', en: 'daily cumulative below limit → pass' } },

  { id: 'v010', seq: 48131, ts: T0 - 5 * HOUR - 11 * MIN - 47 * SEC, verdict: 'fail',
    origin: 'free-mint-airdrop.app', method: 'eth_sendTransaction', fn: 'approve',
    contract: { addr: '0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2', symbol: 'WETH' },
    selector: { sig: '0x095ea7b3', decoded: 'spender=0x9F3a···D21 (unverified), amount=MAX_UINT256' },
    policy: { name: 'rule#allowlist', severity: 'fail' },
    reason: { ko: '출처 도메인이 허용 목록에 없음 → 차단', en: 'origin domain not on allowlist → blocked' } },

  { id: 'v011', seq: 48118, ts: T0 - 8 * HOUR - 3 * MIN - 19 * SEC, verdict: 'pass',
    origin: 'app.lido.fi', method: 'eth_sendTransaction', fn: 'submit',
    contract: { addr: '0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84', symbol: 'stETH' },
    selector: { sig: '0xa1903eab', decoded: 'referral=0x0000···0000, value=4.0 ETH' },
    policy: { name: 'rule#daily.limit', severity: 'pass' },
    reason: { ko: '일일 누적 송금액이 한도 이내 → 통과', en: 'daily cumulative below limit → pass' } },

  { id: 'v012', seq: 48096, ts: T0 - 14 * HOUR - 28 * MIN - 50 * SEC, verdict: 'warn', decision: 'cancelled',
    origin: 'app.curve.fi', method: 'eth_sendTransaction', fn: 'add_liquidity',
    contract: { addr: '0xbEbc44782C7dB0a1A60Cb6fe97d0b483032FF1C7', symbol: '3pool' },
    selector: { sig: '0x4515cef3', decoded: 'amounts=[50k,50k,50k], min_mint=148k' },
    policy: { name: 'rule#slippage.guard', severity: 'warn' },
    reason: { ko: '예상 슬리피지 1.8% > 임계 1.0% → 검토 요청', en: 'expected slippage 1.8% > threshold 1.0% → review requested' } },

  { id: 'v013', seq: 48071, ts: T0 - 20 * HOUR - 50 * MIN - 33 * SEC, verdict: 'pass',
    origin: 'app.aave.com', method: 'eth_sendTransaction', fn: 'withdraw',
    contract: { addr: '0x87870Bca3F3fD6335C3F4ce8392D69350B4fA4E2', symbol: 'aUSDC' },
    selector: { sig: '0x69328dec', decoded: 'asset=USDC, amount=800' },
    policy: { name: 'rule#daily.limit', severity: 'pass' },
    reason: { ko: '일일 누적 송금액이 한도 이내 → 통과', en: 'daily cumulative below limit → pass' } },

  { id: 'v014', seq: 48040, ts: T0 - 1 * DAY - 4 * HOUR - 7 * MIN, verdict: 'fail',
    origin: 'app.uniswap.org', method: 'eth_sendTransaction', fn: 'transfer',
    contract: { addr: '0xdAC17F958D2ee523a2206206994597C13D831ec7', symbol: 'USDT' },
    selector: { sig: '0xa9059cbb', decoded: 'to=0x4B0a···F19, value=85,000' },
    policy: { name: 'rule#daily.limit', severity: 'fail' },
    reason: { ko: '단건 송금액이 일일 한도(50,000)를 초과 → 차단', en: 'single transfer exceeds daily limit (50,000) → blocked' } },

  { id: 'v015', seq: 47988, ts: T0 - 2 * DAY - 7 * HOUR - 22 * MIN, verdict: 'warn', decision: 'trusted',
    origin: 'app.gmx.io', method: 'eth_sendTransaction', fn: 'increasePosition',
    contract: { addr: '0x489ee077994B6658eAfA855C308275EAd8097C4A', symbol: 'GMX-VAULT' },
    selector: { sig: '0xf2b9fdb8', decoded: 'sizeDelta=40,000, leverage=11x' },
    policy: { name: 'rule#leverage.cap', severity: 'warn' },
    reason: { ko: '레버리지 11x > 권장 10x → 검토 요청', en: 'leverage 11x > recommended 10x → review requested' } },

  { id: 'v016', seq: 47930, ts: T0 - 3 * DAY - 1 * HOUR - 40 * MIN, verdict: 'pass',
    origin: 'app.1inch.io', method: 'eth_sendTransaction', fn: 'swap',
    contract: { addr: '0x1111111254EEB25477B68fb85Ed929f73A960582', symbol: '1INCH-RTR' },
    selector: { sig: '0x12aa3caf', decoded: 'srcToken=DAI, dstToken=USDC, amount=10,000' },
    policy: { name: 'rule#allowlist', severity: 'pass' },
    reason: { ko: '대상 컨트랙트가 허용 목록에 포함 → 통과', en: 'target contract is on the allowlist → pass' } },

  { id: 'v017', seq: 47864, ts: T0 - 4 * DAY - 9 * HOUR - 18 * MIN, verdict: 'fail',
    origin: 'metadrop-claim.io', method: 'eth_sendTransaction', fn: 'setApprovalForAll',
    contract: { addr: '0x495f947276749Ce646f68AC8c248420045cb7b5e', symbol: 'OS-NFT' },
    selector: { sig: '0xa22cb465', decoded: 'operator=0xBd3f···c02 (drainer-flagged), approved=true' },
    policy: { name: 'rule#blocklist', severity: 'fail' },
    reason: { ko: 'operator 주소가 드레이너 블록리스트에 매치 → 차단', en: 'operator matches drainer blocklist → blocked' } },

  { id: 'v018', seq: 47799, ts: T0 - 5 * DAY - 2 * HOUR - 55 * MIN, verdict: 'pass',
    origin: 'app.ens.domains', method: 'eth_sendTransaction', fn: 'register',
    contract: { addr: '0x253553366Da8546fC250F225fe3d25d0C782303b', symbol: 'ENS-CTRL' },
    selector: { sig: '0x74694a2b', decoded: 'name="acme", duration=1y' },
    policy: { name: 'rule#sig.scheme', severity: 'pass' },
    reason: { ko: '표준 호출 · 정책 위반 없음 → 통과', en: 'standard call · no policy violation → pass' } },
];

window.HISTORY_RECORDS = HISTORY_RECORDS;
