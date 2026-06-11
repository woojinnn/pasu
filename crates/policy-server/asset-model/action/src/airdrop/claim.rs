use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::position::MerkleProof;
use policy_state::primitives::{Address, ChainId, ProtocolRef, Time, U256};
use policy_state::token::TokenRef;
use policy_state::LiveField;

use crate::Bytes;

/// Claim eligibility right for a one-time airdrop (Merkle, signature, or staking-based).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ClaimAirdropAction {
    /// Source of the airdrop (e.g. Optimism, Arbitrum DAO, Jupiter).
    pub source: ProtocolRef,
    /// Distributor mechanism used to deliver the claim (Merkle, signature, or staking).
    pub claim_target: ClaimTarget,
    /// Address that will receive the claimed tokens.
    #[tsify(type = "string")]
    pub recipient: Address,
    /// Required for a `MerkleDistributor` claim; supplies the inclusion proof.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub proof: Option<MerkleProof>,
    /// EIP-712 signature for signature-based claims (e.g. Optimism v2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional, type = "string")]
    pub sig: Option<Bytes>,
    /// Mandatory payment required to claim, when the distributor charges one
    /// (e.g. LayerZero's Proof-of-Donation: `donateAndClaim` transfers a
    /// donation before delivering the claim). `None` for claims with no payment
    /// leg (Compound rewards, standalone Merkle claims, …). Statically decoded
    /// from calldata — this is the *second* value-out direction of a claim
    /// (the first being `recipient`): an inflated donation drains the signer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub donation: Option<ClaimDonation>,
    /// Live-fetched inputs (claimability, dynamic amount, token, claim window).
    pub live_inputs: ClaimAirdropLiveInputs,
}

/// The mandatory payment leg of a pay-to-claim distributor (e.g. LayerZero
/// Proof-of-Donation). All three fields are statically decoded from the claim
/// calldata — no oracle / live fetch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ClaimDonation {
    /// Amount paid to claim (raw token units). For a native-currency donation
    /// this equals `msg.value`; for an ERC20 donation it is the token amount.
    /// A malicious/buggy frontend can inflate this above the required minimum
    /// (the distributor's check is a `>=` threshold — over-payment is kept and
    /// not refunded), so it is the gated quantity.
    #[tsify(type = "string")]
    pub amount: U256,
    /// Token the donation is paid in, resolved from the distributor's currency
    /// selector (e.g. LayerZero `currency` enum → USDC / USDT / native).
    pub token: TokenRef,
    /// The claimed-asset amount the donation is charged against (e.g. the ZRO
    /// amount), so a policy can relate the payment to what is being claimed
    /// without an oracle. Proof-bound (not attacker-malleable).
    #[tsify(type = "string")]
    pub claim_amount: U256,
}

/// Distributor variant identifying how the airdrop is claimed on-chain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClaimTarget {
    /// Merkle-tree distributor; the user supplies an inclusion `proof` and leaf `index`.
    MerkleDistributor {
        /// Chain hosting the distributor contract.
        chain: ChainId,
        /// Distributor contract address.
        #[tsify(type = "string")]
        contract: Address,
        /// Leaf index in the Merkle tree corresponding to the recipient.
        index: u64,
    },
    /// Signature-based distributor (e.g. Optimism v2) that authorizes claims via EIP-712 signatures.
    SignatureDistributor {
        /// Chain hosting the distributor contract.
        chain: ChainId,
        /// Distributor contract address.
        #[tsify(type = "string")]
        contract: Address,
    },
    /// Staking-reward claim from protocols such as Lido, Pendle, or Convex.
    StakingClaim {
        /// Chain hosting the staking contract.
        chain: ChainId,
        /// Staking/rewards contract address.
        #[tsify(type = "string")]
        contract: Address,
    },
}

/// Live-fetched inputs for a `ClaimAirdropAction` — checks claimability and resolves dynamic fields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct ClaimAirdropLiveInputs {
    /// Whether the claim is still available (not expired and not already claimed).
    pub is_still_claimable: LiveField<bool>,
    /// Actual claimable amount; some airdrops are dynamic (e.g. linear vesting).
    #[tsify(type = "LiveField<string>")]
    pub actual_amount: LiveField<U256>,
    /// Token to be received; some distributions resolve the token dynamically.
    pub claim_token: LiveField<TokenRef>,
    /// Optional `(start, end)` window during which the claim is valid.
    pub claim_window: LiveField<Option<(Time, Time)>>,
}
