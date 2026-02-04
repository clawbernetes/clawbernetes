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
    #[must_use] 
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
    #[must_use] 
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
    #[must_use] 
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
    #[must_use] 
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
    #[must_use] 
    pub fn get_order(&self, order_id: &str) -> Option<&JobOrder> {
        self.orders.get(order_id)
    }

    /// Gets an offer by ID.
    #[must_use] 
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
                    && offer.price_per_hour * u64::from(order.requirements.max_duration_hours)
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

    // =========================================================================
    // Additional Coverage Tests
    // =========================================================================

    #[test]
    fn job_requirements_equality() {
        let req1 = JobRequirements {
            min_gpus: 4,
            gpu_model: Some("A100".to_string()),
            min_memory_gb: 80,
            max_duration_hours: 24,
        };
        let req2 = JobRequirements {
            min_gpus: 4,
            gpu_model: Some("A100".to_string()),
            min_memory_gb: 80,
            max_duration_hours: 24,
        };
        let req3 = JobRequirements {
            min_gpus: 8,
            gpu_model: Some("H100".to_string()),
            min_memory_gb: 80,
            max_duration_hours: 24,
        };

        assert_eq!(req1, req2);
        assert_ne!(req1, req3);
    }

    #[test]
    fn job_requirements_clone() {
        let req = JobRequirements {
            min_gpus: 4,
            gpu_model: Some("A100".to_string()),
            min_memory_gb: 80,
            max_duration_hours: 24,
        };
        let cloned = req.clone();
        assert_eq!(req, cloned);
    }

    #[test]
    fn job_requirements_no_model() {
        let req = JobRequirements {
            min_gpus: 4,
            gpu_model: None,
            min_memory_gb: 40,
            max_duration_hours: 12,
        };
        assert!(req.gpu_model.is_none());
    }

    #[test]
    fn gpu_capacity_equality() {
        let cap1 = GpuCapacity {
            count: 8,
            model: "H100".to_string(),
            memory_gb: 80,
        };
        let cap2 = GpuCapacity {
            count: 8,
            model: "H100".to_string(),
            memory_gb: 80,
        };
        let cap3 = GpuCapacity {
            count: 4,
            model: "A100".to_string(),
            memory_gb: 40,
        };

        assert_eq!(cap1, cap2);
        assert_ne!(cap1, cap3);
    }

    #[test]
    fn capacity_offer_satisfies_requirements() {
        let offer = CapacityOffer::new(
            "provider-1".to_string(),
            GpuCapacity {
                count: 8,
                model: "A100".to_string(),
                memory_gb: 80,
            },
            100,
            900,
        );

        // Satisfies: fewer GPUs, same model, less memory
        let req1 = JobRequirements {
            min_gpus: 4,
            gpu_model: Some("A100".to_string()),
            min_memory_gb: 40,
            max_duration_hours: 24,
        };
        assert!(offer.satisfies(&req1));

        // Satisfies: no model requirement
        let req2 = JobRequirements {
            min_gpus: 4,
            gpu_model: None,
            min_memory_gb: 40,
            max_duration_hours: 24,
        };
        assert!(offer.satisfies(&req2));
    }

    #[test]
    fn capacity_offer_does_not_satisfy_wrong_model() {
        let offer = CapacityOffer::new(
            "provider-1".to_string(),
            GpuCapacity {
                count: 8,
                model: "A100".to_string(),
                memory_gb: 80,
            },
            100,
            900,
        );

        let req = JobRequirements {
            min_gpus: 4,
            gpu_model: Some("H100".to_string()), // Different model
            min_memory_gb: 40,
            max_duration_hours: 24,
        };
        assert!(!offer.satisfies(&req));
    }

    #[test]
    fn capacity_offer_does_not_satisfy_insufficient_memory() {
        let offer = CapacityOffer::new(
            "provider-1".to_string(),
            GpuCapacity {
                count: 8,
                model: "A100".to_string(),
                memory_gb: 40,
            },
            100,
            900,
        );

        let req = JobRequirements {
            min_gpus: 4,
            gpu_model: Some("A100".to_string()),
            min_memory_gb: 80, // Requires more memory than available
            max_duration_hours: 24,
        };
        assert!(!offer.satisfies(&req));
    }

    #[test]
    fn orderbook_empty() {
        let book = OrderBook::new();
        assert!(book.get_offer("nonexistent").is_none());
        assert!(book.get_order("nonexistent").is_none());
    }

    #[test]
    fn orderbook_get_offer() {
        let mut book = OrderBook::new();
        
        let offer = CapacityOffer::new(
            "provider-1".to_string(),
            GpuCapacity {
                count: 4,
                model: "A100".to_string(),
                memory_gb: 80,
            },
            50,
            900,
        );
        let offer_id = offer.id.clone();
        book.insert_offer(offer);

        let retrieved = book.get_offer(&offer_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().provider, "provider-1");
    }

    #[test]
    fn orderbook_get_order() {
        let mut book = OrderBook::new();
        
        let order = JobOrder::new(
            "buyer-1".to_string(),
            JobRequirements {
                min_gpus: 4,
                gpu_model: None,
                min_memory_gb: 40,
                max_duration_hours: 24,
            },
            500,
        );
        let order_id = order.id.clone();
        book.insert_order(order);

        let retrieved = book.get_order(&order_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().buyer, "buyer-1");
    }

    #[test]
    fn orderbook_remove_order() {
        let mut book = OrderBook::new();
        
        let order = JobOrder::new(
            "buyer-1".to_string(),
            JobRequirements {
                min_gpus: 4,
                gpu_model: None,
                min_memory_gb: 40,
                max_duration_hours: 24,
            },
            500,
        );
        let order_id = order.id.clone();
        book.insert_order(order);

        assert!(book.get_order(&order_id).is_some());
        let removed = book.remove_order(&order_id);
        assert!(removed.is_some());
        assert!(book.get_order(&order_id).is_none());
    }

    #[test]
    fn orderbook_multiple_offers() {
        let mut book = OrderBook::new();
        
        for i in 0..5 {
            let offer = CapacityOffer::new(
                format!("provider-{}", i),
                GpuCapacity {
                    count: 4 + i,
                    model: "A100".to_string(),
                    memory_gb: 80,
                },
                50 + i as u64 * 10, // 50, 60, 70, 80, 90 per hour
                800 + i * 20,
            );
            book.insert_offer(offer);
        }

        // Order that multiple offers can satisfy
        // max_price = price_per_hour * max_duration_hours
        // 90/hour * 24 hours = 2160, so max_price should be at least 2160
        let order = JobOrder::new(
            "buyer-1".to_string(),
            JobRequirements {
                min_gpus: 4,
                gpu_model: Some("A100".to_string()),
                min_memory_gb: 40,
                max_duration_hours: 24,
            },
            3000, // High enough to cover 90/hour * 24 hours
        );
        let order_id = order.id.clone();
        book.insert_order(order);

        let matches = book.find_matches(&order_id).unwrap();
        assert_eq!(matches.len(), 5);
    }

    #[test]
    fn orderbook_match_not_found() {
        let book = OrderBook::new();
        let result = book.find_matches("nonexistent");
        assert!(matches!(result, Err(MarketError::OrderNotFound(_))));
    }

    #[test]
    fn order_match_score_sorting() {
        let mut book = OrderBook::new();
        
        // Low reputation, low price
        book.insert_offer(CapacityOffer::new(
            "provider-low".to_string(),
            GpuCapacity {
                count: 8,
                model: "A100".to_string(),
                memory_gb: 80,
            },
            50,
            500, // Low reputation
        ));

        // High reputation, higher price
        book.insert_offer(CapacityOffer::new(
            "provider-high".to_string(),
            GpuCapacity {
                count: 8,
                model: "A100".to_string(),
                memory_gb: 80,
            },
            100,
            950, // High reputation
        ));

        let order = JobOrder::new(
            "buyer-1".to_string(),
            JobRequirements {
                min_gpus: 4,
                gpu_model: Some("A100".to_string()),
                min_memory_gb: 40,
                max_duration_hours: 24,
            },
            3000, // High enough: 100/hour * 24 = 2400
        );
        let order_id = order.id.clone();
        book.insert_order(order);

        let matches = book.find_matches(&order_id).unwrap();
        assert_eq!(matches.len(), 2);
        
        // Higher reputation should be first (higher score)
        assert!(matches[0].score >= matches[1].score);
    }

    #[test]
    fn order_match_price_check() {
        let mut book = OrderBook::new();
        
        // Expensive offer: 1000/hour * 24 hours = 24000 total
        book.insert_offer(CapacityOffer::new(
            "provider-expensive".to_string(),
            GpuCapacity {
                count: 8,
                model: "A100".to_string(),
                memory_gb: 80,
            },
            1000, // Very expensive per hour
            900,
        ));

        // Order with low budget (100 total won't cover 1000/hour * 24 = 24000)
        let order = JobOrder::new(
            "buyer-1".to_string(),
            JobRequirements {
                min_gpus: 4,
                gpu_model: Some("A100".to_string()),
                min_memory_gb: 40,
                max_duration_hours: 24,
            },
            100, // Low max price (total budget)
        );
        let order_id = order.id.clone();
        book.insert_order(order);

        let matches = book.find_matches(&order_id).unwrap();
        // Should not match due to price (100 < 1000 * 24 = 24000)
        assert!(matches.is_empty());
    }

    #[test]
    fn job_order_serialization() {
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

        let json = serde_json::to_string(&order).expect("serialize");
        let deserialized: JobOrder = serde_json::from_str(&json).expect("deserialize");
        
        assert_eq!(order.id, deserialized.id);
        assert_eq!(order.buyer, deserialized.buyer);
        assert_eq!(order.requirements, deserialized.requirements);
    }

    #[test]
    fn capacity_offer_serialization() {
        let offer = CapacityOffer::new(
            "provider-456".to_string(),
            GpuCapacity {
                count: 8,
                model: "H100".to_string(),
                memory_gb: 80,
            },
            200,
            950,
        );

        let json = serde_json::to_string(&offer).expect("serialize");
        let deserialized: CapacityOffer = serde_json::from_str(&json).expect("deserialize");
        
        assert_eq!(offer.id, deserialized.id);
        assert_eq!(offer.provider, deserialized.provider);
        assert_eq!(offer.gpus, deserialized.gpus);
    }

    #[test]
    fn job_requirements_debug() {
        let req = JobRequirements {
            min_gpus: 4,
            gpu_model: Some("A100".to_string()),
            min_memory_gb: 80,
            max_duration_hours: 24,
        };
        let debug = format!("{:?}", req);
        assert!(debug.contains("JobRequirements"));
    }

    #[test]
    fn gpu_capacity_debug() {
        let cap = GpuCapacity {
            count: 8,
            model: "H100".to_string(),
            memory_gb: 80,
        };
        let debug = format!("{:?}", cap);
        assert!(debug.contains("GpuCapacity"));
    }
}
