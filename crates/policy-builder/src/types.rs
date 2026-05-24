//! Core data model: schemas, rules, predicates.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Cedar primitive types the compiler understands.
///
/// Composite Cedar types (records, sets of records) are not represented here
/// directly; nested record fields are flattened into dotted field paths in
/// [`ActionSchema::fields`], and sets-of-records are intentionally unsupported
/// in v1 because Cedar has no general iterator and exposing a
/// "contains the whole token record" predicate is rarely user-meaningful.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CedarType {
    /// Cedar `Long` (64-bit signed integer).
    Long,
    /// Cedar `String`.
    String,
    /// Cedar `Bool`.
    Bool,
    /// Cedar `decimal` extension type. Literals enter as quoted strings via `decimal("…")`.
    Decimal,
    /// `Set<String>` — supports `.contains`, `.containsAny`, `.containsAll`.
    SetOfString,
    /// `Set<Long>` — supports `.contains`, `.containsAny`, `.containsAll`.
    SetOfLong,
}

/// One addressable leaf field inside an action context.
///
/// Nested record fields are flattened: e.g. `totalInputUsd.value` is a single
/// `FieldSpec` whose `parent_path` is `"totalInputUsd"` and whose
/// `parent_optional` is `true`. The generator uses `parent_path` /
/// `parent_optional` to emit `context has parent && …` guards exactly once
/// per parent, regardless of how many leaves under it are referenced.
///
/// Fields are split into two groups by `is_custom`:
/// - Base fields (`is_custom == false`) are calldata-derived and live directly
///   under `context.<path>` — e.g. `context.feeBps`.
/// - Custom fields (`is_custom == true`) are manifest-enriched and live under
///   the optional `context.custom` record — e.g.
///   `context.custom.totalInputUsd.value`. The generator emits
///   `context has custom && context.custom has <parent> && …` guards for
///   these so the resulting policy validates against the v1 schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSpec {
    /// Dotted path under `context` (base) or `context.custom` (custom). The
    /// path itself never includes the `custom.` prefix; that's a property of
    /// the field's `is_custom` flag.
    /// Example: `"totalInputUsd.value"`.
    pub path: String,
    /// Cedar type of this leaf.
    #[serde(rename = "type")]
    pub cedar_type: CedarType,
    /// Whether the leaf itself is optional (independent of parent).
    pub optional: bool,
    /// Dotted path of the parent record, if any. Triggers a `has` guard.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_path: Option<String>,
    /// Whether the parent record is optional and needs a `has` guard.
    #[serde(default)]
    pub parent_optional: bool,
    /// Human-readable label for UIs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// `true` when this field is a manifest-contributed extension that lives
    /// under the optional `context.custom` record. `false` for calldata-derived
    /// base fields. The generator/parser key off this flag to emit and accept
    /// the `context.custom.<path>` prefix.
    #[serde(default)]
    pub is_custom: bool,
    /// Closed-set string enum constraint mirrored from the action-schema JSON
    /// (`"enum": [...]`). When `Some`, only these literal values are accepted
    /// as operands for this field; `None` means the field is free-form within
    /// its `cedar_type`. Enforced by [`crate::validate::validate`] for every
    /// operator arity. Cedar emit is unaffected — bad values are rejected
    /// before reaching the generator, so the emitted policy never contains a
    /// literal outside the declared set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_values: Option<Vec<String>>,
    /// Implicit decimal-point exponent for `Long` fields whose runtime value
    /// has already been rescaled by the manifest enrichment. When `Some(n)`,
    /// the policy builder accepts user input as a decimal string and emits
    /// `value × 10^n` as the Long literal — `0.00003` with `scale = 9`
    /// becomes the Cedar literal `30000`. The runtime field is plain `Long`;
    /// the scale only affects compile-time literal rendering and back-parse
    /// pretty-printing.
    ///
    /// Used by token-native amount fields (`inputAmountNano`,
    /// `outputAmountNano`) so policies read in the same units a user sees on
    /// a DEX UI (`0.5 ETH`, `100 USDC`) regardless of the token's on-chain
    /// `decimals`. Cedar emit stays inside i64; the typical scale `9`
    /// (Gwei-style) preserves 9 fractional digits and ~9.2 × 10⁹ max
    /// magnitude — sufficient for any practical token amount.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<u8>,
    /// Regex the operand string must match, mirrored from the upstream
    /// action-schema JSON's `"pattern"` (e.g. the EVM address shape
    /// `^0x[0-9a-fA-F]{40}$` from `_common.json#/$defs/Address`). When
    /// `Some`, [`crate::validate::validate`] compiles and applies the regex
    /// to every operand of every arity; failures surface as
    /// `PatternMismatch` so a typo'd address (`"0x52"`, `"WETH"`, etc.)
    /// gets caught at compile time instead of producing a syntactically
    /// valid Cedar policy that silently never matches at runtime.
    ///
    /// Cedar emit is unaffected — only well-shaped values reach the
    /// generator. Free-form fields leave this as `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

/// Schema for one action keyword.
///
/// `principal_type` and `resource_type` map directly to the Cedar
/// `appliesTo { principal: …, resource: … }` declaration. `fields` is keyed by
/// the dotted leaf path so lookups during validation are O(log n).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSchema {
    /// Cedar action keyword, used as `Action::"<action>"` in emitted policies.
    pub action: String,
    /// Cedar entity type for the principal (e.g. `"Wallet"`).
    pub principal_type: String,
    /// Cedar entity type for the resource (e.g. `"Protocol"`).
    pub resource_type: String,
    /// All addressable leaf fields, keyed by dotted path.
    pub fields: BTreeMap<String, FieldSpec>,
}

/// Severity annotation. Matches the engine's `@severity("deny"|"warn")` contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Deny — final verdict becomes `Fail`.
    Deny,
    /// Warn — final verdict becomes `Warn` (unless a deny also fires).
    Warn,
}

impl Severity {
    /// String form used in the `@severity(...)` annotation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Deny => "deny",
            Self::Warn => "warn",
        }
    }
}

/// Literal operand(s) for a [`Predicate`].
///
/// Stored as strings so the front-end doesn't need to commit to a numeric
/// representation; the generator parses/escapes per the field's Cedar type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PredicateValue {
    /// Single operand — used by `>`, `<`, `==`, `contains`, etc.
    Single(String),
    /// Multi operand — used by `in [..]`, `containsAny`, `containsAll`.
    Multi(Vec<String>),
    /// No operand — used by `is true`, `is false`.
    None,
}

/// One comparison inside a rule's `when` clause.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Predicate {
    /// Field path (must exist in the resolved [`ActionSchema::fields`]).
    pub field: String,
    /// Operator id. Valid set depends on the field's Cedar type — see [`crate::operators`].
    pub op: String,
    /// Operand(s) — must match the operator's expected arity.
    pub value: PredicateValue,
}

/// A complete user-authored rule. Compiled to one Cedar `forbid` policy.
///
/// Predicates are AND-ed in the emitted `when` clause. OR/NOT is intentionally
/// out of scope for v1 — multiple OR-branches are expressed as multiple rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    /// User-facing id. Emitted as `@id("<id>")` and surfaces in `Verdict.matched[].policy_id`.
    pub id: String,
    /// Action this rule applies to. Must match a registered [`ActionSchema::action`].
    pub action: String,
    /// `deny` or `warn`.
    pub severity: Severity,
    /// Human-readable explanation. Shown to the user when the rule fires.
    pub reason: String,
    /// Predicates AND-ed inside `when { … }`. Empty = unconditional `forbid`.
    pub predicates: Vec<Predicate>,
}
