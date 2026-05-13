//! Signature action enrichment.

use crate::core::{LegacyAction, Token, UsdValuation};
use crate::host::{HostCapabilities, Oracle};
use crate::lowering::decimal::{try_add_decimal_strings, try_multiply_decimal_strings};

/// Enrich signature actions with oracle-derived USD valuations.
pub fn enrich_signature_action(action: &mut LegacyAction, host: &HostCapabilities<'_>) {
    match action {
        LegacyAction::Permit2(permit) => {
            permit.total_approved_usd = total_usd(
                permit
                    .approvals
                    .iter()
                    .map(|approval| (&approval.token, approval.amount.as_str())),
                host.oracle(),
            );
        }
        LegacyAction::Eip2612(permit) => {
            permit.total_approved_usd =
                total_usd([(&permit.token, permit.value.as_str())], host.oracle());
        }
        LegacyAction::Dex(_) | LegacyAction::Other(_) | LegacyAction::Eip712Other(_) => {}
    }
}

fn total_usd<'a, I>(amounts: I, oracle: &dyn Oracle) -> Option<UsdValuation>
where
    I: IntoIterator<Item = (&'a Token, &'a str)>,
{
    let mut total = None;

    for (token, raw_amount) in amounts {
        let Some(unit_price) = oracle.price(token).ok() else {
            continue;
        };
        let Some(value) =
            try_multiply_decimal_strings(raw_amount, token.decimals, &unit_price.value)
        else {
            continue;
        };
        let valuation = UsdValuation {
            value,
            as_of_ts: unit_price.as_of_ts,
            sources: unit_price.sources,
            stale_sec: unit_price.stale_sec,
        };
        total = Some(match total.take() {
            Some(previous) => sum_valuations(previous, valuation),
            None => valuation,
        });
    }

    total
}

fn sum_valuations(mut left: UsdValuation, right: UsdValuation) -> UsdValuation {
    let Some(value) = try_add_decimal_strings(&left.value, &right.value) else {
        return left;
    };
    left.value = value;
    left.as_of_ts = left.as_of_ts.min(right.as_of_ts);
    left.stale_sec = left.stale_sec.max(right.stale_sec);
    left.sources.extend(right.sources);
    left.sources.sort();
    left.sources.dedup();
    left
}
