//! `decoder_id → DynSolType` registry.
//! Builtins:
//! * Aave V3: getUserAccountData, getReserveData

use std::collections::HashMap;
use std::str::FromStr;

use alloy_dyn_abi::DynSolType;

#[derive(Debug, Default)]
pub struct AbiTypeRegistry {
    by_id: HashMap<String, DynSolType>,
}

pub type ParseError = String;

impl AbiTypeRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, id: &str, signature: &str) -> Result<(), ParseError> {
        let ty = DynSolType::from_str(signature).map_err(|e| e.to_string())?;
        self.by_id.insert(id.to_string(), ty);
        Ok(())
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&DynSolType> {
        self.by_id.get(id)
    }

    #[must_use]
    pub fn with_builtins() -> Self {
        let mut r = Self::new();

        r.register("abi_u256", "(uint256)").unwrap();
        r.register("abi_address", "(address)").unwrap();
        r.register("abi_bool", "(bool)").unwrap();

        // ─── Aave V3 ───
        // getUserAccountData(address) returns (uint256×6)
        // = totalCollateralBase, totalDebtBase, availableBorrowsBase,
        //   currentLiquidationThreshold, ltv, healthFactor
        r.register(
            "aave_v3_user_account_data",
            "(uint256,uint256,uint256,uint256,uint256,uint256)",
        )
        .unwrap();

        // getReserveData(address) returns ReserveData (15 fields)
        r.register(
            "aave_v3_reserve_data",
            // configuration (uint256 bitmap), liquidityIndex, currentLiquidityRate,
            // variableBorrowIndex, currentVariableBorrowRate, currentStableBorrowRate,
            // lastUpdateTimestamp (uint40), id (uint16),
            // aTokenAddress, stableDebtTokenAddress, variableDebtTokenAddress,
            // interestRateStrategyAddress,
            // accruedToTreasury (uint128), unbacked (uint128), isolationModeTotalDebt (uint128)
            "(uint256,uint128,uint128,uint128,uint128,uint128,uint40,uint16,address,address,address,address,uint128,uint128,uint128)",
        )
        .unwrap();

        r.register(
            "aave_v3_current_borrow_rate",
            "(uint256,uint128,uint128,uint128,uint128,uint128,uint40,uint16,address,address,address,address,uint128,uint128,uint128)",
        )
        .unwrap();

        // ─── Compound V3 (Comet) ───
        // getReserves() returns int256
        r.register("comet_reserves", "(int256)").unwrap();

        // ─── Uniswap V3 ───
        // slot0() returns (sqrtPriceX96 uint160, tick int24, observationIndex uint16,
        //                  observationCardinality uint16, observationCardinalityNext uint16,
        //                  feeProtocol uint8, unlocked bool)
        r.register(
            "uniswap_v3_slot0",
            "(uint160,int24,uint16,uint16,uint16,uint8,bool)",
        )
        .unwrap();

        // ─── Uniswap V2 ───
        // getReserves() returns (uint112, uint112, uint32)
        r.register("uniswap_v2_get_reserves", "(uint112,uint112,uint32)")
            .unwrap();

        r
    }

    #[must_use]
    pub fn known_ids(&self) -> Vec<&str> {
        self.by_id.keys().map(std::string::String::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_load() {
        let r = AbiTypeRegistry::with_builtins();
        assert!(r.get("aave_v3_user_account_data").is_some());
        assert!(r.get("aave_v3_reserve_data").is_some());
        assert!(r.get("uniswap_v3_slot0").is_some());
        assert!(r.get("nonexistent").is_none());
    }

    #[test]
    fn register_custom() {
        let mut r = AbiTypeRegistry::new();
        r.register("my_func", "(uint256,address)").unwrap();
        assert!(r.get("my_func").is_some());
    }

    #[test]
    fn invalid_signature_errors() {
        let mut r = AbiTypeRegistry::new();
        let result = r.register("bad", "totally not valid abi");
        assert!(result.is_err());
    }
}
