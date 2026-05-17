// Pre-baked virtual transactions for the Policy Test panel.
//
// Calldata is hand-assembled with standard 4-byte selectors + 32-byte
// padded args so we don't pull viem/ethers just for encoding. Addresses
// are illustrative — the engine cares about shape, not on-chain identity.

export interface SampleTransaction {
  id: string;
  label: string;
  description: string;
  method: string;
  to: string;
  value: string;
  data: string;
}

// 4-byte selectors:
//   transfer(address,uint256)                       → 0xa9059cbb
//   approve(address,uint256)                        → 0x095ea7b3
//   swapExactTokensForTokens(uint256,uint256,address[],address,uint256)
//                                                   → 0x38ed1739
const PAD32 = (hex: string): string => hex.padStart(64, "0");
const PAD_ADDR = (addr: string): string => PAD32(addr.replace(/^0x/, "").toLowerCase());

const RECIPIENT = "0xabcdef0000000000000000000000000000000001";
const USDC = "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
const WETH = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const ROUTER = "0x7a250d5630b4cf539739df2c5dacb4c659f2488d";
const UINT256_MAX = "f".repeat(64);

function buildTransferData(recipient: string, amount: bigint): string {
  return (
    "0xa9059cbb" + PAD_ADDR(recipient) + PAD32(amount.toString(16))
  );
}

function buildApproveData(spender: string, amount: bigint | "max"): string {
  const amountHex = amount === "max" ? UINT256_MAX : PAD32(amount.toString(16));
  return "0x095ea7b3" + PAD_ADDR(spender) + amountHex;
}

// swapExactTokensForTokens: amountIn, amountOutMin, path[], to, deadline.
// We hand-roll the dynamic array offset since this is just illustrative.
function buildSwapData(
  amountIn: bigint,
  amountOutMin: bigint,
  path: string[],
  to: string,
  deadline: bigint,
): string {
  const offset = PAD32((5n * 32n).toString(16)); // path array starts after 5 head slots
  const pathLen = PAD32(BigInt(path.length).toString(16));
  const pathBody = path.map(PAD_ADDR).join("");
  return (
    "0x38ed1739" +
    PAD32(amountIn.toString(16)) +
    PAD32(amountOutMin.toString(16)) +
    offset +
    PAD_ADDR(to) +
    PAD32(deadline.toString(16)) +
    pathLen +
    pathBody
  );
}

export const SAMPLE_TRANSACTIONS: SampleTransaction[] = [
  {
    id: "custom",
    label: "Custom / 직접 입력",
    description: "필드를 자유롭게 편집합니다.",
    method: "eth_sendTransaction",
    to: "0x0000000000000000000000000000000000000002",
    value: "0x0",
    data: "0x",
  },
  {
    id: "eth-transfer",
    label: "ETH 직접 전송",
    description: "value=1 ETH, data 비어있음.",
    method: "eth_sendTransaction",
    to: RECIPIENT,
    // 1 ether == 0xDE0B6B3A7640000
    value: "0xde0b6b3a7640000",
    data: "0x",
  },
  {
    id: "erc20-transfer",
    label: "ERC20 transfer (100 USDC)",
    description: "USDC contract에 transfer(recipient, 100_000000) 호출.",
    method: "eth_sendTransaction",
    to: USDC,
    value: "0x0",
    data: buildTransferData(RECIPIENT, 100_000000n),
  },
  {
    id: "erc20-approve-unlimited",
    label: "ERC20 approve unlimited",
    description: "Router에 USDC 무제한 approve (uint256.max) — 위험 패턴.",
    method: "eth_sendTransaction",
    to: USDC,
    value: "0x0",
    data: buildApproveData(ROUTER, "max"),
  },
  {
    id: "uniswap-v2-swap",
    label: "Uniswap V2 swap (1 WETH → USDC)",
    description: "swapExactTokensForTokens(1 WETH, 0, [WETH,USDC], to, +1h).",
    method: "eth_sendTransaction",
    to: ROUTER,
    value: "0x0",
    data: buildSwapData(
      // 1e18 wei == 1 WETH
      1_000_000_000_000_000_000n,
      0n,
      [WETH, USDC],
      RECIPIENT,
      BigInt(Math.floor(Date.now() / 1000) + 3600),
    ),
  },
];

export function findSample(id: string): SampleTransaction | undefined {
  return SAMPLE_TRANSACTIONS.find((s) => s.id === id);
}
