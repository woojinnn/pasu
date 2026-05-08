//! Serde-friendly DTOs for the WASM JSON boundary.

use policy_engine::core::{OracleRequirementKind, Token};
use policy_engine::lowering::{HostFactPlan, WindowKey, WindowKeyPlan};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct Envelope<T: Serialize> {
    pub ok: bool,
    pub data: Option<T>,
    pub error: Option<EngineErrorDto>,
}

impl<T: Serialize> Envelope<T> {
    pub fn ok(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(EngineErrorDto::new(kind, message)),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("envelope serialization cannot fail")
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineErrorDto {
    pub kind: String,
    pub message: String,
}

impl EngineErrorDto {
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct InstallPoliciesInputDto {
    #[serde(default)]
    pub schema_text: String,
    pub policy_set: Vec<PolicyEntryDto>,
}

#[derive(Debug, Deserialize)]
pub struct PolicyEntryDto {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VerdictDto {
    Pass,
    Warn { matched: Vec<MatchedPolicyDto> },
    Fail { matched: Vec<MatchedPolicyDto> },
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchedPolicyDto {
    pub policy_id: String,
    pub reason: Option<String>,
    pub severity: String,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HostFactPlanDto {
    pub tokens_for_oracle: Vec<TokenDto>,
    pub balances: Vec<BalanceFactDto>,
    pub allowances: Vec<AllowanceFactDto>,
    pub clock_required: bool,
    pub sig_oracle_requirements: Vec<OracleRequirementDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WindowKeyPlanDto {
    pub keys: Vec<WindowKeyDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WindowKeyDto {
    pub actor: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenDto {
    pub chain_id: u64,
    pub address: String,
    pub symbol: String,
    pub decimals: u32,
    pub is_native: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BalanceFactDto {
    pub owner: String,
    pub token: TokenDto,
}

#[derive(Debug, Clone, Serialize)]
pub struct AllowanceFactDto {
    pub owner: String,
    pub token: TokenDto,
    pub spender: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OracleRequirementDto {
    pub kind: String,
    pub token: TokenDto,
    pub raw_amount: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HostSnapshotDto {
    #[serde(default)]
    pub oracle: Vec<OracleEntryDto>,
    #[serde(default)]
    pub balances: Vec<BalanceEntryDto>,
    #[serde(default)]
    pub allowances: Vec<AllowanceEntryDto>,
    #[serde(default)]
    pub now_ts: Option<u64>,
    #[serde(default)]
    pub windows: Vec<WindowEntryDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OracleEntryDto {
    pub token_key: String,
    pub usd_per_unit: String,
    pub as_of_ts: u64,
    #[serde(default)]
    pub stale_sec: u64,
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BalanceEntryDto {
    pub owner: String,
    pub token_key: String,
    pub balance: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AllowanceEntryDto {
    pub owner: String,
    pub token_key: String,
    pub spender: String,
    pub allowance: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WindowEntryDto {
    pub actor: String,
    pub name: String,
    pub value: String,
}

impl From<&Token> for TokenDto {
    fn from(token: &Token) -> Self {
        Self {
            chain_id: token.chain_id,
            address: token.address.as_str().to_string(),
            symbol: token.symbol.clone(),
            decimals: token.decimals,
            is_native: token.is_native,
        }
    }
}

impl From<HostFactPlan> for HostFactPlanDto {
    fn from(plan: HostFactPlan) -> Self {
        let HostFactPlan {
            tokens_for_oracle,
            balances,
            allowances,
            clock_required,
            sig_oracle_requirements,
        } = plan;

        Self {
            tokens_for_oracle: tokens_for_oracle.iter().map(TokenDto::from).collect(),
            balances: balances
                .into_iter()
                .map(|(owner, token)| BalanceFactDto {
                    owner: owner.as_str().to_string(),
                    token: TokenDto::from(&token),
                })
                .collect(),
            allowances: allowances
                .into_iter()
                .map(|(owner, token, spender)| AllowanceFactDto {
                    owner: owner.as_str().to_string(),
                    token: TokenDto::from(&token),
                    spender: spender.as_str().to_string(),
                })
                .collect(),
            clock_required,
            sig_oracle_requirements: sig_oracle_requirements
                .iter()
                .map(|requirement| OracleRequirementDto {
                    kind: match requirement.kind {
                        OracleRequirementKind::Input => "input",
                        OracleRequirementKind::MinOutput => "minOutput",
                    }
                    .to_string(),
                    token: TokenDto::from(&requirement.token),
                    raw_amount: requirement.raw_amount.clone(),
                })
                .collect(),
        }
    }
}

impl From<WindowKeyPlan> for WindowKeyPlanDto {
    fn from(plan: WindowKeyPlan) -> Self {
        Self {
            keys: plan.keys.iter().map(WindowKeyDto::from).collect(),
        }
    }
}

impl From<&WindowKey> for WindowKeyDto {
    fn from(key: &WindowKey) -> Self {
        Self {
            actor: key.actor.as_str().to_string(),
            name: key.key.as_str().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn envelope_ok_uses_boolean_wire_shape() {
        let output = Envelope::ok(json!({"answer": 42})).to_json();
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["ok"], true, "{parsed}");
        assert_eq!(parsed["data"]["answer"], 42, "{parsed}");
        assert!(parsed["error"].is_null(), "{parsed}");
    }
}
