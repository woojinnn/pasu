//! Uniswap Trade API fetcher — UniswapX order lifecycle (`GET /v1/orders`).
//! API: <https://trade-api.gateway.uniswap.org/v1/orders> (header `x-api-key`).

use policy_state::pending::PendingStatus;

/// Map a Uniswap Trade API `orderStatus` string to our canonical
/// `PendingStatus`. The second tuple element is the verbatim venue string,
/// stored in `PendingLifecycle.raw_status`.
#[must_use]
pub fn map_uniswapx_status(raw: &str) -> (PendingStatus, Option<String>) {
    let status = match raw {
        "open" | "unverified" => PendingStatus::Active,
        "filled" => PendingStatus::Filled,
        "cancelled" => PendingStatus::Cancelled,
        "expired" => PendingStatus::Expired,
        "error" | "insufficient-funds" => PendingStatus::Failed,
        _ => PendingStatus::Unknown,
    };
    (status, Some(raw.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping_is_exhaustive_and_preserves_raw() {
        let cases = [
            ("open", PendingStatus::Active),
            ("unverified", PendingStatus::Active),
            ("filled", PendingStatus::Filled),
            ("cancelled", PendingStatus::Cancelled),
            ("expired", PendingStatus::Expired),
            ("error", PendingStatus::Failed),
            ("insufficient-funds", PendingStatus::Failed),
            ("something-new", PendingStatus::Unknown),
        ];
        for (raw, want) in cases {
            let (got, kept) = map_uniswapx_status(raw);
            assert_eq!(got, want, "status for {raw}");
            assert_eq!(kept.as_deref(), Some(raw), "raw preserved for {raw}");
        }
    }
}
