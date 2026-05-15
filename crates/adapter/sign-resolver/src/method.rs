/// RPC method variants that represent a sign (off-chain or deferred) request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignMethod {
    /// EIP-712 structured data signature.
    EthSignTypedDataV4,
    /// Personal message signature — prefixes "\x19Ethereum Signed Message:\n".
    PersonalSign,
    /// Legacy raw-hash signature (deprecated, no prefix).
    EthSign,
    /// Signs a transaction object without broadcasting it.
    EthSignTransaction,
    /// ERC-4337 UserOperation submission; signer = sender.
    EthSendUserOperation,
    /// ERC-7715 permission grant request.
    WalletGrantPermissions,
}

impl SignMethod {
    /// Map an RPC method string to a `SignMethod`, or `None` if it is not a
    /// recognised sign method.
    #[must_use]
    pub fn detect(method: &str) -> Option<Self> {
        match method {
            "eth_signTypedData_v4" => Some(Self::EthSignTypedDataV4),
            "personal_sign" => Some(Self::PersonalSign),
            "eth_sign" => Some(Self::EthSign),
            "eth_signTransaction" => Some(Self::EthSignTransaction),
            "eth_sendUserOperation" => Some(Self::EthSendUserOperation),
            "wallet_grantPermissions" => Some(Self::WalletGrantPermissions),
            _ => None,
        }
    }

    /// Canonical RPC method name.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::EthSignTypedDataV4 => "eth_signTypedData_v4",
            Self::PersonalSign => "personal_sign",
            Self::EthSign => "eth_sign",
            Self::EthSignTransaction => "eth_signTransaction",
            Self::EthSendUserOperation => "eth_sendUserOperation",
            Self::WalletGrantPermissions => "wallet_grantPermissions",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_all_known_methods() {
        assert_eq!(
            SignMethod::detect("eth_signTypedData_v4"),
            Some(SignMethod::EthSignTypedDataV4)
        );
        assert_eq!(
            SignMethod::detect("personal_sign"),
            Some(SignMethod::PersonalSign)
        );
        assert_eq!(SignMethod::detect("eth_sign"), Some(SignMethod::EthSign));
        assert_eq!(
            SignMethod::detect("eth_signTransaction"),
            Some(SignMethod::EthSignTransaction)
        );
        assert_eq!(
            SignMethod::detect("eth_sendUserOperation"),
            Some(SignMethod::EthSendUserOperation)
        );
        assert_eq!(
            SignMethod::detect("wallet_grantPermissions"),
            Some(SignMethod::WalletGrantPermissions)
        );
    }

    #[test]
    fn returns_none_for_write_methods() {
        assert_eq!(SignMethod::detect("eth_sendTransaction"), None);
        assert_eq!(SignMethod::detect("eth_call"), None);
        assert_eq!(SignMethod::detect("eth_getBalance"), None);
    }
}
