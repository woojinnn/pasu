use std::collections::HashMap;

use serde_json::{json, Map, Value};

pub type MapperFn = fn(&Value) -> Option<Value>;

#[derive(Default)]
pub struct MapperRegistry {
    by_id: HashMap<String, MapperFn>,
}

impl std::fmt::Debug for MapperRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapperRegistry")
            .field("known_ids", &self.by_id.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl MapperRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, id: &str, f: MapperFn) {
        self.by_id.insert(id.to_string(), f);
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&MapperFn> {
        self.by_id.get(id)
    }

    #[must_use]
    pub fn maybe_apply(&self, id: &str, value: Value) -> Value {
        if let Some(mapper) = self.by_id.get(id) {
            if let Some(mapped) = mapper(&value) {
                return mapped;
            }
        }
        value
    }

    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        r.register("aave_v3_user_account_data", map_aave_v3_user_account_data);
        r.register("aave_v3_reserve_data", map_aave_v3_reserve_data);
        r.register(
            "aave_v3_current_borrow_rate",
            map_aave_v3_current_borrow_rate,
        );
        r
    }
}

#[must_use]
pub fn map_aave_v3_current_borrow_rate(v: &Value) -> Option<Value> {
    let arr = v.as_array()?;
    if arr.len() < 5 {
        return None;
    }
    Some(ray_to_decimal_string(&arr[4]))
}

// ─────────────────────── Aave V3 ───────────────────────

/// Aave V3 `getUserAccountData` return tuple:
///
///   [0] totalCollateralBase       (USD, 8 decimals)
///   [1] totalDebtBase             (USD, 8 decimals)
///   [2] availableBorrowsBase      (USD, 8 decimals)
///   [3] currentLiquidationThreshold (bp)
///   [4] ltv                       (bp)
///   [5] healthFactor              (ray = 1e27)
///
/// `UserLendingState` shape:
///   { `total_collat_usd`: U256, `total_debt_usd`: U256, `available_borrow_usd`: U256,
///     `health_factor`: Decimal }
#[must_use]
pub fn map_aave_v3_user_account_data(v: &Value) -> Option<Value> {
    let arr = v.as_array()?;
    if arr.len() < 6 {
        return None;
    }
    Some(json!({
        "total_collat_usd":     arr[0].clone(),
        "total_debt_usd":       arr[1].clone(),
        "available_borrow_usd": arr[2].clone(),
        "health_factor":        ray_to_decimal_string(&arr[5]),
    }))
}

/// Aave V3 `getReserveData` return tuple:
///
///   [1]  liquidityIndex (uint128, ray)
///   [2]  currentLiquidityRate (uint128, ray = supply APY)
///   [3]  variableBorrowIndex (uint128, ray)
///   [4]  currentVariableBorrowRate (uint128, ray = borrow APY)
///   [5]  currentStableBorrowRate (uint128, ray)
///   [6]  lastUpdateTimestamp (uint40)
///   [7]  id (uint16)
///   [8]  aTokenAddress
///   [9]  stableDebtTokenAddress
///   [10] variableDebtTokenAddress
///   [11] interestRateStrategyAddress
///   [12] accruedToTreasury (uint128)
///   [13] unbacked (uint128)
///   [14] isolationModeTotalDebt (uint128)
///
/// `ReserveState` shape:
///   { `total_supply`: U256, `total_borrow`: U256, `utilization_bp`: u32,
///     `supply_cap`: `Option<U256>`, `borrow_cap`: `Option<U256>`,
///     `ltv_bp`: u32, `liquidation_threshold_bp`: u32, `liquidation_bonus_bp`: u32,
///     `reserve_factor_bp`: u32, `is_frozen`: bool, `is_paused`: bool }
#[must_use]
pub fn map_aave_v3_reserve_data(v: &Value) -> Option<Value> {
    let arr = v.as_array()?;
    if arr.len() < 15 {
        return None;
    }
    Some(json!({
        "total_supply":              arr[1].clone(),
        "total_borrow":              arr[3].clone(),
        "utilization_bp":            0,
        "supply_cap":                Value::Null,
        "borrow_cap":                Value::Null,
        "ltv_bp":                    0,
        "liquidation_threshold_bp":  0,
        "liquidation_bonus_bp":      0,
        "reserve_factor_bp":         0,
        "is_frozen":                 false,
        "is_paused":                 false,
    }))
}

// ─────────────────────── Helpers ───────────────────────

fn ray_to_decimal_string(v: &Value) -> Value {
    let s = match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return v.clone(),
    };
    Value::String(scale_string_by_decimals(&s, 27))
}

fn scale_string_by_decimals(value_str: &str, decimals: usize) -> String {
    let neg = value_str.starts_with('-');
    let abs = value_str.trim_start_matches('-');
    let digits: String = abs.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return "0".into();
    }

    let result = if digits.len() <= decimals {
        let pad = decimals - digits.len();
        let frac = format!("{}{}", "0".repeat(pad), digits);
        let trimmed = frac.trim_end_matches('0');
        if trimmed.is_empty() {
            "0".into()
        } else {
            format!("0.{trimmed}")
        }
    } else {
        let split = digits.len() - decimals;
        let int_part = &digits[..split];
        let frac_part = &digits[split..].trim_end_matches('0');
        if frac_part.is_empty() {
            int_part.to_string()
        } else {
            format!("{int_part}.{frac_part}")
        }
    };
    if neg {
        format!("-{result}")
    } else {
        result
    }
}

fn _bp_to_decimal_string(v: &Value) -> Value {
    let s = match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return v.clone(),
    };
    Value::String(scale_string_by_decimals(&s, 4))
}

// ─────────────────────── Helper accessor ───────────────────────

#[allow(dead_code)]
fn _ensure_map_import() -> Map<String, Value> {
    Map::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_to_decimal_basic() {
        // 1.5 in ray = 1.5e27
        let v = Value::String("1500000000000000000000000000".into());
        assert_eq!(ray_to_decimal_string(&v), Value::String("1.5".into()));
    }

    #[test]
    fn ray_zero() {
        let v = Value::String("0".into());
        assert_eq!(ray_to_decimal_string(&v), Value::String("0".into()));
    }

    #[test]
    fn aave_user_account_data_maps_to_struct_shape() {
        let arr = Value::Array(vec![
            Value::String("1000000000000".into()),                // collat
            Value::String("500000000000".into()),                 // debt
            Value::String("300000000000".into()),                 // available
            Value::String("8000".into()),                         // liq_threshold
            Value::String("7500".into()),                         // ltv
            Value::String("1500000000000000000000000000".into()), // hf = 1.5 ray
        ]);
        let mapped = map_aave_v3_user_account_data(&arr).unwrap();
        let obj = mapped.as_object().unwrap();
        assert_eq!(
            obj["total_collat_usd"],
            Value::String("1000000000000".into())
        );
        assert_eq!(obj["health_factor"], Value::String("1.5".into()));
    }

    #[test]
    fn aave_user_too_short_returns_none() {
        let arr = Value::Array(vec![Value::String("1".into())]);
        assert!(map_aave_v3_user_account_data(&arr).is_none());
    }

    #[test]
    fn registry_builtins() {
        let r = MapperRegistry::with_builtins();
        assert!(r.get("aave_v3_user_account_data").is_some());
        assert!(r.get("aave_v3_reserve_data").is_some());
        assert!(r.get("nonexistent").is_none());
    }
}
