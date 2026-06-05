use serde::{Deserialize, Deserializer, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, Time, U256};
use policy_state::token::TokenRef;
use policy_state::LiveField;

fn time_from_str_or_num<'de, D>(deserializer: D) -> Result<Time, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error as _;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StrOrNum {
        Num(u64),
        Str(String),
    }

    let n = match StrOrNum::deserialize(deserializer)? {
        StrOrNum::Num(n) => n,
        StrOrNum::Str(s) => match s.parse::<u64>() {
            Ok(n) => n,
            Err(error) if matches!(error.kind(), std::num::IntErrorKind::PosOverflow) => u64::MAX,
            Err(error) => {
                return Err(D::Error::custom(format!("Time from string {s:?}: {error}")));
            }
        },
    };
    Ok(Time::from_unix(n))
}

/// `Uniswap` `Permit2` SignatureTransfer typed-data signature.
///
/// `PermitTransferFrom` signs a one-time token spend cap plus unordered nonce
/// and deadline. It does not sign the final recipient; that is supplied when a
/// spender submits the signature on chain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Permit2SignTransferAction {
    pub token: TokenRef,
    #[tsify(type = "string")]
    pub owner: Address,
    #[tsify(type = "string")]
    pub spender: Address,
    #[tsify(type = "string")]
    pub amount: U256,
    /// `(word, bit)` pair — Permit2 unordered nonce bitmap coordinates.
    #[tsify(type = "LiveField<[string, number]>")]
    pub nonce: LiveField<(U256, u8)>,
    #[serde(deserialize_with = "time_from_str_or_num")]
    pub sig_deadline: Time,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub witness_type: Option<String>,
}

/// `Uniswap` `Permit2` SignatureTransfer execution (`permitTransferFrom` /
/// `permitWitnessTransferFrom`), including owner-aware fund movement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Permit2TransferFromAction {
    pub token: TokenRef,
    #[tsify(type = "string")]
    pub owner: Address,
    #[tsify(type = "string")]
    pub spender: Address,
    #[tsify(type = "string")]
    pub recipient: Address,
    #[tsify(type = "string")]
    pub amount: U256,
    #[tsify(type = "string")]
    pub permitted_amount: U256,
    /// `(word, bit)` pair — Permit2 unordered nonce bitmap coordinates.
    #[tsify(type = "LiveField<[string, number]>")]
    pub nonce: LiveField<(U256, u8)>,
    #[serde(deserialize_with = "time_from_str_or_num")]
    pub sig_deadline: Time,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[tsify(optional)]
    pub witness_type: Option<String>,
}
