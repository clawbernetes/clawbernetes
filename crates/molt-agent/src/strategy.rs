//! Pricing and bidding strategies.
//!
//! Provides algorithms for determining optimal bid prices based on
//! market conditions, resource costs, and competitive dynamics.

use serde::{Deserialize, Serialize};

/// Pricing strategy for bid calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum PricingStrategy {
    /// Fixed markup over cost.
    #[default]
    FixedMarkup,
    /// Dynamic pricing based on utilization.
    UtilizationBased,
    /// Competitive pricing based on market.
    MarketBased,
}


impl PricingStrategy {
    /// Calculate bid price based on this strategy.
    ///
    /// # Arguments
    /// * `base_cost` - The base cost of providing the service
    /// * `utilization` - Current utilization (0.0 to 1.0)
    /// * `market_rate` - Current market rate (if known)
    #[must_use]
    pub fn calculate_price(
        &self,
        base_cost: u64,
        utilization: f64,
        market_rate: Option<u64>,
    ) -> u64 {
        match self {
            Self::FixedMarkup => {
                // 20% markup
                ((base_cost as f64) * 1.2).ceil() as u64
            }
            Self::UtilizationBased => {
                // Higher utilization = higher prices
                // Range from 1.1x (low util) to 2.0x (high util)
                let multiplier = utilization.mul_add(0.9, 1.1);
                ((base_cost as f64) * multiplier).ceil() as u64
            }
            Self::MarketBased => {
                // Price at market rate if known, otherwise fixed markup
                market_rate.unwrap_or_else(|| ((base_cost as f64) * 1.2).ceil() as u64)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pricing_strategy_default() {
        assert_eq!(PricingStrategy::default(), PricingStrategy::FixedMarkup);
    }

    #[test]
    fn pricing_strategy_fixed_markup() {
        let price = PricingStrategy::FixedMarkup.calculate_price(100, 0.5, None);
        assert_eq!(price, 120); // 20% markup
    }

    #[test]
    fn pricing_strategy_utilization_low() {
        let price = PricingStrategy::UtilizationBased.calculate_price(100, 0.0, None);
        // 1.1x at 0% utilization = 110, but ceil may give 111 due to float precision
        assert!(price >= 110 && price <= 111);
    }

    #[test]
    fn pricing_strategy_utilization_high() {
        let price = PricingStrategy::UtilizationBased.calculate_price(100, 1.0, None);
        assert_eq!(price, 200); // 2.0x at 100% utilization
    }

    #[test]
    fn pricing_strategy_market_based_with_rate() {
        let price = PricingStrategy::MarketBased.calculate_price(100, 0.5, Some(150));
        assert_eq!(price, 150); // Use market rate
    }

    #[test]
    fn pricing_strategy_market_based_no_rate() {
        let price = PricingStrategy::MarketBased.calculate_price(100, 0.5, None);
        assert_eq!(price, 120); // Fall back to fixed markup
    }

    #[test]
    fn pricing_strategy_serialization() {
        let strategy = PricingStrategy::UtilizationBased;
        let json = serde_json::to_string(&strategy).unwrap();
        assert_eq!(json, "\"utilization_based\"");
        
        let parsed: PricingStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, strategy);
    }
}
