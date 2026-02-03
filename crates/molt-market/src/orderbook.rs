//! Order book for job matching.
//!
//! Provides the core matching engine for the MOLT marketplace,
//! connecting job buyers with capacity providers.

use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::MarketError;

/// Requirements for a compute job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRequirements {
    /// Minimum number of GPUs needed.
    pub min_gpus: u32,
    /// Specific GPU model requested (optional).
    pub gpu_model: Option<String>,
    /// Minimum GPU memory in gigabytes.
    pub min_memory_gb: u32,
    /// Maximum job duration in hours.
    pub max_duration_hours: u32,
}

/// A job order from a buyer seeking compute capacity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobOrder {
    /// Unique identifier for this order.
    pub id: String,
    /// Buyer's identifier.
    pub buyer: String,
    /// Job requirements.
    pub requirements: JobRequirements,
    /// Maximum price willing to pay (in tokens).
    pub max_price: u64,
    /// Unix timestamp when order was created.
    pub created_at: i64,
}

impl JobOrder {
    /// Creates a new job order with a generated ID and timestamp.
    pub fn new(buyer: String, requirements: JobRequirements, max_price: u64) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            buyer,
            requirements,
            max_price,
            created_at: Utc::now().timestamp(),
        }
    }
}

/// GPU capacity specification from a provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuCapacity {
    /// Number of GPUs available.
    pub count: u32,
    /// GPU model name.
    pub model: String,
    /// Memory per GPU in gigabytes.
    pub memory_gb: u32,
}

/// A capacity offer from a provider with available compute resources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityOffer {
    /// Unique identifier for this offer.
    pub id: String,
    /// Provider's identifier.
    pub provider: String,
    /// Available GPU capacity.
    pub gpus: GpuCapacity,
    /// Price per hour in tokens.
    pub price_per_hour: u64,
    /// Provider's reputation score (0-1000).
    pub reputation: u32,
}

impl CapacityOffer {
    /// Creates a new capacity offer with a generated ID.
    pub fn new(provider: String, gpus: GpuCapacity, price_per_hour: u64, reputation: u32) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            provider,
            gpus,
            price_per_hour,
            reputation,
        }
    }

    /// Checks if this offer can satisfy the given job requirements.
    pub fn satisfies(&self, requirements: &JobRequirements) -> bool {
        // Check GPU count
        if self.gpus.count < requirements.min_gpus {
            return false;
        }

        // Check GPU model if specified
        if let Some(ref required_model) = requirements.gpu_model {
            if &self.gpus.model != required_model {
                return false;
            }
        }

        // Check memory
        if self.gpus.memory_gb < requirements.min_memory_gb {
            return false;
        }

        true
    }
}

/// A match between a job order and a capacity offer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderMatch {
    /// The matched order ID.
    pub order_id: String,
    /// The matched offer ID.
    pub offer_id: String,
    /// Match score (higher is better).
    pub score: u32,
}

/// The order book managing job orders and capacity offers.
#[derive(Debug, Default)]
pub struct OrderBook {
    orders: HashMap<String, JobOrder>,
    offers: HashMap<String, CapacityOffer>,
}

impl OrderBook {
    /// Creates a new empty order book.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a job order into the book.
    pub fn insert_order(&mut self, order: JobOrder) {
        self.orders.insert(order.id.clone(), order);
    }

    /// Inserts a capacity offer into the book.
    pub fn insert_offer(&mut self, offer: CapacityOffer) {
        self.offers.insert(offer.id.clone(), offer);
    }

    /// Gets an order by ID.
    pub fn get_order(&self, order_id: &str) -> Option<&JobOrder> {
        self.orders.get(order_id)
    }

    /// Gets an offer by ID.
    pub fn get_offer(&self, offer_id: &str) -> Option<&CapacityOffer> {
        self.offers.get(offer_id)
    }

    /// Removes an order by ID.
    pub fn remove_order(&mut self, order_id: &str) -> Option<JobOrder> {
        self.orders.remove(order_id)
    }

    /// Removes an offer by ID.
    pub fn remove_offer(&mut self, offer_id: &str) -> Option<CapacityOffer> {
        self.offers.remove(offer_id)
    }

    /// Finds matching offers for a given order.
    ///
    /// Returns offers sorted by score (reputation and price weighted).
    pub fn find_matches(&self, order_id: &str) -> Result<Vec<OrderMatch>, MarketError> {
        let order = self
            .orders
            .get(order_id)
            .ok_or_else(|| MarketError::OrderNotFound(order_id.to_string()))?;

        let mut matches: Vec<OrderMatch> = self
            .offers
            .values()
            .filter(|offer| {
                offer.satisfies(&order.requirements)
                    && offer.price_per_hour * order.requirements.max_duration_hours as u64
                        <= order.max_price
            })
            .map(|offer| OrderMatch {
                order_id: order_id.to_string(),
                offer_id: offer.id.clone(),
                // Score: reputation weighted, lower price is better
                score: offer.reputation.saturating_sub((offer.price_per_hour / 10) as u32),
            })
            .collect();

        // Sort by score descending
        matches.sort_by(|a, b| b.score.cmp(&a.score));

        Ok(matches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_order_creation() {
        let order = JobOrder::new(
            "buyer-123".to_string(),
            JobRequirements {
                min_gpus: 4,
                gpu_model: Some("A100".to_string()),
                min_memory_gb: 80,
                max_duration_hours: 24,
            },
            1000,
        );

        assert!(!order.id.is_empty());
        assert_eq!(order.buyer, "buyer-123");
        assert_eq!(order.requirements.min_gpus, 4);
        assert_eq!(order.max_price, 1000);
        assert!(order.created_at > 0);
    }

    #[test]
    fn capacity_offer_creation() {
        let offer = CapacityOffer::new(
            "provider-456".to_string(),
            GpuCapacity {
                count: 8,
                model: "A100".to_string(),
                memory_gb: 80,
            },
            50,
            850,
        );

        assert!(!offer.id.is_empty());
        assert_eq!(offer.provider, "provider-456");
        assert_eq!(offer.gpus.count, 8);
        assert_eq!(offer.price_per_hour, 50);
        assert_eq!(offer.reputation, 850);
    }

    #[test]
    fn orderbook_insert_and_match() {
        let mut book = OrderBook::new();

        // Insert a capacity offer
        let offer = CapacityOffer::new(
            "provider-1".to_string(),
            GpuCapacity {
                count: 8,
                model: "A100".to_string(),
                memory_gb: 80,
            },
            50,
            900,
        );
        let offer_id = offer.id.clone();
        book.insert_offer(offer);

        // Insert a job order that matches
        let order = JobOrder::new(
            "buyer-1".to_string(),
            JobRequirements {
                min_gpus: 4,
                gpu_model: Some("A100".to_string()),
                min_memory_gb: 40,
                max_duration_hours: 8,
            },
            500,
        );
        let order_id = order.id.clone();
        book.insert_order(order);

        // Try to match
        let matches = book.find_matches(&order_id).unwrap();
        assert!(!matches.is_empty());
        assert_eq!(matches[0].offer_id, offer_id);
    }

    #[test]
    fn orderbook_remove_operations() {
        let mut book = OrderBook::new();

        let offer = CapacityOffer::new(
            "provider-1".to_string(),
            GpuCapacity {
                count: 4,
                model: "H100".to_string(),
                memory_gb: 80,
            },
            100,
            750,
        );
        let offer_id = offer.id.clone();
        book.insert_offer(offer);

        assert!(book.get_offer(&offer_id).is_some());
        let removed = book.remove_offer(&offer_id);
        assert!(removed.is_some());
        assert!(book.get_offer(&offer_id).is_none());
    }

    #[test]
    fn orderbook_no_match_insufficient_gpus() {
        let mut book = OrderBook::new();

        // Offer with 2 GPUs
        let offer = CapacityOffer::new(
            "provider-1".to_string(),
            GpuCapacity {
                count: 2,
                model: "A100".to_string(),
                memory_gb: 80,
            },
            50,
            900,
        );
        book.insert_offer(offer);

        // Order requiring 8 GPUs - should not match
        let order = JobOrder::new(
            "buyer-1".to_string(),
            JobRequirements {
                min_gpus: 8,
                gpu_model: Some("A100".to_string()),
                min_memory_gb: 40,
                max_duration_hours: 8,
            },
            500,
        );
        let order_id = order.id.clone();
        book.insert_order(order);

        let matches = book.find_matches(&order_id).unwrap();
        assert!(matches.is_empty());
    }
}
