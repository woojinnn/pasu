// One-shot generator: Curve liquidity gauge surface (V3, staking domain).
//   8 mainnet pool gauges (tied to the onboarded stableswap-ng + cryptoswap pools).
//   Multi-address: ONE manifest per function, chain_to_addresses = all 8 gauges
//   (the LP is identified by the gauge venue, so no per-gauge baking needed).
// Outputs:
//   registryV2/surface/curve/gauge.coverage.json
//   registryV2/manifests/curve/gauge/<fn>@1.0.0.json
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { toFunctionSelector } from "viem";

const ROOT = "/Users/jhy/Desktop/Dambi/dambi-registry-v2";
const SURF = `${ROOT}/registryV2/surface/curve`;
const MAN = `${ROOT}/registryV2/manifests/curve/gauge`;

// 8 mainnet pool gauges (Curve API getAllGauges, live, swap==my onboarded pools).
const GAUGES = [
  "0x96424e6b5eaafe0c3b36ca82068d574d44be4e3c", // FRAX+crvUSD
  "0xea012f5b25fa0d8e46123b85f585d0a5075e96b5", // wstETH+rETH+sfrxETH
  "0x60d3d7ebbc44dc810a743703184f062d00e6db7e", // crvUSD+tBTC+wstETH
  "0x8d867bef70c6733ff25cc0d1caa8aa6c38b24817", // crvUSD+WETH+CRV
  "0x1e4b83f6bfe9dbeb6d5b92a5237e5c18a44176f4", // GHO+cbBTC+WETH
  "0x4e21418095d32d15c6e2b96a9910772613a50d50", // WETH+frxETH
  "0x5010263ac1978297f56048c7d2b02316a3435404", // WBTC+tBTC
  "0xf29fff074f5cf755b55fbb3eb10a29203ac91ea2", // USDT+WBTC+WETH
];

const sigOf = (fn) => `${fn.name}(${(fn.inputs || []).map((i) => i.type).join(",")})`;
const fragment = (fn) => ({
  function_name: fn.name,
  abi: { name: fn.name, type: "function", stateMutability: fn.stateMutability, inputs: (fn.inputs || []).map((i) => ({ name: i.name, type: i.type })) },
});
const venue = () => ({ name: "curve_gauge", chain: "$chain", gauge: "$to" });
const gaugeDeposit = (extra) => ({ domain: "staking", staking: { action: "gauge_deposit", gauge_deposit: { venue: venue(), amount: "$args._value", ...extra } } });
const gaugeWithdraw = () => ({ domain: "staking", staking: { action: "gauge_withdraw", gauge_withdraw: { venue: venue(), amount: "$args._value" } } });
const claim = (extra) => ({ domain: "staking", staking: { action: "claim_rewards", claim_rewards: { venue: venue(), gauges: [], ...extra } } });

const ITEMS = [
  { file: "deposit",            sig: "deposit(uint256)",                 body: gaugeDeposit({}) },
  { file: "deposit-for",        sig: "deposit(uint256,address)",         body: gaugeDeposit({ on_behalf_of: "$args._addr" }) },
  { file: "deposit-claim",      sig: "deposit(uint256,address,bool)",    body: gaugeDeposit({ on_behalf_of: "$args._addr" }) },
  { file: "withdraw",           sig: "withdraw(uint256)",                body: gaugeWithdraw() },
  { file: "withdraw-claim",     sig: "withdraw(uint256,bool)",           body: gaugeWithdraw() },
  { file: "claim_rewards",      sig: "claim_rewards()",                  body: claim({}) },
  { file: "claim_rewards-for",  sig: "claim_rewards(address)",           body: claim({ on_behalf_of: "$args._addr" }) },
  { file: "claim_rewards-to",   sig: "claim_rewards(address,address)",   body: claim({ on_behalf_of: "$args._addr", recipient: "$args._receiver" }) },
];

const EXCLUDE = {
  "initialize(address)": "factory init, not a user op.",
  "user_checkpoint(address)": "keeper — boost checkpoint.",
  "claimable_tokens(address)": "view-like (state-updating getter), not a user write.",
  "set_rewards_receiver(address)": "user config (reward forwarding); low-value, deferred.",
  "kick(address)": "keeper — kick a stale boost.",
  "transfer(address,uint256)": "ERC20 gauge-token transfer — token domain.",
  "transferFrom(address,address,uint256)": "ERC20 gauge-token transferFrom — token domain.",
  "approve(address,uint256)": "ERC20 gauge-token approve — token domain.",
  "increaseAllowance(address,uint256)": "ERC20 allowance — token domain.",
  "decreaseAllowance(address,uint256)": "ERC20 allowance — token domain.",
  "add_reward(address,address)": "reward-manager admin.",
  "set_reward_distributor(address,address)": "reward-manager admin.",
  "deposit_reward_token(address,uint256)": "reward-distributor admin (funds the reward stream).",
  "set_killed(bool)": "admin — kill switch.",
};
const COVER_REASON = "user gauge stake/unstake/claim (staking domain) — GaugeDeposit/GaugeWithdraw (LP identified by gauge venue) / ClaimRewards (gauge multi-reward, no reward_token). Multi-address: one manifest covers all 8 pool gauges.";

const abi = JSON.parse(readFileSync(`${SURF}/gauge.abi.json`, "utf8")).abi;
const mutating = abi.filter((e) => e.type === "function" && (e.stateMutability === "nonpayable" || e.stateMutability === "payable"));
const bySig = new Map(mutating.map((fn) => [sigOf(fn), fn]));
const coverSigs = new Set(ITEMS.map((it) => it.sig));

// coverage.json (addresses[] = 8 gauges)
const functions = {};
for (const fn of mutating) {
  const sig = sigOf(fn);
  functions[toFunctionSelector(sig)] = {
    name: sig,
    decision: coverSigs.has(sig) ? "cover" : "exclude",
    reason: coverSigs.has(sig) ? COVER_REASON : (EXCLUDE[sig] || "non-user / admin / keeper — out of pre-sign scope."),
  };
}
writeFileSync(`${SURF}/gauge.coverage.json`, JSON.stringify({
  contract: "Curve-LiquidityGauge",
  chainId: 1,
  addresses: GAUGES,
  snapshot: "gauge.abi.json",
  note: `Curve liquidity gauges — 8 mainnet pool gauges (tied to onboarded pools). ${ITEMS.length} cover + ${mutating.length - ITEMS.length} exclude (ERC20 gauge-token / admin / keeper). Multi-address (H1).`,
  functions,
}, null, 2) + "\n");

// manifests (multi-address)
mkdirSync(MAN, { recursive: true });
let n = 0;
for (const it of ITEMS) {
  const fn = bySig.get(it.sig);
  if (!fn) throw new Error(`cover sig not in ABI: ${it.sig}`);
  const m = {
    type: "adapter_action",
    id: `curve/gauge/${it.file}@1.0.0`,
    publisher: "curve.fi",
    schema_version: "3",
    match: { selector: toFunctionSelector(it.sig), chain_to_addresses: { "1": GAUGES } },
    abi_fragment: fragment(fn),
    emit: { strategy: "single_emit", body: it.body },
    requires: { imperative: [], adapter_capabilities: ["token_metadata"], host_capabilities: [], extension: ">=0.1.0" },
  };
  writeFileSync(`${MAN}/${it.file}@1.0.0.json`, JSON.stringify(m, null, 2) + "\n");
  n++;
}
console.log(`gauge: ${mutating.length} mutating -> ${ITEMS.length} cover manifest (x${GAUGES.length} gauges multi-address) + coverage(${mutating.length} fns), wrote ${n} manifests`);
