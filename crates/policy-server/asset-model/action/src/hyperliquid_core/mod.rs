//! Hyperliquid CORE actions — the thin, off-chain L1 action model.
//! Unlike [`PerpAction`](crate::perp::PerpAction), which carries
//! venue-live inputs (mark price, order book, account state) that an order
//! payload does NOT contain, a Hyperliquid `/exchange` request is a small,
//! self-describing JSON intent signed by an agent key. This module models only
//! the order-/transfer-intrinsic fields the request actually carries, so the
//! policy engine can evaluate it WITHOUT fetching any live data from the venue.
//! The modeled surface covers high-risk CORE actions that place orders, change
//! leverage or margin, move funds, bridge to EVM, authorize agents/builders, or
//! delegate stake. Non-benign actions that are not explicitly modeled are
//! represented as `hl_unknown` so policy evaluation remains visible rather than
//! silently allowing them.
//! ## Tag naming
//! The serde `action` tags are prefixed `hl_` (`hl_order`, `hl_withdraw`, …)
//! so they are globally unique across every domain's action set — notably
//! `withdraw` is already a Lending tag, and the engine's flat action registries
//! require unique tags. Policies match on these prefixed tags.

use serde::{Deserialize, Serialize};
use tsify_next::Tsify;

use policy_state::primitives::{Address, Decimal};

/// A Hyperliquid CORE action, decoded from a `/exchange` POST body.
/// The serde `action` tag is the source of truth for the trigger tag a policy
/// matches on; [`Self::action_tag`] returns the same string and is verified
/// against serde by the `action_tag_matches_serde` test.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
#[serde(tag = "action")]
pub enum HyperliquidCoreAction {
    /// Withdraw USDC off the L1 to a destination (`{"type":"withdraw3"}`).
    #[serde(rename = "hl_withdraw")]
    Withdraw(HlWithdrawAction),
    /// Send USDC to another account (`{"type":"usdSend"}`).
    #[serde(rename = "hl_usd_send")]
    UsdSend(HlUsdSendAction),
    /// Transfer a spot token off the account (`{"type":"spotSend"}`).
    #[serde(rename = "hl_spot_send")]
    SpotSend(HlSpotSendAction),
    /// Move balance between the perp and spot wallets
    /// (`{"type":"usdClassTransfer"}`).
    #[serde(rename = "hl_usd_class_transfer")]
    UsdClassTransfer(HlUsdClassTransferAction),
    /// Send a token across DEXes / accounts (`{"type":"sendAsset"}`).
    #[serde(rename = "hl_send_asset")]
    SendAsset(HlSendAssetAction),
    /// Bridge a token to an EVM recipient with arbitrary calldata
    /// (`{"type":"sendToEvmWithData"}`). Highest-risk fund movement.
    #[serde(rename = "hl_send_to_evm_with_data")]
    SendToEvmWithData(HlSendToEvmWithDataAction),
    /// Deposit into HYPE staking (`{"type":"cDeposit"}`).
    #[serde(rename = "hl_c_deposit")]
    CDeposit(HlCDepositAction),
    /// Withdraw from HYPE staking (`{"type":"cWithdraw"}`).
    #[serde(rename = "hl_c_withdraw")]
    CWithdraw(HlCWithdrawAction),
    /// Deposit into / withdraw from a vault (`{"type":"vaultTransfer"}`).
    #[serde(rename = "hl_vault_transfer")]
    VaultTransfer(HlVaultTransferAction),
    /// Move USDC to / from a sub-account (`{"type":"subAccountTransfer"}`).
    #[serde(rename = "hl_sub_account_transfer")]
    SubAccountTransfer(HlSubAccountTransferAction),
    /// Delegate / undelegate stake to a validator (`{"type":"tokenDelegate"}`).
    #[serde(rename = "hl_token_delegate")]
    TokenDelegate(HlTokenDelegateAction),
    /// Any `/exchange` action not explicitly modeled above. Carries only the raw
    /// wire `type` string so a policy can gate or surface unrecognized actions
    /// (`HlUnknown` — policy default: warn / deny). Closes the silent-allow gap:
    /// every non-benign `/exchange` action reaches the engine rather than passing
    /// through unevaluated.
    #[serde(rename = "hl_unknown")]
    Unknown(HlUnknownAction),
}

impl HyperliquidCoreAction {
    /// The serde `action` tag — the trigger tag a policy matches on.
    #[must_use]
    pub const fn action_tag(&self) -> &'static str {
        match self {
            Self::Withdraw(_) => "hl_withdraw",
            Self::UsdSend(_) => "hl_usd_send",
            Self::SpotSend(_) => "hl_spot_send",
            Self::UsdClassTransfer(_) => "hl_usd_class_transfer",
            Self::SendAsset(_) => "hl_send_asset",
            Self::SendToEvmWithData(_) => "hl_send_to_evm_with_data",
            Self::CDeposit(_) => "hl_c_deposit",
            Self::CWithdraw(_) => "hl_c_withdraw",
            Self::VaultTransfer(_) => "hl_vault_transfer",
            Self::SubAccountTransfer(_) => "hl_sub_account_transfer",
            Self::TokenDelegate(_) => "hl_token_delegate",
            Self::Unknown(_) => "hl_unknown",
        }
    }

    /// Every Hyperliquid CORE action is on the `"hyperliquid"` venue, so policies
    /// can scope on `context.venue.name == "hyperliquid"`.
    #[must_use]
    pub const fn venue_name(&self) -> Option<&'static str> {
        Some("hyperliquid")
    }
}

/// USDC withdrawal off the L1: `{"type":"withdraw3"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlWithdrawAction {
    /// Destination address funds are withdrawn to (`destination`).
    #[tsify(type = "string")]
    pub destination: Address,
    /// USDC amount (`amount`), a decimal value held as a string.
    pub amount: Decimal,
}

/// USDC transfer to another account: `{"type":"usdSend"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlUsdSendAction {
    /// Recipient address (`destination`).
    #[tsify(type = "string")]
    pub destination: Address,
    /// USDC amount (`amount`), a decimal value held as a string.
    pub amount: Decimal,
}

/// Spot token transfer off the account: `{"type":"spotSend"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlSpotSendAction {
    /// Recipient address (`destination`).
    #[tsify(type = "string")]
    pub destination: Address,
    /// Token identifier (`token`), e.g. `"USDC:0x..."`.
    pub token: String,
    /// Amount (`amount`), a decimal value held as a string.
    pub amount: Decimal,
}

/// Move balance between the perp and spot wallets: `{"type":"usdClassTransfer"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlUsdClassTransferAction {
    /// USDC amount (`amount`), a decimal value held as a string.
    pub amount: Decimal,
    /// `toPerp` — `true` ⇒ spot → perp, `false` ⇒ perp → spot.
    pub to_perp: bool,
}

/// Send a token across DEXes / accounts: `{"type":"sendAsset"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlSendAssetAction {
    /// Recipient address (`destination`).
    #[tsify(type = "string")]
    pub destination: Address,
    /// Source DEX name (`sourceDex`).
    pub source_dex: String,
    /// Destination DEX name (`destinationDex`).
    pub destination_dex: String,
    /// Token identifier (`token`).
    pub token: String,
    /// Amount (`amount`), a decimal value held as a string.
    pub amount: Decimal,
}

/// Bridge a token to an EVM recipient with arbitrary calldata.
///
/// `{"type":"sendToEvmWithData"}` — the highest-risk fund movement: funds leave
/// `HyperCore` for an arbitrary EVM address with attacker-controllable `data`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlSendToEvmWithDataAction {
    /// Token identifier (`token`).
    pub token: String,
    /// Amount (`amount`), a decimal value held as a string.
    pub amount: Decimal,
    /// Source DEX name (`sourceDex`).
    pub source_dex: String,
    /// EVM recipient address (`destinationRecipient`).
    #[tsify(type = "string")]
    pub destination_recipient: Address,
    /// Raw calldata forwarded to the recipient (`data`), 0x-hex.
    pub data: String,
}

/// Deposit into HYPE staking: `{"type":"cDeposit"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlCDepositAction {
    /// Amount in token wei (`wei`), a decimal value held as a string.
    pub wei: Decimal,
}

/// Withdraw from HYPE staking: `{"type":"cWithdraw"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlCWithdrawAction {
    /// Amount in token wei (`wei`), a decimal value held as a string.
    pub wei: Decimal,
}

/// Vault deposit / withdrawal: `{"type":"vaultTransfer"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlVaultTransferAction {
    /// Vault address (`vaultAddress`).
    #[tsify(type = "string")]
    pub vault_address: Address,
    /// `isDeposit` — `true` ⇒ deposit into vault, `false` ⇒ withdraw.
    pub is_deposit: bool,
    /// USD amount (`usd`), a decimal value held as a string.
    pub usd: Decimal,
}

/// Sub-account USDC transfer: `{"type":"subAccountTransfer"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlSubAccountTransferAction {
    /// Sub-account address (`subAccountUser`).
    #[tsify(type = "string")]
    pub sub_account_user: Address,
    /// `isDeposit` — `true` ⇒ fund the sub-account, `false` ⇒ pull from it.
    pub is_deposit: bool,
    /// USD amount (`usd`), a decimal value held as a string.
    pub usd: Decimal,
}

/// Stake delegation / undelegation: `{"type":"tokenDelegate"}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlTokenDelegateAction {
    /// Validator address (`validator`).
    #[tsify(type = "string")]
    pub validator: Address,
    /// `isUndelegate` — `true` ⇒ undelegate, `false` ⇒ delegate.
    pub is_undelegate: bool,
    /// Amount in token wei (`wei`), a decimal value held as a string.
    pub wei: Decimal,
}

/// Catch-all for an `/exchange` action not explicitly modeled.
///
/// Holds only the raw wire `type` string (`{"type":"<actionType>"}`) — no
/// per-action fields — so a policy can `forbid`/`warn` on
/// `HyperliquidCore::Action::"HlUnknown"` or scope on `context.actionType`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[tsify(into_wasm_abi, from_wasm_abi)]
pub struct HlUnknownAction {
    /// The raw `/exchange` wire `type` string (e.g. `"convertToMultiSigUser"`).
    pub action_type: String,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// `action_tag()` must equal the serde `action` discriminant for every
    /// variant — a policy trigger matches on the serde tag.
    #[test]
    fn action_tag_matches_serde() {
        let cases: Vec<HyperliquidCoreAction> = vec![
            HyperliquidCoreAction::Withdraw(HlWithdrawAction {
                destination: Address::from([0x11; 20]),
                amount: Decimal::new("100"),
            }),
            HyperliquidCoreAction::UsdSend(HlUsdSendAction {
                destination: Address::from([0x22; 20]),
                amount: Decimal::new("50"),
            }),
            HyperliquidCoreAction::SpotSend(HlSpotSendAction {
                destination: Address::from([0x44; 20]),
                token: "USDC:0xdeadbeef".to_owned(),
                amount: Decimal::new("500"),
            }),
            HyperliquidCoreAction::UsdClassTransfer(HlUsdClassTransferAction {
                amount: Decimal::new("100"),
                to_perp: true,
            }),
            HyperliquidCoreAction::SendAsset(HlSendAssetAction {
                destination: Address::from([0x55; 20]),
                source_dex: String::new(),
                destination_dex: "perp".to_owned(),
                token: "USDC".to_owned(),
                amount: Decimal::new("25"),
            }),
            HyperliquidCoreAction::SendToEvmWithData(HlSendToEvmWithDataAction {
                token: "USDC".to_owned(),
                amount: Decimal::new("1"),
                source_dex: String::new(),
                destination_recipient: Address::from([0x66; 20]),
                data: "0x".to_owned(),
            }),
            HyperliquidCoreAction::CDeposit(HlCDepositAction {
                wei: Decimal::new("1000000000"),
            }),
            HyperliquidCoreAction::CWithdraw(HlCWithdrawAction {
                wei: Decimal::new("1000000000"),
            }),
            HyperliquidCoreAction::VaultTransfer(HlVaultTransferAction {
                vault_address: Address::from([0x77; 20]),
                is_deposit: true,
                usd: Decimal::new("250"),
            }),
            HyperliquidCoreAction::SubAccountTransfer(HlSubAccountTransferAction {
                sub_account_user: Address::from([0x88; 20]),
                is_deposit: false,
                usd: Decimal::new("75"),
            }),
            HyperliquidCoreAction::TokenDelegate(HlTokenDelegateAction {
                validator: Address::from([0xaa; 20]),
                is_undelegate: false,
                wei: Decimal::new("1000000000"),
            }),
            HyperliquidCoreAction::Unknown(HlUnknownAction {
                action_type: "convertToMultiSigUser".to_owned(),
            }),
        ];
        for c in cases {
            let json = serde_json::to_value(&c).unwrap();
            let serde_tag = json.get("action").and_then(serde_json::Value::as_str);
            assert_eq!(
                serde_tag,
                Some(c.action_tag()),
                "serde `action` tag must equal action_tag()"
            );
        }
    }

}
