//! Small selector resolver for policy-rpc manifests.

use super::PolicyRpcError;
use serde_json::Value;

/// Resolve a v0 selector against known roots.
///
/// Supported syntax is dot field access from one of `$.root`, `$.action`,
/// `$.context`, `$.result`, or `$.params`.
///
/// # Errors
///
/// Returns an error for unsupported selector syntax, unknown roots, or missing
/// fields.
pub fn resolve_selector(
    selector: &str,
    root: &Value,
    action: &Value,
    context: &Value,
    result: &Value,
    params: &Value,
) -> Result<Value, PolicyRpcError> {
    if !selector.starts_with("$.") {
        return Err(PolicyRpcError::Selector(format!(
            "selector must start with `$.`: {selector}"
        )));
    }
    if selector
        .chars()
        .any(|ch| matches!(ch, '[' | ']' | '*' | '(' | ')'))
    {
        return Err(PolicyRpcError::Selector(format!(
            "selector uses unsupported syntax: {selector}"
        )));
    }

    let mut parts = selector[2..].split('.');
    let root_name = parts
        .next()
        .ok_or_else(|| PolicyRpcError::Selector("selector is empty".to_owned()))?;
    let mut current = match root_name {
        "root" => root,
        "action" => action,
        "context" => context,
        "result" => result,
        "params" => params,
        other => {
            return Err(PolicyRpcError::Selector(format!(
                "unknown selector root `{other}`"
            )));
        }
    };

    for field in parts {
        if field.is_empty() {
            return Err(PolicyRpcError::Selector(format!(
                "empty selector segment in {selector}"
            )));
        }
        current = current.get(field).ok_or_else(|| {
            PolicyRpcError::Selector(format!("selector field `{field}` not found in {selector}"))
        })?;
    }

    Ok(current.clone())
}
