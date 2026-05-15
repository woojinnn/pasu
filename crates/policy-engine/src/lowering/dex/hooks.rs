//! Shared cedar shape for `HookPermissions` records.

use crate::action::dex::HookPermissions;
use crate::context_keys::{
    HOOK_AFTER_ADD_LIQUIDITY, HOOK_AFTER_ADD_LIQUIDITY_RETURN_DELTA, HOOK_AFTER_DONATE,
    HOOK_AFTER_INITIALIZE, HOOK_AFTER_REMOVE_LIQUIDITY, HOOK_AFTER_REMOVE_LIQUIDITY_RETURN_DELTA,
    HOOK_AFTER_SWAP, HOOK_AFTER_SWAP_RETURN_DELTA, HOOK_BEFORE_ADD_LIQUIDITY, HOOK_BEFORE_DONATE,
    HOOK_BEFORE_INITIALIZE, HOOK_BEFORE_REMOVE_LIQUIDITY, HOOK_BEFORE_SWAP,
    HOOK_BEFORE_SWAP_RETURN_DELTA,
};
use serde_json::{Map, Value};

pub(crate) fn hook_permissions_json(permissions: &HookPermissions) -> Value {
    let mut out = Map::new();
    out.insert(
        HOOK_BEFORE_INITIALIZE.into(),
        Value::Bool(permissions.before_initialize),
    );
    out.insert(
        HOOK_AFTER_INITIALIZE.into(),
        Value::Bool(permissions.after_initialize),
    );
    out.insert(
        HOOK_BEFORE_ADD_LIQUIDITY.into(),
        Value::Bool(permissions.before_add_liquidity),
    );
    out.insert(
        HOOK_AFTER_ADD_LIQUIDITY.into(),
        Value::Bool(permissions.after_add_liquidity),
    );
    out.insert(
        HOOK_BEFORE_REMOVE_LIQUIDITY.into(),
        Value::Bool(permissions.before_remove_liquidity),
    );
    out.insert(
        HOOK_AFTER_REMOVE_LIQUIDITY.into(),
        Value::Bool(permissions.after_remove_liquidity),
    );
    out.insert(
        HOOK_BEFORE_SWAP.into(),
        Value::Bool(permissions.before_swap),
    );
    out.insert(HOOK_AFTER_SWAP.into(), Value::Bool(permissions.after_swap));
    out.insert(
        HOOK_BEFORE_DONATE.into(),
        Value::Bool(permissions.before_donate),
    );
    out.insert(
        HOOK_AFTER_DONATE.into(),
        Value::Bool(permissions.after_donate),
    );
    out.insert(
        HOOK_BEFORE_SWAP_RETURN_DELTA.into(),
        Value::Bool(permissions.before_swap_return_delta),
    );
    out.insert(
        HOOK_AFTER_SWAP_RETURN_DELTA.into(),
        Value::Bool(permissions.after_swap_return_delta),
    );
    out.insert(
        HOOK_AFTER_ADD_LIQUIDITY_RETURN_DELTA.into(),
        Value::Bool(permissions.after_add_liquidity_return_delta),
    );
    out.insert(
        HOOK_AFTER_REMOVE_LIQUIDITY_RETURN_DELTA.into(),
        Value::Bool(permissions.after_remove_liquidity_return_delta),
    );
    Value::Object(out)
}
