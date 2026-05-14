//! Tri-state policy evaluation outcome and supporting metadata.

/// Final, host-facing verdict. Tri-state: `Pass` carries no data, `Warn` and
/// `Fail` carry the list of matched policies that drove the verdict.
///
/// Variant semantics map directly onto the spec's deny-overrides + warn-union
/// rule:
/// - any matched `forbid` with `@severity("deny")` → `Fail(...)`
/// - otherwise, any matched `forbid` with `@severity("warn")` → `Warn(...)`
/// - otherwise → `Pass`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// No matching deny or warning policies fired.
    Pass,
    /// One or more warning policies fired and no deny policy fired.
    Warn(Vec<MatchedPolicy>),
    /// One or more deny policies fired.
    Fail(Vec<MatchedPolicy>),
}

impl Verdict {
    /// True iff the host wallet must refuse to sign.
    #[must_use]
    pub const fn is_failure(&self) -> bool {
        matches!(self, Self::Fail(_))
    }

    /// True iff there's at least one warning the host should display.
    #[must_use]
    pub fn has_warnings(&self) -> bool {
        self.matched().iter().any(|m| m.severity == Severity::Warn)
    }

    /// Returns the matched policies for the warn / fail variants; empty for
    /// `Pass`. Useful when the caller wants to render diagnostics regardless
    /// of variant.
    #[must_use]
    pub fn matched(&self) -> &[MatchedPolicy] {
        match self {
            Self::Pass => &[],
            Self::Warn(v) | Self::Fail(v) => v,
        }
    }

    /// Combine per-request verdicts with deny-overrides semantics.
    #[must_use]
    pub fn aggregate<I>(verdicts: I) -> Self
    where
        I: IntoIterator<Item = Self>,
    {
        let mut matched = Vec::new();
        let mut any_deny = false;
        let mut any_warn = false;

        for verdict in verdicts {
            match verdict {
                Self::Pass => {}
                Self::Warn(mut v) => {
                    any_warn = true;
                    matched.append(&mut v);
                }
                Self::Fail(mut v) => {
                    any_deny = true;
                    any_warn |= v.iter().any(|m| m.severity == Severity::Warn);
                    matched.append(&mut v);
                }
            }
        }

        if any_deny {
            Self::Fail(matched)
        } else if any_warn {
            Self::Warn(matched)
        } else {
            Self::Pass
        }
    }
}

/// `@severity("deny" | "warn")` annotation parsed from a policy's source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// Blocking severity.
    Deny,
    /// Non-blocking warning severity.
    Warn,
}

/// Origin of the Cedar `PolicyRequest` that caused a policy match.
///
/// Describes which lowering layer produced the returned match (currently
/// always `Action` for the envelope-driven pipeline).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PolicyRequestOrigin {
    /// Request originated from an action-level policy request.
    Action,
    /// Request originated from a transaction-level policy request.
    Tx,
}

/// Policy metadata returned with `Warn` and `Fail` verdicts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedPolicy {
    /// Cedar policy id.
    pub policy_id: String,
    /// Optional `@reason(...)` annotation.
    pub reason: Option<String>,
    /// Parsed severity annotation.
    pub severity: Severity,
    /// Originating Cedar policy request layer.
    pub origin: PolicyRequestOrigin,
}
