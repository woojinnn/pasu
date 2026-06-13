// One-shot generator: Curve DAO staking/vote-escrow surface (V3).
//   veCRV (VotingEscrow) + Minter + GaugeController — mainnet singletons.
// Outputs:
//   registryV2/surface/curve/{vecrv,minter,gauge-controller}.coverage.json
//   registryV2/manifests/curve/{vecrv,minter,gauge-controller}/<fn>@1.0.0.json
// Selectors computed with viem (canonical `name(types)` keccak); abi_fragment
// pulled from the fetched Etherscan surface snapshot.
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { toFunctionSelector } from "viem";

const ROOT = "/Users/jhy/Desktop/Dambi/dambi-registry-v2";
const SURF = `${ROOT}/registryV2/surface/curve`;
const MAN = `${ROOT}/registryV2/manifests/curve`;

const VECRV = "0x5f3b5dfeb7b28cdbd7faba78963ee202a494e2a2";
const MINTER = "0xd061d61a4d941c39e5453435b6345dc261c2fce0";
const GC = "0x2f50d538606fa9edd2b11e2446beb18c9d5846bb";
const CRV = "0xd533a949740bb3306d119cc777fa900ba034cd52";

const erc20 = (addr) => ({ key: { standard: "erc20", chain: "$chain", address: addr } });
const sigOf = (fn) => `${fn.name}(${(fn.inputs || []).map((i) => i.type).join(",")})`;
const fragment = (fn) => ({
  function_name: fn.name,
  abi: {
    name: fn.name,
    type: "function",
    stateMutability: fn.stateMutability,
    inputs: (fn.inputs || []).map((i) => ({ name: i.name, type: i.type })),
  },
});

// venues (single-address per contract → $to)
const vecrvVenue = () => ({ name: "curve_voting_escrow", chain: "$chain", escrow: "$to" });
const minterVenue = () => ({ name: "curve_minter", chain: "$chain", minter: "$to" });
const gcVenue = () => ({ name: "curve_gauge_controller", chain: "$chain", controller: "$to" });

// ── COVER tables: file → { sig, body(args) } ───────────────────────────────
const COVER = {
  vecrv: {
    addr: VECRV,
    contract: "Curve-veCRV-VotingEscrow",
    items: [
      { file: "create_lock", sig: "create_lock(uint256,uint256)",
        body: { domain: "staking", staking: { action: "lock", lock: {
          venue: vecrvVenue(), token: erc20(CRV), amount: "$args._value", unlock_time: "$args._unlock_time" } } } },
      { file: "increase_amount", sig: "increase_amount(uint256)",
        body: { domain: "staking", staking: { action: "increase_lock_amount", increase_lock_amount: {
          venue: vecrvVenue(), token: erc20(CRV), amount: "$args._value" } } } },
      { file: "deposit_for", sig: "deposit_for(address,uint256)",
        body: { domain: "staking", staking: { action: "increase_lock_amount", increase_lock_amount: {
          venue: vecrvVenue(), token: erc20(CRV), amount: "$args._value", on_behalf_of: "$args._addr" } } } },
      { file: "increase_unlock_time", sig: "increase_unlock_time(uint256)",
        body: { domain: "staking", staking: { action: "increase_lock_time", increase_lock_time: {
          venue: vecrvVenue(), unlock_time: "$args._unlock_time" } } } },
      { file: "withdraw", sig: "withdraw()",
        body: { domain: "staking", staking: { action: "unlock", unlock: {
          venue: vecrvVenue(), token: erc20(CRV) } } } },
    ],
    excludeReason: {
      "commit_transfer_ownership(address)": "admin — ownership transfer (2-step commit).",
      "apply_transfer_ownership()": "admin — ownership transfer (2-step apply).",
      "commit_smart_wallet_checker(address)": "admin — smart-wallet allowlist checker.",
      "apply_smart_wallet_checker()": "admin — smart-wallet allowlist checker.",
      "checkpoint()": "keeper — no-arg global checkpoint, no user funds.",
      "changeController(address)": "admin — controller migration.",
    },
  },
  minter: {
    addr: MINTER,
    contract: "Curve-Minter",
    items: [
      { file: "mint", sig: "mint(address)",
        body: { domain: "staking", staking: { action: "claim_rewards", claim_rewards: {
          venue: minterVenue(), reward_token: erc20(CRV), gauges: ["$args.gauge_addr"] } } } },
      { file: "mint_for", sig: "mint_for(address,address)",
        body: { domain: "staking", staking: { action: "claim_rewards", claim_rewards: {
          venue: minterVenue(), reward_token: erc20(CRV), gauges: ["$args.gauge_addr"], on_behalf_of: "$args._for" } } } },
      { file: "mint_many", sig: "mint_many(address[8])",
        body: { domain: "staking", staking: { action: "claim_rewards", claim_rewards: {
          venue: minterVenue(), reward_token: erc20(CRV), gauges: "$args.gauge_addrs" } } } },
      { file: "toggle_approve_mint", sig: "toggle_approve_mint(address)",
        body: { domain: "permission", permission: { action: "protocol_authorization", protocol_authorization: {
          chain: "$chain", protocol: "$to", protocol_name: "curve_minter",
          permission: "operator", authorized: "$args.minting_user", is_authorized: true } } } },
    ],
    excludeReason: {},
  },
  "gauge-controller": {
    addr: GC,
    contract: "Curve-GaugeController",
    items: [
      { file: "vote_for_gauge_weights", sig: "vote_for_gauge_weights(address,uint256)",
        body: { domain: "staking", staking: { action: "vote_for_gauge", vote_for_gauge: {
          venue: gcVenue(), gauge: "$args._gauge_addr", weight_bp: "$args._user_weight" } } } },
    ],
    excludeReason: {
      "commit_transfer_ownership(address)": "admin — ownership transfer.",
      "apply_transfer_ownership()": "admin — ownership transfer.",
      "add_gauge(address,int128)": "admin — register a gauge.",
      "add_gauge(address,int128,uint256)": "admin — register a gauge (with weight).",
      "checkpoint()": "keeper — global checkpoint.",
      "checkpoint_gauge(address)": "keeper — per-gauge checkpoint.",
      "gauge_relative_weight_write(address)": "keeper — cache relative weight, no user funds.",
      "gauge_relative_weight_write(address,uint256)": "keeper — cache relative weight at time, no user funds.",
      "add_type(string)": "admin — add a gauge type.",
      "add_type(string,uint256)": "admin — add a gauge type (with weight).",
      "change_type_weight(int128,uint256)": "admin — set gauge-type weight.",
      "change_gauge_weight(address,uint256)": "admin — override a gauge weight.",
    },
  },
};

const COVER_REASON = {
  vecrv: "user veCRV vote-escrow op (staking domain) — Lock/IncreaseLockAmount/IncreaseLockTime/Unlock, venue=curve_voting_escrow{escrow}. Locked token = CRV (baked).",
  minter: "user CRV reward mint (staking ClaimRewards) / toggle_approve_mint (permission ProtocolAuthorization, operator). reward token = CRV (baked). toggle is a FLIP — modeled is_authorized:true (grant, the risk-relevant direction); static analysis can't read current state (documented limitation).",
  "gauge-controller": "user gauge-weight vote (staking VoteForGauge), venue=curve_gauge_controller{controller}. weight_bp = _user_weight (0–10000). Moves no funds.",
};

let manCount = 0;
for (const [key, spec] of Object.entries(COVER)) {
  const abi = JSON.parse(readFileSync(`${SURF}/${key}.abi.json`, "utf8"));
  const mutating = abi.filter(
    (e) => e.type === "function" && (e.stateMutability === "nonpayable" || e.stateMutability === "payable")
  );
  // sig → fn (last wins is fine; overloads disambiguated by full sig)
  const bySig = new Map(mutating.map((fn) => [sigOf(fn), fn]));
  const coverSigs = new Set(spec.items.map((it) => it.sig));

  // ── coverage.json ──
  const functions = {};
  for (const fn of mutating) {
    const sig = sigOf(fn);
    const sel = toFunctionSelector(sig);
    const cover = coverSigs.has(sig);
    functions[sel] = {
      name: sig,
      decision: cover ? "cover" : "exclude",
      reason: cover ? COVER_REASON[key] : (spec.excludeReason[sig] || "non-user / admin / keeper — out of pre-sign scope."),
    };
  }
  const coverage = {
    contract: spec.contract,
    chainId: 1,
    addresses: [spec.addr],
    snapshot: `${key}.abi.json`,
    note: `Curve ${spec.contract} mainnet singleton. ${spec.items.length} cover + ${mutating.length - spec.items.length} exclude (admin/keeper).`,
    functions,
  };
  writeFileSync(`${SURF}/${key}.coverage.json`, JSON.stringify(coverage, null, 2) + "\n");

  // ── manifests ──
  mkdirSync(`${MAN}/${key}`, { recursive: true });
  for (const it of spec.items) {
    const fn = bySig.get(it.sig);
    if (!fn) throw new Error(`${key}: cover sig not in ABI: ${it.sig}`);
    const sel = toFunctionSelector(it.sig);
    const m = {
      type: "adapter_action",
      id: `curve/${key}/${it.file}@1.0.0`,
      publisher: "curve.fi",
      schema_version: "3",
      match: { selector: sel, chain_to_addresses: { "1": [spec.addr] } },
      abi_fragment: fragment(fn),
      emit: { strategy: "single_emit", body: it.body },
      requires: { imperative: [], adapter_capabilities: ["token_metadata"], host_capabilities: [], extension: ">=0.1.0" },
    };
    writeFileSync(`${MAN}/${key}/${it.file}@1.0.0.json`, JSON.stringify(m, null, 2) + "\n");
    manCount++;
  }
  console.log(`${key}: ${mutating.length} mutating → ${spec.items.length} cover manifest + coverage(${mutating.length} fns)`);
}
console.log(`wrote ${manCount} manifests total`);
