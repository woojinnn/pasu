/**
 * Aave V3 receipt token resolver — 3 source kinds:
 *  - `aave_v3:atokens`         → interest-bearing aTokens (aEthUSDC, ...)
 *  - `aave_v3:variable_debts`  → variableDebtTokenAddress per reserve
 *  - `aave_v3:stable_debts`    → stableDebtTokenAddress per reserve
 *
 * Resolution flow per chain:
 *   1. `Pool.getReservesList(): address[]`  (single eth_call)
 *   2. Multicall3 batch: `Pool.getReserveData(asset): ReserveData` for each reserve
 *   3. Extract `aTokenAddress` / `variableDebtTokenAddress` / `stableDebtTokenAddress`
 *
 * Cached under `cache/protocol-sources/aave_v3/<chainId>.<scope>.json`.
 *
 * 1차 출처: Aave V3 deployed contracts
 *   - https://docs.aave.com/developers/deployed-contracts/v3-mainnet
 *   - Pool address verified per-chain below.
 */

import { parseAbi, type Address } from "viem";

import { rpcClient, readOrFetch } from "./rpc.ts";
import type {
  CacheEntry,
  Hex,
  ProtocolResolver,
  ProtocolSourceKind,
  ResolverOpts,
} from "./types.ts";

// ---------------------------------------------------------------------------
// Per-chain Pool addresses (Aave V3 verified deployments)
// ---------------------------------------------------------------------------

const AAVE_V3_POOL_ADDRESSES: Record<number, Address> = {
  1: "0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2", // Ethereum mainnet
  10: "0x794a61358d6845594f94dc1db02a252b5b4814ad", // Optimism
  8453: "0xa238dd80c259a72e81d7e4664a9801593f98d1c5", // Base
  42161: "0x794a61358d6845594f94dc1db02a252b5b4814ad", // Arbitrum (shares deploy with OP)
};

// ---------------------------------------------------------------------------
// Aave V3 Pool ABI (minimal — only the two reads we need)
// ---------------------------------------------------------------------------

/**
 * `getReserveData` returns a `ReserveData` struct. We only consult the three
 * token address fields (a / variableDebt / stableDebt) but viem requires the
 * full struct shape for ABI decoding to work.
 *
 * Field order matches Aave V3 `DataTypes.ReserveData` exactly:
 *   configuration | liquidityIndex | currentLiquidityRate | variableBorrowIndex |
 *   currentVariableBorrowRate | currentStableBorrowRate | lastUpdateTimestamp |
 *   id | aTokenAddress | stableDebtTokenAddress | variableDebtTokenAddress |
 *   interestRateStrategyAddress | accruedToTreasury | unbacked | isolationModeTotalDebt
 */
const POOL_ABI = parseAbi([
  "function getReservesList() view returns (address[])",
  "function getReserveData(address asset) view returns ((uint256 data) configuration, uint128 liquidityIndex, uint128 currentLiquidityRate, uint128 variableBorrowIndex, uint128 currentVariableBorrowRate, uint128 currentStableBorrowRate, uint40 lastUpdateTimestamp, uint16 id, address aTokenAddress, address stableDebtTokenAddress, address variableDebtTokenAddress, address interestRateStrategyAddress, uint128 accruedToTreasury, uint128 unbacked, uint128 isolationModeTotalDebt)",
]);

// ---------------------------------------------------------------------------
// Reserve enumeration — single call → reserve list, single multicall batch → struct[]
// ---------------------------------------------------------------------------

interface ReserveSnapshot {
  reserve: Address;
  aToken: Address;
  variableDebtToken: Address;
  stableDebtToken: Address;
}

async function fetchReserves(chainId: number): Promise<{
  pool: Address;
  blockNumber: number;
  reserves: ReserveSnapshot[];
}> {
  const pool = AAVE_V3_POOL_ADDRESSES[chainId];
  if (!pool) {
    throw new Error(`aave_v3: no Pool address configured for chainId ${chainId}`);
  }

  const blockNumber = await rpcClient.blockNumber(chainId);

  // Step 1 — getReservesList
  const reserves = (await rpcClient.multicall<readonly Address[][]>(chainId, [
    {
      address: pool,
      abi: POOL_ABI,
      functionName: "getReservesList",
    },
  ]))[0];

  if (reserves.length === 0) {
    return { pool, blockNumber: Number(blockNumber), reserves: [] };
  }

  // Step 2 — batch getReserveData for each reserve
  const reserveDataCalls = reserves.map((asset) => ({
    address: pool,
    abi: POOL_ABI,
    functionName: "getReserveData" as const,
    args: [asset] as const,
  }));

  const datas = await rpcClient.multicall<
    readonly {
      aTokenAddress: Address;
      variableDebtTokenAddress: Address;
      stableDebtTokenAddress: Address;
    }[]
  >(chainId, reserveDataCalls);

  const snapshots: ReserveSnapshot[] = reserves.map((reserve, i) => ({
    reserve,
    aToken: datas[i].aTokenAddress,
    variableDebtToken: datas[i].variableDebtTokenAddress,
    stableDebtToken: datas[i].stableDebtTokenAddress,
  }));

  return { pool, blockNumber: Number(blockNumber), reserves: snapshots };
}

// ---------------------------------------------------------------------------
// Resolver factory — one per source kind, all share the same fetchReserves snapshot
// ---------------------------------------------------------------------------

const NULL_ADDRESS = "0x0000000000000000000000000000000000000000";

function makeResolver(
  scope: ProtocolSourceKind,
  pick: (snap: ReserveSnapshot) => Address,
): ProtocolResolver {
  return {
    source: scope,
    async resolve(chainId: number, opts: ResolverOpts): Promise<Hex[]> {
      const entry = await readOrFetch(scope, chainId, opts.forceRefresh, async () => {
        const { pool, blockNumber, reserves } = await fetchReserves(chainId);
        const addresses = reserves
          .map(pick)
          .filter((addr) => addr.toLowerCase() !== NULL_ADDRESS) // Aave deprecates stable rate per reserve — null when N/A
          .map((addr) => addr.toLowerCase() as Hex);
        const fresh: CacheEntry = {
          scope,
          chainId,
          addresses: addresses.sort(),
          synced_at: Math.floor(Date.now() / 1000),
          source_block: blockNumber,
          pool_address: pool.toLowerCase() as Hex,
          entry_count: reserves.length,
        };
        return fresh;
      });
      return entry.addresses;
    },
  };
}

export const atokensResolver = makeResolver(
  "aave_v3:atokens",
  (snap) => snap.aToken,
);

export const variableDebtsResolver = makeResolver(
  "aave_v3:variable_debts",
  (snap) => snap.variableDebtToken,
);

export const stableDebtsResolver = makeResolver(
  "aave_v3:stable_debts",
  (snap) => snap.stableDebtToken,
);
