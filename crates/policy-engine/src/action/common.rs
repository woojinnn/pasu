//! Shared action schema primitives.

use alloy_primitives::U256;
use serde::{de, Deserialize, Deserializer, Serialize};
use std::{fmt, str::FromStr};

/// EVM address normalized to lowercase `0x` plus 40 hex characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Address(String);

impl FromStr for Address {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_hex_prefixed(s, Some(40), "address")?;
        Ok(Self(s.to_ascii_lowercase()))
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Address {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Address> for String {
    fn from(value: Address) -> Self {
        value.0
    }
}

/// Hex byte string encoded as `0x` plus an even number of hex characters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Hex(String);

impl FromStr for Hex {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_hex_prefixed(s, None, "hex")?;
        let digit_len = s.len() - 2;
        if !digit_len.is_multiple_of(2) {
            return Err("hex must contain an even number of digits".to_owned());
        }
        Ok(Self(s.to_ascii_lowercase()))
    }
}

impl fmt::Display for Hex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Hex {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Hex> for String {
    fn from(value: Hex) -> Self {
        value.0
    }
}

/// Unsigned base-10 integer string bounded to the `uint256` range.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DecimalString(String);

impl FromStr for DecimalString {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err("decimal string must not be empty".to_owned());
        }
        if !s.chars().all(|c| c.is_ascii_digit()) {
            return Err("decimal string must contain only base-10 digits".to_owned());
        }
        U256::from_str_radix(s, 10)
            .map_err(|e| format!("decimal string exceeds uint256 range: {e}"))?;
        Ok(Self(s.to_owned()))
    }
}

impl fmt::Display for DecimalString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for DecimalString {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<DecimalString> for String {
    fn from(value: DecimalString) -> Self {
        value.0
    }
}

/// Asset classification used by action fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    /// Native chain asset.
    Native,
    /// ERC-20 token.
    Erc20,
    /// ERC-721 non-fungible token.
    Erc721,
    /// ERC-1155 multi-token.
    Erc1155,
    /// Unknown or unsupported asset type.
    Unknown,
}

/// Asset reference used by action fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRef {
    /// Asset classification.
    pub kind: AssetKind,
    /// Token contract address, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
    /// Token id for non-fungible or multi-token assets, when applicable.
    #[serde(rename = "tokenId", skip_serializing_if = "Option::is_none")]
    pub token_id: Option<DecimalString>,
    /// Human-readable asset symbol, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Token decimal precision, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decimals: Option<u8>,
}

impl<'de> Deserialize<'de> for AssetRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RawAssetRef {
            kind: AssetKind,
            address: Option<Address>,
            #[serde(rename = "tokenId")]
            token_id: Option<DecimalString>,
            symbol: Option<String>,
            decimals: Option<u8>,
        }

        let raw = RawAssetRef::deserialize(deserializer)?;
        let asset = Self {
            kind: raw.kind,
            address: raw.address,
            token_id: raw.token_id,
            symbol: raw.symbol,
            decimals: raw.decimals,
        };

        asset
            .validate_required_fields()
            .map_err(de::Error::custom)?;
        Ok(asset)
    }
}

impl AssetRef {
    #[allow(clippy::missing_const_for_fn)]
    fn validate_required_fields(&self) -> Result<(), &'static str> {
        if matches!(
            self.kind,
            AssetKind::Erc20 | AssetKind::Erc721 | AssetKind::Erc1155
        ) && self.address.is_none()
        {
            return Err("address is required for erc20, erc721, and erc1155 assets");
        }

        if matches!(self.kind, AssetKind::Erc721 | AssetKind::Erc1155) && self.token_id.is_none() {
            return Err("tokenId is required for erc721 and erc1155 assets");
        }

        Ok(())
    }
}

/// Asset reference paired with an amount constraint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRefWithAmountConstraint {
    /// Referenced asset.
    pub asset: AssetRef,
    /// Amount constraint for the referenced asset.
    pub amount: AmountConstraint,
}

/// Constraint semantics for an amount field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmountKind {
    /// Exact amount.
    Exact,
    /// Minimum acceptable amount.
    Min,
    /// Maximum acceptable amount.
    Max,
    /// Unlimited amount.
    Unlimited,
    /// Estimated amount.
    Estimated,
    /// Unknown amount semantics.
    Unknown,
    /// Portion-based amount.
    Portion,
}

/// Amount constraint paired with an optional raw value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmountConstraint {
    /// Constraint semantics.
    pub kind: AmountKind,
    /// Raw integer value, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<DecimalString>,
}

/// Source field used to derive an action validity window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ValiditySource {
    /// Transaction calldata deadline.
    TxDeadline,
    /// Signature deadline.
    SignatureDeadline,
    /// Delegated grant expiration.
    GrantExpiration,
}

/// Action validity window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Validity {
    /// Expiration timestamp encoded as a decimal string.
    pub expires_at: DecimalString,
    /// Source that supplied the expiration.
    pub source: ValiditySource,
}

fn validate_hex_prefixed(
    s: &str,
    expected_digits: Option<usize>,
    label: &str,
) -> Result<(), String> {
    if !s.starts_with("0x") {
        return Err(format!("{label} must start with 0x"));
    }

    let digits = &s[2..];
    if let Some(expected) = expected_digits {
        if digits.len() != expected {
            return Err(format!(
                "{label} must contain exactly {expected} hex digits"
            ));
        }
    }

    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("{label} contains non-hex characters"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        Address, AmountConstraint, AmountKind, AssetKind, AssetRef, AssetRefWithAmountConstraint,
        DecimalString, Validity, ValiditySource,
    };
    use std::str::FromStr as _;

    #[test]
    fn test_address_normalize_uppercase_to_lowercase() {
        let address = Address::from_str("0xABCDabcdABCDabcdABCDabcdABCDabcdABCDabcd").unwrap();

        assert_eq!(
            address.to_string(),
            "0xabcdabcdabcdabcdabcdabcdabcdabcdabcdabcd"
        );
    }

    #[test]
    fn test_address_reject_wrong_length() {
        assert!(Address::from_str("0x1234").is_err());
    }

    #[test]
    fn test_address_reject_non_hex() {
        assert!(Address::from_str("0xggggabcdabcdabcdabcdabcdabcdabcdabcdabcd").is_err());
    }

    #[test]
    fn test_decimal_string_reject_non_digits() {
        assert!(DecimalString::from_str("12.3").is_err());
        assert!(DecimalString::from_str("").is_err());
    }

    #[test]
    fn test_decimal_string_accept_zero_and_max_u256() {
        assert_eq!(DecimalString::from_str("0").unwrap().to_string(), "0");
        assert_eq!(
            DecimalString::from_str(
                "115792089237316195423570985008687907853269984665640564039457584007913129639935",
            )
            .unwrap()
            .to_string(),
            "115792089237316195423570985008687907853269984665640564039457584007913129639935",
        );
    }

    #[test]
    fn test_asset_ref_serde_roundtrip_erc20() {
        let asset = AssetRef {
            kind: AssetKind::Erc20,
            address: Some(Address::from_str("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap()),
            token_id: None,
            symbol: Some("USDC".to_owned()),
            decimals: Some(6),
        };

        let json = serde_json::to_string(&asset).unwrap();

        assert_eq!(
            json,
            r#"{"kind":"erc20","address":"0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48","symbol":"USDC","decimals":6}"#
        );
        assert_eq!(serde_json::from_str::<AssetRef>(&json).unwrap(), asset);
    }

    #[test]
    fn test_asset_ref_token_id_serde_roundtrip() {
        let asset = AssetRef {
            kind: AssetKind::Erc721,
            address: Some(Address::from_str("0x1234567890abcdef1234567890abcdef12345678").unwrap()),
            token_id: Some(DecimalString::from_str("42").unwrap()),
            symbol: Some("PUNK".to_owned()),
            decimals: None,
        };

        let json = serde_json::to_string(&asset).unwrap();

        assert_eq!(
            json,
            r#"{"kind":"erc721","address":"0x1234567890abcdef1234567890abcdef12345678","tokenId":"42","symbol":"PUNK"}"#
        );
        assert_eq!(serde_json::from_str::<AssetRef>(&json).unwrap(), asset);
    }

    #[test]
    fn test_asset_ref_serde_omits_optional_fields() {
        let asset = AssetRef {
            kind: AssetKind::Native,
            address: None,
            token_id: None,
            symbol: None,
            decimals: None,
        };

        let json = serde_json::to_string(&asset).unwrap();

        assert_eq!(json, r#"{"kind":"native"}"#);
    }

    #[test]
    fn test_asset_ref_deserialize_erc20_with_address_ok() {
        let json = r#"{"kind":"erc20","address":"0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48"}"#;

        let asset = serde_json::from_str::<AssetRef>(json).unwrap();

        assert_eq!(asset.kind, AssetKind::Erc20);
        assert!(asset.address.is_some());
        assert_eq!(asset.token_id, None);
    }

    #[test]
    fn test_asset_ref_deserialize_erc20_missing_address_err() {
        let json = r#"{"kind":"erc20"}"#;

        let err = serde_json::from_str::<AssetRef>(json).unwrap_err();

        assert!(err.to_string().contains("address is required"));
    }

    #[test]
    fn test_asset_ref_deserialize_erc721_with_address_and_token_id_ok() {
        let json = r#"{"kind":"erc721","address":"0x1234567890abcdef1234567890abcdef12345678","tokenId":"42"}"#;

        let asset = serde_json::from_str::<AssetRef>(json).unwrap();

        assert_eq!(asset.kind, AssetKind::Erc721);
        assert!(asset.address.is_some());
        assert_eq!(asset.token_id.unwrap().to_string(), "42");
    }

    #[test]
    fn test_asset_ref_deserialize_erc721_missing_token_id_err() {
        let json = r#"{"kind":"erc721","address":"0x1234567890abcdef1234567890abcdef12345678"}"#;

        let err = serde_json::from_str::<AssetRef>(json).unwrap_err();

        assert!(err.to_string().contains("tokenId is required"));
    }

    #[test]
    fn test_asset_ref_deserialize_native_without_address_ok() {
        let json = r#"{"kind":"native"}"#;

        let asset = serde_json::from_str::<AssetRef>(json).unwrap();

        assert_eq!(asset.kind, AssetKind::Native);
        assert_eq!(asset.address, None);
        assert_eq!(asset.token_id, None);
    }

    #[test]
    fn test_asset_ref_deserialize_unknown_without_address_ok() {
        // `unknown` is the schema's representation for an asset whose
        // identity cannot be resolved statically (e.g. an LP token whose
        // address is a CREATE2 result absent from calldata). It must
        // deserialize without an address — declarative bundles emit it for
        // such assets so the evaluate stage does not fail-closed.
        let json = r#"{"kind":"unknown"}"#;

        let asset = serde_json::from_str::<AssetRef>(json).unwrap();

        assert_eq!(asset.kind, AssetKind::Unknown);
        assert_eq!(asset.address, None);
        assert_eq!(asset.token_id, None);
    }

    #[test]
    fn test_asset_ref_with_amount_constraint_serde_roundtrip() {
        let constrained = AssetRefWithAmountConstraint {
            asset: AssetRef {
                kind: AssetKind::Erc1155,
                address: Some(
                    Address::from_str("0xabcdefabcdefabcdefabcdefabcdefabcdefabcd").unwrap(),
                ),
                token_id: Some(DecimalString::from_str("7").unwrap()),
                symbol: None,
                decimals: None,
            },
            amount: AmountConstraint {
                kind: AmountKind::Min,
                value: Some(DecimalString::from_str("1000").unwrap()),
            },
        };

        let json = serde_json::to_string(&constrained).unwrap();

        assert_eq!(
            json,
            r#"{"asset":{"kind":"erc1155","address":"0xabcdefabcdefabcdefabcdefabcdefabcdefabcd","tokenId":"7"},"amount":{"kind":"min","value":"1000"}}"#
        );
        assert_eq!(
            serde_json::from_str::<AssetRefWithAmountConstraint>(&json).unwrap(),
            constrained
        );
    }

    #[test]
    fn test_amount_constraint_serde_each_kind() {
        let cases = [
            (AmountKind::Exact, "exact"),
            (AmountKind::Min, "min"),
            (AmountKind::Max, "max"),
            (AmountKind::Unlimited, "unlimited"),
            (AmountKind::Estimated, "estimated"),
            (AmountKind::Unknown, "unknown"),
            (AmountKind::Portion, "portion"),
        ];

        for (kind, expected) in cases {
            let constraint = AmountConstraint {
                kind,
                value: Some(DecimalString::from_str("123").unwrap()),
            };

            let json = serde_json::to_string(&constraint).unwrap();
            let expected_json = format!(r#"{{"kind":"{expected}","value":"123"}}"#);

            assert_eq!(json, expected_json);
            assert_eq!(
                serde_json::from_str::<AmountConstraint>(&json).unwrap(),
                constraint
            );
        }
    }

    #[test]
    fn test_validity_serde_uses_kebab_case() {
        let validity = Validity {
            expires_at: DecimalString::from_str("1700000000").unwrap(),
            source: ValiditySource::SignatureDeadline,
        };

        let json = serde_json::to_string(&validity).unwrap();

        assert_eq!(
            json,
            r#"{"expiresAt":"1700000000","source":"signature-deadline"}"#
        );
        assert_eq!(serde_json::from_str::<Validity>(&json).unwrap(), validity);
    }
}
