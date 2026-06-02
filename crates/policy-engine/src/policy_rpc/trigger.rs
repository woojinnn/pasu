//! Trigger DSL evaluator.
//!
//! Decides whether a v2 manifest's [`Trigger`] matches a decoded action,
//! *before* any policy-rpc call. The evaluator is pure and cheap: it reads at
//! most the six top-level fields exposed by [`ActionView`] plus transaction
//! metadata ([`TxView`]), and returns a single `bool`.
//!
//! Semantics (see [`super::manifest_v2`]):
//! - empty [`Trigger::where_`] → `true` (always matches);
//! - all `(field, constraint)` pairs must hold (implicit AND);
//! - string comparison only;
//! - when a field is absent (`None`), `eq`/`in` are `false` and `ne`/`nin` are
//!   `true` (the field is "not that value").
//!
//! Multicall fan-out (which [`ActionView`] to feed for `scope: inner` vs
//! `outer`) is the caller's responsibility; this function judges one view.

use policy_transition::action::ActionView;

use super::manifest_v2::{Trigger, TriggerConstraint, TriggerField};

/// Transaction-level fields a trigger may match on. Borrow-only, built by the
/// caller from the transaction envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxView<'a> {
    /// CAIP-2 chain id, e.g. `eip155:1`.
    pub chain_id: &'a str,
    /// Submitter address.
    pub from: &'a str,
    /// Target address.
    pub to: &'a str,
}

/// Evaluate `trigger` against one action view and the transaction metadata.
///
/// Returns `true` iff every constraint in [`Trigger::where_`] holds. An empty
/// `where_` returns `true`.
#[must_use]
pub fn evaluate(trigger: &Trigger, action: &ActionView<'_>, tx: &TxView<'_>) -> bool {
    trigger
        .where_
        .iter()
        .all(|(field, constraint)| matches_field(*field, constraint, action, tx))
}

fn matches_field(
    field: TriggerField,
    constraint: &TriggerConstraint,
    action: &ActionView<'_>,
    tx: &TxView<'_>,
) -> bool {
    let lhs: Option<&str> = match field {
        TriggerField::ActionDomain => Some(action.domain),
        TriggerField::ActionTag => action.action_tag,
        TriggerField::ActionVenue => action.venue_name,
        TriggerField::TxChainId => Some(tx.chain_id),
        TriggerField::TxFrom => Some(tx.from),
        TriggerField::TxTo => Some(tx.to),
    };
    // Absent field (`None`): positive constraints (`eq`/`in`) miss; negative
    // constraints (`ne`/`nin`) hold (the field is "not that value").
    lhs.map_or(
        matches!(
            constraint,
            TriggerConstraint::Ne(_) | TriggerConstraint::Nin(_)
        ),
        |have| match constraint {
            TriggerConstraint::Eq(want) => have == want,
            TriggerConstraint::Ne(want) => have != want,
            TriggerConstraint::In(set) => set.iter().any(|v| v == have),
            TriggerConstraint::Nin(set) => !set.iter().any(|v| v == have),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy_rpc::manifest_v2::Trigger;
    use std::collections::BTreeMap;

    fn tx() -> TxView<'static> {
        TxView {
            chain_id: "eip155:1",
            from: "0xfrom",
            to: "0xto",
        }
    }

    fn swap_v3() -> ActionView<'static> {
        ActionView {
            domain: "amm",
            action_tag: Some("swap"),
            venue_name: Some("uniswap_v3"),
        }
    }

    fn token_approve() -> ActionView<'static> {
        ActionView {
            domain: "token",
            action_tag: Some("erc20_approve"),
            venue_name: None,
        }
    }

    fn trigger_of(pairs: &[(TriggerField, TriggerConstraint)]) -> Trigger {
        let mut where_ = BTreeMap::new();
        for (field, constraint) in pairs {
            where_.insert(*field, constraint.clone());
        }
        Trigger {
            scope: super::super::manifest_v2::TriggerScope::Inner,
            where_,
        }
    }

    #[test]
    fn empty_trigger_matches() {
        assert!(evaluate(&Trigger::default(), &swap_v3(), &tx()));
    }

    #[test]
    fn eq_match_and_miss() {
        let t = trigger_of(&[(
            TriggerField::ActionTag,
            TriggerConstraint::Eq("swap".into()),
        )]);
        assert!(evaluate(&t, &swap_v3(), &tx()));
        let t = trigger_of(&[(
            TriggerField::ActionTag,
            TriggerConstraint::Eq("add_liquidity".into()),
        )]);
        assert!(!evaluate(&t, &swap_v3(), &tx()));
    }

    #[test]
    fn in_match() {
        let t = trigger_of(&[(
            TriggerField::ActionVenue,
            TriggerConstraint::In(vec!["uniswap_v3".into(), "uniswap_v4".into()]),
        )]);
        assert!(evaluate(&t, &swap_v3(), &tx()));
    }

    #[test]
    fn nin_match() {
        let t = trigger_of(&[(
            TriggerField::ActionDomain,
            TriggerConstraint::Nin(vec!["lending".into()]),
        )]);
        assert!(evaluate(&t, &swap_v3(), &tx()));
    }

    #[test]
    fn venue_absent_eq_false_ne_true() {
        let eq = trigger_of(&[(
            TriggerField::ActionVenue,
            TriggerConstraint::Eq("uniswap_v3".into()),
        )]);
        assert!(!evaluate(&eq, &token_approve(), &tx()));
        let ne = trigger_of(&[(
            TriggerField::ActionVenue,
            TriggerConstraint::Ne("uniswap_v3".into()),
        )]);
        assert!(evaluate(&ne, &token_approve(), &tx()));
        // `in` absent → false; `nin` absent → true.
        let in_ = trigger_of(&[(
            TriggerField::ActionVenue,
            TriggerConstraint::In(vec!["uniswap_v3".into()]),
        )]);
        assert!(!evaluate(&in_, &token_approve(), &tx()));
        let nin = trigger_of(&[(
            TriggerField::ActionVenue,
            TriggerConstraint::Nin(vec!["uniswap_v3".into()]),
        )]);
        assert!(evaluate(&nin, &token_approve(), &tx()));
    }

    #[test]
    fn implicit_and_all_must_pass() {
        // domain==amm AND venue==uniswap_v2 on a v3 swap → false (venue misses).
        let t = trigger_of(&[
            (
                TriggerField::ActionDomain,
                TriggerConstraint::Eq("amm".into()),
            ),
            (
                TriggerField::ActionVenue,
                TriggerConstraint::Eq("uniswap_v2".into()),
            ),
        ]);
        assert!(!evaluate(&t, &swap_v3(), &tx()));
        // domain==amm AND venue==uniswap_v3 → true.
        let t = trigger_of(&[
            (
                TriggerField::ActionDomain,
                TriggerConstraint::Eq("amm".into()),
            ),
            (
                TriggerField::ActionVenue,
                TriggerConstraint::Eq("uniswap_v3".into()),
            ),
        ]);
        assert!(evaluate(&t, &swap_v3(), &tx()));
    }

    #[test]
    fn tx_fields_match() {
        let t = trigger_of(&[(
            TriggerField::TxChainId,
            TriggerConstraint::Eq("eip155:1".into()),
        )]);
        assert!(evaluate(&t, &swap_v3(), &tx()));
        let t = trigger_of(&[(
            TriggerField::TxFrom,
            TriggerConstraint::Eq("0xother".into()),
        )]);
        assert!(!evaluate(&t, &swap_v3(), &tx()));
    }
}
