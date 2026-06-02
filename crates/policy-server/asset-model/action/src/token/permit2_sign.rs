use serde::{Deserialize, Deserializer, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, Time, U256};
use policy_state::token::TokenRef;
use policy_state::LiveField;

/// Deserialize a [`Time`] from EITHER a JSON number OR a JSON decimal string.
///
/// The v3 calldata decoder renders Solidity `uintN` width-aware: `uint8..=uint64`
/// become JSON numbers, but `uint256` (Permit2 `sigDeadline`) becomes a decimal
/// STRING to dodge the 2^53 JSON-number precision cliff. EIP-712 typed-data
/// messages likewise encode integers as decimal strings. A plain `Time` (newtype
/// over `u64`) rejects the string, so accept both shapes here; the value must
/// still fit `u64`.
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

/// `Uniswap` `Permit2` signed allowance — off-chain signature consumed by `Permit2`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct Permit2SignAction {
    /// Underlying token whose allowance is delegated through `Permit2`.
    pub token: TokenRef,
    /// Address authorized to spend.
    #[tsify(type = "string")]
    pub spender: Address,
    /// Allowance amount.
    #[tsify(type = "string")]
    pub amount: U256,
    /// Timestamp at which the allowance expires.
    #[serde(deserialize_with = "time_from_str_or_num")]
    pub expires_at: Time,
    /// Timestamp at which the signature itself expires.
    #[serde(deserialize_with = "time_from_str_or_num")]
    pub sig_deadline: Time,
    /// `(word, bit)` pair — `Permit2` nonce bitmap coordinates.
    #[tsify(type = "LiveField<[string, number]>")]
    pub nonce: LiveField<(U256, u8)>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use policy_state::primitives::Time;

    use super::Permit2SignAction;

    #[test]
    fn permit2_time_strings_accept_uint256_max_as_forever() {
        let action: Permit2SignAction = serde_json::from_value(json!({
            "token": {
                "key": {
                    "standard": "erc20",
                    "chain": "eip155:1",
                    "address": "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"
                }
            },
            "spender": "0x0000000000000000000000000000000000000001",
            "amount": "1",
            "expires_at": "115792089237316195423570985008687907853269984665640564039457584007913129639935",
            "sig_deadline": "115792089237316195423570985008687907853269984665640564039457584007913129639935",
            "nonce": {
                "value": [0, 0],
                "source": { "kind": "user_supplied" },
                "synced_at": 0
            }
        }))
        .expect("uint256 max Permit2 deadlines should saturate");

        assert_eq!(action.expires_at, Time::from_unix(u64::MAX));
        assert_eq!(action.sig_deadline, Time::from_unix(u64::MAX));
    }
}
