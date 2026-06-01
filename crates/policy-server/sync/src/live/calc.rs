use std::collections::HashMap;

use serde_json::Value;

use policy_state::WalletState;

use crate::error::SyncError;

pub struct CalcContext<'a> {
    pub state: &'a WalletState,
    pub inputs: Vec<Value>,
}

pub type CalcFn = fn(&CalcContext<'_>) -> Result<Value, SyncError>;

#[derive(Default)]
pub struct CalcRegistry {
    by_id: HashMap<String, CalcFn>,
}

impl CalcRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        r.register("aave_hf", aave_hf);
        r.register("perp_pnl", perp_pnl);
        r.register("perp_liq_price", perp_liq_price);
        r
    }

    pub fn register(&mut self, id: &str, f: CalcFn) {
        self.by_id.insert(id.to_string(), f);
    }

    pub fn run(&self, calc_id: &str, ctx: &CalcContext<'_>) -> Result<Value, SyncError> {
        let f = self
            .by_id
            .get(calc_id)
            .copied()
            .ok_or_else(|| SyncError::UnknownCalcId(calc_id.to_string()))?;
        f(ctx)
    }

    #[must_use]
    pub fn known_ids(&self) -> Vec<&str> {
        self.by_id.keys().map(std::string::String::as_str).collect()
    }
}

/// Aave Health Factor = sum(collateral * liqThreshold) / sum(debt)
///   0: total collateral (in USD), decimal-string
///   1: total debt (in USD), decimal-string
///   2: liquidation threshold (0..1), decimal-string
fn aave_hf(ctx: &CalcContext<'_>) -> Result<Value, SyncError> {
    if ctx.inputs.len() < 3 {
        return Err(SyncError::DeriveFailed {
            calc_id: "aave_hf".into(),
            reason: format!("expected 3 inputs, got {}", ctx.inputs.len()),
        });
    }
    let collateral = parse_decimal_f64(&ctx.inputs[0], "collateral")?;
    let debt = parse_decimal_f64(&ctx.inputs[1], "debt")?;
    let liq_threshold = parse_decimal_f64(&ctx.inputs[2], "liq_threshold")?;

    let hf = if debt == 0.0 {
        1e18
    } else {
        (collateral * liq_threshold) / debt
    };
    Ok(Value::String(format_decimal(hf)))
}

/// Perp unrealized `PnL` = (mark - entry) * size * `side_sign`
/// inputs: 0=entry, 1=mark, 2=size, 3=side ("long" | "short")
fn perp_pnl(ctx: &CalcContext<'_>) -> Result<Value, SyncError> {
    if ctx.inputs.len() < 4 {
        return Err(SyncError::DeriveFailed {
            calc_id: "perp_pnl".into(),
            reason: format!("expected 4 inputs, got {}", ctx.inputs.len()),
        });
    }
    let entry = parse_decimal_f64(&ctx.inputs[0], "entry")?;
    let mark = parse_decimal_f64(&ctx.inputs[1], "mark")?;
    let size = parse_decimal_f64(&ctx.inputs[2], "size")?;
    let side = ctx.inputs[3].as_str().unwrap_or("long");
    let sign = if side == "short" { -1.0 } else { 1.0 };

    let pnl = (mark - entry) * size * sign;
    Ok(Value::String(format_decimal(pnl)))
}

fn perp_liq_price(ctx: &CalcContext<'_>) -> Result<Value, SyncError> {
    if ctx.inputs.len() < 4 {
        return Err(SyncError::DeriveFailed {
            calc_id: "perp_liq_price".into(),
            reason: format!("expected 4 inputs, got {}", ctx.inputs.len()),
        });
    }
    let entry = parse_decimal_f64(&ctx.inputs[0], "entry")?;
    let leverage = parse_decimal_f64(&ctx.inputs[1], "leverage")?;
    let maint = parse_decimal_f64(&ctx.inputs[2], "maintenance_factor")?;
    let side = ctx.inputs[3].as_str().unwrap_or("long");

    if leverage == 0.0 {
        return Err(SyncError::DeriveFailed {
            calc_id: "perp_liq_price".into(),
            reason: "leverage cannot be 0".into(),
        });
    }
    let factor = (1.0 / leverage).mul_add(-maint, 1.0);
    let liq = if side == "short" {
        entry * (2.0 - factor)
    } else {
        entry * factor
    };
    Ok(Value::String(format_decimal(liq)))
}

// ============ helpers ============

fn parse_decimal_f64(v: &Value, name: &str) -> Result<f64, SyncError> {
    match v {
        Value::String(s) => s.parse::<f64>().map_err(|e| SyncError::DeriveFailed {
            calc_id: "calc".into(),
            reason: format!("{name}: parse f64 from '{s}': {e}"),
        }),
        Value::Number(n) => n.as_f64().ok_or_else(|| SyncError::DeriveFailed {
            calc_id: "calc".into(),
            reason: format!("{name}: number not convertible to f64"),
        }),
        _ => Err(SyncError::DeriveFailed {
            calc_id: "calc".into(),
            reason: format!("{name}: expected number or string"),
        }),
    }
}

fn format_decimal(v: f64) -> String {
    if v.abs() >= 1e15 {
        format!("{v:e}")
    } else {
        let s = format!("{v:.8}");
        trim_trailing_zeros(&s).to_string()
    }
}

fn trim_trailing_zeros(s: &str) -> &str {
    if !s.contains('.') {
        return s;
    }
    let t = s.trim_end_matches('0');
    t.trim_end_matches('.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use policy_state::{Address, ChainId, WalletId, WalletState};

    fn dummy_state() -> WalletState {
        WalletState::new(WalletId::new(Address::ZERO, [ChainId::ethereum_mainnet()]))
    }

    #[test]
    fn aave_hf_basic() {
        let state = dummy_state();
        let ctx = CalcContext {
            state: &state,
            inputs: vec![
                Value::String("1000".into()), // collateral
                Value::String("500".into()),  // debt
                Value::String("0.8".into()),  // liq_threshold
            ],
        };
        let v = aave_hf(&ctx).unwrap();
        assert_eq!(v, Value::String("1.6".into()));
    }

    #[test]
    fn aave_hf_no_debt_is_infinite() {
        let state = dummy_state();
        let ctx = CalcContext {
            state: &state,
            inputs: vec![
                Value::String("1000".into()),
                Value::String("0".into()),
                Value::String("0.8".into()),
            ],
        };
        let v = aave_hf(&ctx).unwrap();
        if let Value::String(s) = v {
            assert!(s.starts_with("1e18") || s.contains('e'));
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn perp_pnl_long_profit() {
        let state = dummy_state();
        let ctx = CalcContext {
            state: &state,
            inputs: vec![
                Value::String("3500".into()), // entry
                Value::String("3550".into()), // mark
                Value::String("2".into()),    // size
                Value::String("long".into()),
            ],
        };
        let v = perp_pnl(&ctx).unwrap();
        assert_eq!(v, Value::String("100".into())); // (3550-3500)*2
    }

    #[test]
    fn perp_pnl_short_profit() {
        let state = dummy_state();
        let ctx = CalcContext {
            state: &state,
            inputs: vec![
                Value::String("3500".into()),
                Value::String("3400".into()),
                Value::String("1".into()),
                Value::String("short".into()),
            ],
        };
        let v = perp_pnl(&ctx).unwrap();
        assert_eq!(v, Value::String("100".into())); // (3400-3500)*1*(-1)
    }

    #[test]
    fn registry_known_ids() {
        let r = CalcRegistry::with_builtins();
        let mut ids = r.known_ids();
        ids.sort_unstable();
        assert_eq!(ids, vec!["aave_hf", "perp_liq_price", "perp_pnl"]);
    }

    #[test]
    fn registry_unknown_id_errors() {
        let state = dummy_state();
        let r = CalcRegistry::with_builtins();
        let err = r
            .run(
                "nonexistent",
                &CalcContext {
                    state: &state,
                    inputs: vec![],
                },
            )
            .unwrap_err();
        assert!(matches!(err, SyncError::UnknownCalcId(_)));
    }
}
