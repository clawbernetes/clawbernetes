//! Escrow tracking for MOLT compute jobs.
//!
//! Tracks pending escrows (funds held during job execution) and provides
//! balance summaries for CLI queries.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use molt_token::{Amount, EscrowId, EscrowState};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Tracked escrow for a compute job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedEscrow {
    /// Escrow ID.
    pub id: EscrowId,
    /// Amount held in escrow.
    pub amount: Amount,
    /// Job/workload ID this escrow is for.
    pub job_id: String,
    /// Provider peer ID.
    pub provider_id: String,
    /// Buyer peer ID (us).
    pub buyer_id: String,
    /// Current state.
    pub state: EscrowState,
    /// When the escrow was created.
    pub created_at: DateTime<Utc>,
    /// When the escrow expires.
    pub expires_at: DateTime<Utc>,
}

impl TrackedEscrow {
    /// Check if the escrow has expired.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Check if the escrow is still pending (active, not released/refunded).
    #[must_use]
    pub fn is_pending(&self) -> bool {
        matches!(self.state, EscrowState::Active | EscrowState::Creating)
    }
}

/// Summary of escrow balances.
#[derive(Debug, Clone, Default)]
pub struct EscrowSummary {
    /// Total amount in pending escrows (as buyer).
    pub pending_as_buyer: u64,
    /// Total amount in pending escrows (as provider).
    pub pending_as_provider: u64,
    /// Number of active escrows.
    pub active_count: usize,
    /// Number of expired escrows.
    pub expired_count: usize,
}

impl EscrowSummary {
    /// Total pending amount.
    #[must_use]
    pub fn total_pending(&self) -> u64 {
        self.pending_as_buyer.saturating_add(self.pending_as_provider)
    }
}

/// Tracks escrows for compute jobs.
#[derive(Debug, Default)]
pub struct EscrowTracker {
    /// Escrows indexed by ID.
    escrows: HashMap<EscrowId, TrackedEscrow>,
    /// Our peer ID (to determine buyer vs provider).
    our_peer_id: Option<String>,
}

impl EscrowTracker {
    /// Create a new escrow tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set our peer ID for determining roles.
    pub fn set_peer_id(&mut self, peer_id: impl Into<String>) {
        self.our_peer_id = Some(peer_id.into());
    }

    /// Add a new escrow.
    pub fn add(&mut self, escrow: TrackedEscrow) {
        info!(
            escrow_id = %escrow.id,
            amount = %escrow.amount,
            job_id = %escrow.job_id,
            "Tracking new escrow"
        );
        self.escrows.insert(escrow.id.clone(), escrow);
    }

    /// Update escrow state.
    pub fn update_state(&mut self, escrow_id: &EscrowId, state: EscrowState) {
        if let Some(escrow) = self.escrows.get_mut(escrow_id) {
            debug!(
                escrow_id = %escrow_id,
                old_state = ?escrow.state,
                new_state = ?state,
                "Updating escrow state"
            );
            escrow.state = state;
        } else {
            warn!(escrow_id = %escrow_id, "Escrow not found for state update");
        }
    }

    /// Remove an escrow (after completion/refund).
    pub fn remove(&mut self, escrow_id: &EscrowId) -> Option<TrackedEscrow> {
        self.escrows.remove(escrow_id).inspect(|e| {
            info!(
                escrow_id = %escrow_id,
                amount = %e.amount,
                state = ?e.state,
                "Removed escrow"
            );
        })
    }

    /// Get an escrow by ID.
    #[must_use]
    pub fn get(&self, escrow_id: &EscrowId) -> Option<&TrackedEscrow> {
        self.escrows.get(escrow_id)
    }

    /// Get all pending escrows.
    #[must_use]
    pub fn pending_escrows(&self) -> Vec<&TrackedEscrow> {
        self.escrows.values().filter(|e| e.is_pending()).collect()
    }

    /// Get all expired escrows.
    #[must_use]
    pub fn expired_escrows(&self) -> Vec<&TrackedEscrow> {
        self.escrows.values().filter(|e| e.is_expired()).collect()
    }

    /// Get escrow summary.
    #[must_use]
    pub fn summary(&self) -> EscrowSummary {
        let mut summary = EscrowSummary::default();

        for escrow in self.escrows.values() {
            if escrow.is_expired() {
                summary.expired_count += 1;
            } else if escrow.is_pending() {
                summary.active_count += 1;

                let amount = escrow.amount.lamports();
                if self.our_peer_id.as_deref() == Some(&escrow.buyer_id) {
                    summary.pending_as_buyer = summary.pending_as_buyer.saturating_add(amount);
                } else if self.our_peer_id.as_deref() == Some(&escrow.provider_id) {
                    summary.pending_as_provider = summary.pending_as_provider.saturating_add(amount);
                }
            }
        }

        summary
    }

    /// Get pending balance (for CLI).
    #[must_use]
    pub fn pending_balance(&self) -> u64 {
        self.summary().total_pending()
    }

    /// Clean up completed/expired escrows.
    pub fn cleanup_completed(&mut self) {
        let to_remove: Vec<_> = self
            .escrows
            .iter()
            .filter(|(_, e)| !e.is_pending())
            .map(|(id, _)| id.clone())
            .collect();

        for id in to_remove {
            self.escrows.remove(&id);
        }
    }

    /// Get count of tracked escrows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.escrows.len()
    }

    /// Check if no escrows are tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.escrows.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_escrow(id: &str, amount: u64, buyer: &str, provider: &str) -> TrackedEscrow {
        TrackedEscrow {
            id: EscrowId::new(),
            amount: Amount::from_lamports(amount),
            job_id: id.into(),
            buyer_id: buyer.into(),
            provider_id: provider.into(),
            state: EscrowState::Active,
            created_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(24),
        }
    }

    #[test]
    fn test_tracker_new_is_empty() {
        let tracker = EscrowTracker::new();
        assert!(tracker.is_empty());
        assert_eq!(tracker.len(), 0);
    }

    #[test]
    fn test_add_and_get_escrow() {
        let mut tracker = EscrowTracker::new();
        let escrow = make_escrow("job-1", 1_000_000, "buyer-1", "provider-1");
        let id = escrow.id.clone();

        tracker.add(escrow);

        assert!(!tracker.is_empty());
        assert!(tracker.get(&id).is_some());
    }

    #[test]
    fn test_update_state() {
        let mut tracker = EscrowTracker::new();
        let escrow = make_escrow("job-1", 1_000_000, "buyer-1", "provider-1");
        let id = escrow.id.clone();

        tracker.add(escrow);
        tracker.update_state(&id, EscrowState::Released);

        let updated = tracker.get(&id).unwrap();
        assert_eq!(updated.state, EscrowState::Released);
    }

    #[test]
    fn test_remove_escrow() {
        let mut tracker = EscrowTracker::new();
        let escrow = make_escrow("job-1", 1_000_000, "buyer-1", "provider-1");
        let id = escrow.id.clone();

        tracker.add(escrow);
        let removed = tracker.remove(&id);

        assert!(removed.is_some());
        assert!(tracker.is_empty());
    }

    #[test]
    fn test_pending_escrows() {
        let mut tracker = EscrowTracker::new();

        let escrow1 = make_escrow("job-1", 1_000_000, "buyer-1", "provider-1");
        let id1 = escrow1.id.clone();
        tracker.add(escrow1);

        let escrow2 = make_escrow("job-2", 2_000_000, "buyer-1", "provider-2");
        let id2 = escrow2.id.clone();
        tracker.add(escrow2);

        // Mark one as released
        tracker.update_state(&id1, EscrowState::Released);

        let pending = tracker.pending_escrows();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, id2);
    }

    #[test]
    fn test_summary_as_buyer() {
        let mut tracker = EscrowTracker::new();
        tracker.set_peer_id("buyer-1");

        let escrow1 = make_escrow("job-1", 1_000_000, "buyer-1", "provider-1");
        let escrow2 = make_escrow("job-2", 2_000_000, "buyer-1", "provider-2");
        tracker.add(escrow1);
        tracker.add(escrow2);

        let summary = tracker.summary();
        assert_eq!(summary.active_count, 2);
        assert_eq!(summary.pending_as_buyer, 3_000_000);
        assert_eq!(summary.pending_as_provider, 0);
    }

    #[test]
    fn test_summary_as_provider() {
        let mut tracker = EscrowTracker::new();
        tracker.set_peer_id("provider-1");

        let escrow = make_escrow("job-1", 1_000_000, "buyer-1", "provider-1");
        tracker.add(escrow);

        let summary = tracker.summary();
        assert_eq!(summary.pending_as_provider, 1_000_000);
        assert_eq!(summary.pending_as_buyer, 0);
    }

    #[test]
    fn test_pending_balance() {
        let mut tracker = EscrowTracker::new();
        tracker.set_peer_id("buyer-1");

        tracker.add(make_escrow("job-1", 1_000_000, "buyer-1", "provider-1"));
        tracker.add(make_escrow("job-2", 500_000, "buyer-1", "provider-2"));

        assert_eq!(tracker.pending_balance(), 1_500_000);
    }
}
