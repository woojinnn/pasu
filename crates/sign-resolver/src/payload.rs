use serde_json::Value;

/// Decoded payload for each sign method variant.
#[derive(Debug, Clone)]
pub enum SignPayload {
    /// `eth_signTypedData_v4` — full EIP-712 typed data object.
    /// Contains `domain`, `types`, `primaryType`, `message`.
    TypedData(Value),

    /// `personal_sign` — raw message as a hex string.
    /// The wallet prepends "\x19Ethereum Signed Message:\n{len}" before signing.
    RawMessage(String),

    /// `eth_sign` — raw 32-byte hash as a hex string (no prefix added).
    RawHash(String),

    /// `eth_signTransaction` — full unsigned transaction object.
    /// The `data` field (calldata) is raw hex; callers that need ABI decoding
    /// should forward it to `abi-resolver`.
    Transaction(Value),

    /// `eth_sendUserOperation` — ERC-4337 UserOperation.
    /// `callData` inside `user_op` is raw hex; callers should forward it to
    /// `abi-resolver` for decoding.
    UserOperation {
        user_op: Value,
        /// EntryPoint contract address (second param). Empty string when absent.
        entry_point: String,
    },

    /// `wallet_grantPermissions` — ERC-7715 permission request object.
    PermissionRequest(Value),
}
