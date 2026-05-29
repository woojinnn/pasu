//! Builder for [`PolicyEngine`].
//!
//! Accepts text Cedar policy sources via `add_text` and optional schema
//! fragments via `add_schema_text`. Always injects the baseline permit
//! policy so absence of any matched `forbid` evaluates to allow, and
//! always pre-loads the bundled Cedar schema so every produced engine is
//! strict-validated.

use super::engine::PolicyEngine;
use super::error::PolicyError;
use crate::schema::PolicySchemaComposer;

/// Builder for `PolicyEngine`.
#[derive(Debug)]
pub struct PolicyEngineBuilder {
    text_sources: Vec<String>,
    schema_sources: Vec<String>,
}

impl Default for PolicyEngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyEngineBuilder {
    /// Construct a builder pre-loaded with the bundled Cedar schema (core +
    /// every action under `schema/policy-schema/actions/`). The bundled schema is
    /// mandatory: every engine produced by this builder is strict-validated.
    ///
    /// To extend the schema with adapter-contributed fragments, chain
    /// `add_schema_text(...)` after `new()`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            text_sources: Vec::new(),
            schema_sources: vec![PolicySchemaComposer::new().compose()],
        }
    }

    /// Construct a builder with an already-composed schema text.
    #[must_use]
    pub fn with_schema_text<S: Into<String>>(schema_text: S) -> Self {
        Self {
            text_sources: Vec::new(),
            schema_sources: vec![schema_text.into()],
        }
    }

    /// Append one or more text Cedar policies. Multiple `forbid`/`permit`
    /// clauses in the string are fine — they'll all be parsed.
    #[must_use]
    pub fn add_text<S: Into<String>>(mut self, src: S) -> Self {
        self.text_sources.push(src.into());
        self
    }

    /// Extend the schema with an additional Cedar fragment. The bundled
    /// schema is already pre-loaded by `new()`, so this method is for
    /// adapter- or host-contributed fragments only. Re-declaring an entity,
    /// type, or action that the bundled schema already provides is a parse
    /// error (`Wallet declared twice`); pass only fragments that introduce
    /// new declarations, ideally inside their own `namespace` block.
    #[must_use]
    pub fn add_schema_text<S: Into<String>>(mut self, src: S) -> Self {
        self.schema_sources.push(src.into());
        self
    }

    /// Finish the builder, producing a ready-to-evaluate `PolicyEngine`.
    ///
    /// # Errors
    ///
    /// Returns an error when schema parsing, policy parsing, or policy
    /// validation fails.
    pub fn build(self) -> Result<PolicyEngine, PolicyError> {
        // schema_sources is non-empty by construction (`new()` pre-loads the
        // bundled schema and there is no public path that empties it).
        let mut combined_schema = String::new();
        for src in &self.schema_sources {
            combined_schema.push_str(src);
            combined_schema.push('\n');
        }
        let (schema, _warnings) = cedar_policy::Schema::from_cedarschema_str(&combined_schema)
            .map_err(|e| PolicyError::Schema(e.to_string()))?;

        let mut combined = String::new();
        combined.push_str(super::engine::BASELINE_PERMIT);
        for src in &self.text_sources {
            combined.push_str(src);
            combined.push('\n');
        }
        PolicyEngine::build_from(&combined, schema)
    }
}
