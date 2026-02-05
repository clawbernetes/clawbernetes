//! Volume manager for provisioning and lifecycle management.
//!
//! The [`VolumeManager`] is the central component for managing volumes:
//! - Provisioning new volumes
//! - Binding volumes to claims
//! - Attaching/detaching volumes from workloads
//! - Volume lifecycle management

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use tracing::{debug, info, warn};

use crate::error::{Error, Result};
use crate::types::{
    ClaimStatus, ReclaimPolicy, StorageClass, Volume, VolumeClaim, VolumeId, VolumeStatus,
};

/// Events emitted by the volume manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeEvent {
    /// A volume was created.
    Created(VolumeId),

    /// A volume became available.
    Available(VolumeId),

    /// A volume was bound to a claim.
    Bound {
        /// The volume ID.
        volume_id: VolumeId,
        /// The claim ID.
        claim_id: String,
    },

    /// A volume was attached to a workload.
    Attached {
        /// The volume ID.
        volume_id: VolumeId,
        /// The workload ID.
        workload_id: String,
    },

    /// A volume was detached from a workload.
    Detached(VolumeId),

    /// A volume was released from a claim.
    Released(VolumeId),

    /// A volume was deleted.
    Deleted(VolumeId),

    /// A volume failed.
    Failed {
        /// The volume ID.
        volume_id: VolumeId,
        /// The error message.
        error: String,
    },
}

/// Configuration for the volume manager.
#[derive(Debug, Clone)]
pub struct VolumeManagerConfig {
    /// Default storage class name.
    pub default_storage_class: Option<String>,

    /// Whether to auto-provision volumes for unbound claims.
    pub auto_provision: bool,

    /// Maximum number of volumes.
    pub max_volumes: usize,

    /// Maximum number of claims.
    pub max_claims: usize,
}

impl Default for VolumeManagerConfig {
    fn default() -> Self {
        Self {
            default_storage_class: None,
            auto_provision: true,
            max_volumes: 10000,
            max_claims: 10000,
        }
    }
}

/// Internal state of the volume manager.
#[derive(Debug, Default)]
struct VolumeManagerState {
    /// All volumes by ID.
    volumes: HashMap<VolumeId, Volume>,

    /// All claims by ID.
    claims: HashMap<String, VolumeClaim>,

    /// Storage classes by name.
    storage_classes: HashMap<String, StorageClass>,

    /// Recent events (ring buffer).
    events: Vec<VolumeEvent>,
}

/// Volume manager for provisioning and lifecycle management.
#[derive(Debug)]
pub struct VolumeManager {
    /// Configuration.
    config: VolumeManagerConfig,

    /// Internal state.
    state: Arc<RwLock<VolumeManagerState>>,
}

impl VolumeManager {
    /// Create a new volume manager with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(VolumeManagerConfig::default())
    }

    /// Create a new volume manager with custom configuration.
    #[must_use]
    pub fn with_config(config: VolumeManagerConfig) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(VolumeManagerState::default())),
        }
    }

    /// Register a storage class.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage class name is invalid.
    pub fn register_storage_class(&self, storage_class: StorageClass) -> Result<()> {
        let mut state = self.state.write();

        if storage_class.is_default {
            // Clear existing default
            for sc in state.storage_classes.values_mut() {
                sc.is_default = false;
            }
        }

        info!(
            name = %storage_class.name,
            provisioner = %storage_class.provisioner,
            "Registered storage class"
        );

        state
            .storage_classes
            .insert(storage_class.name.clone(), storage_class);

        Ok(())
    }

    /// Get a storage class by name.
    #[must_use]
    pub fn get_storage_class(&self, name: &str) -> Option<StorageClass> {
        self.state.read().storage_classes.get(name).cloned()
    }

    /// Get the default storage class.
    #[must_use]
    pub fn default_storage_class(&self) -> Option<StorageClass> {
        let state = self.state.read();

        // First check if there's an explicitly marked default
        if let Some(sc) = state.storage_classes.values().find(|sc| sc.is_default) {
            return Some(sc.clone());
        }

        // Then check config
        if let Some(ref name) = self.config.default_storage_class {
            return state.storage_classes.get(name).cloned();
        }

        None
    }

    /// List all storage classes.
    #[must_use]
    pub fn list_storage_classes(&self) -> Vec<StorageClass> {
        self.state.read().storage_classes.values().cloned().collect()
    }

    /// Provision a new volume.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A volume with the same ID already exists
    /// - The volume configuration is invalid
    /// - The maximum number of volumes has been reached
    pub fn provision(&self, volume: Volume) -> Result<VolumeId> {
        let mut state = self.state.write();

        // Check capacity
        if state.volumes.len() >= self.config.max_volumes {
            return Err(Error::CapacityError {
                reason: format!("maximum volumes ({}) reached", self.config.max_volumes),
            });
        }

        // Check for duplicates
        if state.volumes.contains_key(&volume.id) {
            return Err(Error::VolumeAlreadyExists {
                id: volume.id.clone(),
            });
        }

        // Validate volume
        volume.validate()?;

        let id = volume.id.clone();
        state.volumes.insert(id.clone(), volume);
        state.events.push(VolumeEvent::Created(id.clone()));

        info!(volume_id = %id, "Volume provisioned");

        Ok(id)
    }

    /// Provision a volume and immediately mark it as available.
    ///
    /// # Errors
    ///
    /// Returns an error if provisioning fails.
    pub fn provision_available(&self, mut volume: Volume) -> Result<VolumeId> {
        volume.set_status(VolumeStatus::Available);
        self.provision(volume)
    }

    /// Get a volume by ID.
    #[must_use]
    pub fn get_volume(&self, id: &VolumeId) -> Option<Volume> {
        self.state.read().volumes.get(id).cloned()
    }

    /// List all volumes.
    #[must_use]
    pub fn list_volumes(&self) -> Vec<Volume> {
        self.state.read().volumes.values().cloned().collect()
    }

    /// List available volumes.
    #[must_use]
    pub fn list_available_volumes(&self) -> Vec<Volume> {
        self.state
            .read()
            .volumes
            .values()
            .filter(|v| v.is_available())
            .cloned()
            .collect()
    }

    /// Mark a volume as available.
    ///
    /// # Errors
    ///
    /// Returns an error if the volume is not found or not in the correct state.
    pub fn mark_available(&self, id: &VolumeId) -> Result<()> {
        let mut state = self.state.write();

        let volume = state
            .volumes
            .get_mut(id)
            .ok_or_else(|| Error::VolumeNotFound { id: id.clone() })?;

        if volume.status != VolumeStatus::Pending {
            return Err(Error::InvalidVolumeState {
                expected: VolumeStatus::Pending,
                actual: volume.status,
            });
        }

        volume.set_status(VolumeStatus::Available);
        state.events.push(VolumeEvent::Available(id.clone()));

        debug!(volume_id = %id, "Volume marked available");

        Ok(())
    }

    /// Mark a volume as failed.
    ///
    /// # Errors
    ///
    /// Returns an error if the volume is not found.
    pub fn mark_failed(&self, id: &VolumeId, error: impl Into<String>) -> Result<()> {
        let error_msg = error.into();
        let mut state = self.state.write();

        let volume = state
            .volumes
            .get_mut(id)
            .ok_or_else(|| Error::VolumeNotFound { id: id.clone() })?;

        volume.set_status(VolumeStatus::Failed);
        volume.error_message = Some(error_msg.clone());
        state.events.push(VolumeEvent::Failed {
            volume_id: id.clone(),
            error: error_msg.clone(),
        });

        warn!(volume_id = %id, error = %error_msg, "Volume marked failed");

        Ok(())
    }

    /// Create a volume claim.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The claim ID already exists
    /// - The maximum number of claims has been reached
    pub fn create_claim(&self, claim: VolumeClaim) -> Result<String> {
        let mut state = self.state.write();

        // Check capacity
        if state.claims.len() >= self.config.max_claims {
            return Err(Error::CapacityError {
                reason: format!("maximum claims ({}) reached", self.config.max_claims),
            });
        }

        // Check for duplicates
        if state.claims.contains_key(&claim.id) {
            // Note: "unknown" is always a valid VolumeId, so the inner unwrap is safe
            #[allow(clippy::expect_used)]
            let placeholder_id = VolumeId::new("unknown").expect("'unknown' is always valid");
            return Err(Error::ClaimAlreadyBound {
                claim_id: claim.id.clone(),
                volume_id: state.claims[&claim.id]
                    .bound_volume
                    .clone()
                    .unwrap_or(placeholder_id),
            });
        }

        let id = claim.id.clone();
        state.claims.insert(id.clone(), claim);

        info!(claim_id = %id, "Volume claim created");

        Ok(id)
    }

    /// Get a claim by ID.
    #[must_use]
    pub fn get_claim(&self, id: &str) -> Option<VolumeClaim> {
        self.state.read().claims.get(id).cloned()
    }

    /// List all claims.
    #[must_use]
    pub fn list_claims(&self) -> Vec<VolumeClaim> {
        self.state.read().claims.values().cloned().collect()
    }

    /// List pending claims.
    #[must_use]
    pub fn list_pending_claims(&self) -> Vec<VolumeClaim> {
        self.state
            .read()
            .claims
            .values()
            .filter(|c| c.status == ClaimStatus::Pending)
            .cloned()
            .collect()
    }

    /// Find a matching volume for a claim.
    #[must_use]
    pub fn find_matching_volume(&self, claim: &VolumeClaim) -> Option<VolumeId> {
        let state = self.state.read();

        state
            .volumes
            .values()
            .filter(|v| v.is_available())
            .find(|v| claim.matches_volume(v))
            .map(|v| v.id.clone())
    }

    /// Bind a volume to a claim.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The volume or claim is not found
    /// - The volume is not available
    /// - The claim is already bound
    /// - The volume doesn't match the claim requirements
    pub fn bind(&self, volume_id: &VolumeId, claim_id: &str) -> Result<()> {
        let mut state = self.state.write();

        // Get and validate volume
        let volume = state
            .volumes
            .get(volume_id)
            .ok_or_else(|| Error::VolumeNotFound {
                id: volume_id.clone(),
            })?;

        if !volume.is_available() {
            return Err(Error::InvalidVolumeState {
                expected: VolumeStatus::Available,
                actual: volume.status,
            });
        }

        // Get and validate claim
        let claim = state.claims.get(claim_id).ok_or_else(|| Error::ClaimNotFound {
            claim_id: claim_id.to_string(),
        })?;

        if claim.is_bound() {
            return Err(Error::ClaimAlreadyBound {
                claim_id: claim_id.to_string(),
                volume_id: claim.bound_volume.clone().ok_or_else(|| {
                    Error::ClaimNotFound {
                        claim_id: claim_id.to_string(),
                    }
                })?,
            });
        }

        // Check compatibility
        if !claim.matches_volume(volume) {
            return Err(Error::NoMatchingVolume {
                claim_id: claim_id.to_string(),
                reason: "volume does not meet claim requirements".to_string(),
            });
        }

        // Perform binding
        let volume = state.volumes.get_mut(volume_id).ok_or_else(|| {
            Error::VolumeNotFound {
                id: volume_id.clone(),
            }
        })?;
        volume.set_status(VolumeStatus::Bound);
        volume.bound_claim = Some(claim_id.to_string());

        let claim = state.claims.get_mut(claim_id).ok_or_else(|| {
            Error::ClaimNotFound {
                claim_id: claim_id.to_string(),
            }
        })?;
        claim.status = ClaimStatus::Bound;
        claim.bound_volume = Some(volume_id.clone());

        state.events.push(VolumeEvent::Bound {
            volume_id: volume_id.clone(),
            claim_id: claim_id.to_string(),
        });

        info!(
            volume_id = %volume_id,
            claim_id = %claim_id,
            "Volume bound to claim"
        );

        Ok(())
    }

    /// Try to bind pending claims to available volumes.
    ///
    /// Returns the number of claims that were bound.
    pub fn reconcile_claims(&self) -> usize {
        let pending_claims: Vec<_> = self.list_pending_claims();
        let mut bound_count = 0;

        for claim in pending_claims {
            if let Some(volume_id) = self.find_matching_volume(&claim) {
                if self.bind(&volume_id, &claim.id).is_ok() {
                    bound_count += 1;
                }
            }
        }

        if bound_count > 0 {
            info!(bound_count, "Reconciled pending claims");
        }

        bound_count
    }

    /// Attach a volume to a workload.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The volume is not found
    /// - The volume is already attached
    /// - The volume is not bound
    pub fn attach(&self, volume_id: &VolumeId, workload_id: impl Into<String>) -> Result<()> {
        let workload = workload_id.into();
        let mut state = self.state.write();

        let volume = state.volumes.get_mut(volume_id).ok_or_else(|| {
            Error::VolumeNotFound {
                id: volume_id.clone(),
            }
        })?;

        if volume.is_attached() {
            return Err(Error::VolumeAlreadyAttached {
                volume_id: volume_id.clone(),
                workload_id: volume
                    .attached_to
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
            });
        }

        if volume.status != VolumeStatus::Bound {
            return Err(Error::InvalidVolumeState {
                expected: VolumeStatus::Bound,
                actual: volume.status,
            });
        }

        volume.set_status(VolumeStatus::Attached);
        volume.attached_to = Some(workload.clone());

        state.events.push(VolumeEvent::Attached {
            volume_id: volume_id.clone(),
            workload_id: workload.clone(),
        });

        info!(
            volume_id = %volume_id,
            workload_id = %workload,
            "Volume attached to workload"
        );

        Ok(())
    }

    /// Detach a volume from a workload.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The volume is not found
    /// - The volume is not attached
    pub fn detach(&self, volume_id: &VolumeId) -> Result<()> {
        let mut state = self.state.write();

        let volume = state.volumes.get_mut(volume_id).ok_or_else(|| {
            Error::VolumeNotFound {
                id: volume_id.clone(),
            }
        })?;

        if !volume.is_attached() {
            return Err(Error::VolumeNotAttached {
                volume_id: volume_id.clone(),
            });
        }

        let workload = volume.attached_to.take();
        volume.set_status(VolumeStatus::Bound);

        state.events.push(VolumeEvent::Detached(volume_id.clone()));

        info!(
            volume_id = %volume_id,
            workload_id = ?workload,
            "Volume detached from workload"
        );

        Ok(())
    }

    /// Release a volume from its claim.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The volume is not found
    /// - The volume is not bound
    /// - The volume is still attached
    pub fn release(&self, volume_id: &VolumeId) -> Result<ReclaimPolicy> {
        let mut state = self.state.write();

        let volume = state.volumes.get(volume_id).ok_or_else(|| {
            Error::VolumeNotFound {
                id: volume_id.clone(),
            }
        })?;

        if volume.is_attached() {
            return Err(Error::VolumeAlreadyAttached {
                volume_id: volume_id.clone(),
                workload_id: volume
                    .attached_to
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
            });
        }

        if !volume.is_bound() {
            return Err(Error::InvalidVolumeState {
                expected: VolumeStatus::Bound,
                actual: volume.status,
            });
        }

        let claim_id = volume.bound_claim.clone();

        // Get reclaim policy from storage class
        let reclaim_policy = volume
            .storage_class
            .as_ref()
            .and_then(|sc| state.storage_classes.get(sc))
            .map_or(ReclaimPolicy::Retain, |sc| sc.reclaim_policy);

        // Update volume
        let volume = state.volumes.get_mut(volume_id).ok_or_else(|| {
            Error::VolumeNotFound {
                id: volume_id.clone(),
            }
        })?;
        volume.set_status(VolumeStatus::Releasing);
        volume.bound_claim = None;

        // Update claim if exists
        if let Some(ref cid) = claim_id {
            if let Some(claim) = state.claims.get_mut(cid) {
                claim.status = ClaimStatus::Lost;
                claim.bound_volume = None;
            }
        }

        state.events.push(VolumeEvent::Released(volume_id.clone()));

        info!(
            volume_id = %volume_id,
            claim_id = ?claim_id,
            reclaim_policy = ?reclaim_policy,
            "Volume released"
        );

        // Apply reclaim policy
        match reclaim_policy {
            ReclaimPolicy::Delete => {
                // Mark for deletion
                let volume = state.volumes.get_mut(volume_id).ok_or_else(|| {
                    Error::VolumeNotFound {
                        id: volume_id.clone(),
                    }
                })?;
                volume.set_status(VolumeStatus::Deleting);
            }
            ReclaimPolicy::Recycle => {
                // Mark as pending (will be recycled and made available)
                let volume = state.volumes.get_mut(volume_id).ok_or_else(|| {
                    Error::VolumeNotFound {
                        id: volume_id.clone(),
                    }
                })?;
                volume.set_status(VolumeStatus::Pending);
            }
            ReclaimPolicy::Retain => {
                // Keep in releasing state
            }
        }

        Ok(reclaim_policy)
    }

    /// Delete a volume.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The volume is not found
    /// - The volume is attached
    /// - The volume is bound
    pub fn delete(&self, volume_id: &VolumeId) -> Result<()> {
        let mut state = self.state.write();

        let volume = state.volumes.get(volume_id).ok_or_else(|| {
            Error::VolumeNotFound {
                id: volume_id.clone(),
            }
        })?;

        if volume.is_attached() {
            return Err(Error::VolumeAlreadyAttached {
                volume_id: volume_id.clone(),
                workload_id: volume
                    .attached_to
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
            });
        }

        if volume.is_bound() {
            return Err(Error::InvalidVolumeState {
                expected: VolumeStatus::Available,
                actual: volume.status,
            });
        }

        state.volumes.remove(volume_id);
        state.events.push(VolumeEvent::Deleted(volume_id.clone()));

        info!(volume_id = %volume_id, "Volume deleted");

        Ok(())
    }

    /// Delete a claim.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The claim is not found
    pub fn delete_claim(&self, claim_id: &str) -> Result<Option<VolumeId>> {
        let mut state = self.state.write();

        let claim = state.claims.remove(claim_id).ok_or_else(|| {
            Error::ClaimNotFound {
                claim_id: claim_id.to_string(),
            }
        })?;

        info!(
            claim_id = %claim_id,
            bound_volume = ?claim.bound_volume,
            "Volume claim deleted"
        );

        Ok(claim.bound_volume)
    }

    /// Get recent events.
    #[must_use]
    pub fn events(&self) -> Vec<VolumeEvent> {
        self.state.read().events.clone()
    }

    /// Get the number of volumes.
    #[must_use]
    pub fn volume_count(&self) -> usize {
        self.state.read().volumes.len()
    }

    /// Get the number of claims.
    #[must_use]
    pub fn claim_count(&self) -> usize {
        self.state.read().claims.len()
    }

    /// Get statistics about volumes.
    #[must_use]
    pub fn stats(&self) -> VolumeManagerStats {
        let state = self.state.read();

        let mut pending_volumes = 0;
        let mut available_volumes = 0;
        let mut bound_volumes = 0;
        let mut attached_volumes = 0;
        let mut failed_volumes = 0;
        let mut total_capacity = 0;

        for volume in state.volumes.values() {
            match volume.status {
                VolumeStatus::Pending => pending_volumes += 1,
                VolumeStatus::Available => available_volumes += 1,
                VolumeStatus::Bound => bound_volumes += 1,
                VolumeStatus::Attached => attached_volumes += 1,
                VolumeStatus::Failed => failed_volumes += 1,
                VolumeStatus::Releasing | VolumeStatus::Deleting => {}
            }
            total_capacity += volume.capacity;
        }

        let mut pending_claims = 0;
        let mut bound_claims = 0;
        let mut lost_claims = 0;

        for claim in state.claims.values() {
            match claim.status {
                ClaimStatus::Pending => pending_claims += 1,
                ClaimStatus::Bound => bound_claims += 1,
                ClaimStatus::Lost => lost_claims += 1,
            }
        }

        VolumeManagerStats {
            total_volumes: state.volumes.len(),
            pending_volumes,
            available_volumes,
            bound_volumes,
            attached_volumes,
            failed_volumes,
            total_capacity,
            total_claims: state.claims.len(),
            pending_claims,
            bound_claims,
            lost_claims,
            storage_classes: state.storage_classes.len(),
        }
    }
}

impl Default for VolumeManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the volume manager.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VolumeManagerStats {
    /// Total number of volumes.
    pub total_volumes: usize,

    /// Number of pending volumes.
    pub pending_volumes: usize,

    /// Number of available volumes.
    pub available_volumes: usize,

    /// Number of bound volumes.
    pub bound_volumes: usize,

    /// Number of attached volumes.
    pub attached_volumes: usize,

    /// Number of failed volumes.
    pub failed_volumes: usize,

    /// Total capacity across all volumes in bytes.
    pub total_capacity: u64,

    /// Total number of claims.
    pub total_claims: usize,

    /// Number of pending claims.
    pub pending_claims: usize,

    /// Number of bound claims.
    pub bound_claims: usize,

    /// Number of lost claims.
    pub lost_claims: usize,

    /// Number of storage classes.
    pub storage_classes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AccessMode, VolumeType};

    fn create_test_volume(id: &str) -> Volume {
        let vol_id = VolumeId::new(id).expect("valid id");
        Volume::new(vol_id, VolumeType::empty_dir(), 10 * 1024 * 1024 * 1024)
    }

    fn create_test_claim(id: &str) -> VolumeClaim {
        VolumeClaim::new(id, 5 * 1024 * 1024 * 1024)
    }

    // ===================
    // Basic Operations
    // ===================

    #[test]
    fn volume_manager_new() {
        let manager = VolumeManager::new();
        assert_eq!(manager.volume_count(), 0);
        assert_eq!(manager.claim_count(), 0);
    }

    #[test]
    fn volume_manager_provision() {
        let manager = VolumeManager::new();
        let volume = create_test_volume("test-vol");

        let id = manager.provision(volume).expect("provision should succeed");
        assert_eq!(id.as_str(), "test-vol");
        assert_eq!(manager.volume_count(), 1);

        let vol = manager.get_volume(&id).expect("volume should exist");
        assert_eq!(vol.status, VolumeStatus::Pending);
    }

    #[test]
    fn volume_manager_provision_available() {
        let manager = VolumeManager::new();
        let volume = create_test_volume("test-vol");

        let id = manager
            .provision_available(volume)
            .expect("provision should succeed");

        let vol = manager.get_volume(&id).expect("volume should exist");
        assert_eq!(vol.status, VolumeStatus::Available);
    }

    #[test]
    fn volume_manager_provision_duplicate() {
        let manager = VolumeManager::new();
        let volume1 = create_test_volume("test-vol");
        let volume2 = create_test_volume("test-vol");

        manager.provision(volume1).expect("first provision should succeed");
        let result = manager.provision(volume2);

        assert!(matches!(result, Err(Error::VolumeAlreadyExists { .. })));
    }

    #[test]
    fn volume_manager_get_volume() {
        let manager = VolumeManager::new();
        let volume = create_test_volume("test-vol");
        let id = manager.provision(volume).expect("provision");

        let vol = manager.get_volume(&id).expect("volume should exist");
        assert_eq!(vol.id, id);

        let missing = VolumeId::new("missing").expect("valid id");
        assert!(manager.get_volume(&missing).is_none());
    }

    #[test]
    fn volume_manager_list_volumes() {
        let manager = VolumeManager::new();

        let vol1 = create_test_volume("vol1");
        let vol2 = create_test_volume("vol2");

        manager.provision(vol1).expect("provision");
        manager.provision(vol2).expect("provision");

        let volumes = manager.list_volumes();
        assert_eq!(volumes.len(), 2);
    }

    #[test]
    fn volume_manager_mark_available() {
        let manager = VolumeManager::new();
        let volume = create_test_volume("test-vol");
        let id = manager.provision(volume).expect("provision");

        manager.mark_available(&id).expect("mark available should succeed");

        let vol = manager.get_volume(&id).expect("volume");
        assert!(vol.is_available());
    }

    #[test]
    fn volume_manager_mark_failed() {
        let manager = VolumeManager::new();
        let volume = create_test_volume("test-vol");
        let id = manager.provision(volume).expect("provision");

        manager
            .mark_failed(&id, "disk error")
            .expect("mark failed should succeed");

        let vol = manager.get_volume(&id).expect("volume");
        assert_eq!(vol.status, VolumeStatus::Failed);
        assert_eq!(vol.error_message, Some("disk error".to_string()));
    }

    // ===================
    // Claim Operations
    // ===================

    #[test]
    fn volume_manager_create_claim() {
        let manager = VolumeManager::new();
        let claim = create_test_claim("test-claim");

        let id = manager.create_claim(claim).expect("create claim");
        assert_eq!(id, "test-claim");
        assert_eq!(manager.claim_count(), 1);
    }

    #[test]
    fn volume_manager_get_claim() {
        let manager = VolumeManager::new();
        let claim = create_test_claim("test-claim");
        manager.create_claim(claim).expect("create claim");

        let c = manager.get_claim("test-claim").expect("claim should exist");
        assert_eq!(c.id, "test-claim");

        assert!(manager.get_claim("missing").is_none());
    }

    #[test]
    fn volume_manager_list_claims() {
        let manager = VolumeManager::new();

        let claim1 = create_test_claim("claim1");
        let claim2 = create_test_claim("claim2");

        manager.create_claim(claim1).expect("create claim");
        manager.create_claim(claim2).expect("create claim");

        let claims = manager.list_claims();
        assert_eq!(claims.len(), 2);
    }

    // ===================
    // Binding Operations
    // ===================

    #[test]
    fn volume_manager_bind() {
        let manager = VolumeManager::new();

        let volume = create_test_volume("test-vol");
        let vol_id = manager.provision_available(volume).expect("provision");

        let claim = create_test_claim("test-claim");
        manager.create_claim(claim).expect("create claim");

        manager.bind(&vol_id, "test-claim").expect("bind should succeed");

        let vol = manager.get_volume(&vol_id).expect("volume");
        assert!(vol.is_bound());
        assert_eq!(vol.bound_claim, Some("test-claim".to_string()));

        let c = manager.get_claim("test-claim").expect("claim");
        assert!(c.is_bound());
        assert_eq!(c.bound_volume, Some(vol_id));
    }

    #[test]
    fn volume_manager_bind_not_available() {
        let manager = VolumeManager::new();

        let volume = create_test_volume("test-vol");
        let vol_id = manager.provision(volume).expect("provision"); // Not marked available

        let claim = create_test_claim("test-claim");
        manager.create_claim(claim).expect("create claim");

        let result = manager.bind(&vol_id, "test-claim");
        assert!(matches!(result, Err(Error::InvalidVolumeState { .. })));
    }

    #[test]
    fn volume_manager_find_matching_volume() {
        let manager = VolumeManager::new();

        let volume = create_test_volume("test-vol")
            .with_access_mode(AccessMode::ReadWriteMany)
            .with_storage_class("fast")
            .with_label("app", "web");

        manager.provision_available(volume).expect("provision");

        let claim = create_test_claim("test-claim")
            .with_access_mode(AccessMode::ReadWriteOnce)
            .with_storage_class("fast")
            .with_selector("app", "web");

        let vol_id = manager.find_matching_volume(&claim);
        assert!(vol_id.is_some());
    }

    #[test]
    fn volume_manager_reconcile_claims() {
        let manager = VolumeManager::new();

        // Create available volumes
        for i in 0..3 {
            let volume = create_test_volume(&format!("vol{i}"));
            manager.provision_available(volume).expect("provision");
        }

        // Create claims
        for i in 0..2 {
            let claim = create_test_claim(&format!("claim{i}"));
            manager.create_claim(claim).expect("create claim");
        }

        let bound = manager.reconcile_claims();
        assert_eq!(bound, 2);

        let stats = manager.stats();
        assert_eq!(stats.bound_volumes, 2);
        assert_eq!(stats.bound_claims, 2);
        assert_eq!(stats.available_volumes, 1);
    }

    // ===================
    // Attach/Detach Operations
    // ===================

    #[test]
    fn volume_manager_attach_detach() {
        let manager = VolumeManager::new();

        let volume = create_test_volume("test-vol");
        let vol_id = manager.provision_available(volume).expect("provision");

        let claim = create_test_claim("test-claim");
        manager.create_claim(claim).expect("create claim");

        manager.bind(&vol_id, "test-claim").expect("bind");
        manager.attach(&vol_id, "workload-1").expect("attach");

        let vol = manager.get_volume(&vol_id).expect("volume");
        assert!(vol.is_attached());
        assert_eq!(vol.attached_to, Some("workload-1".to_string()));

        manager.detach(&vol_id).expect("detach");

        let vol = manager.get_volume(&vol_id).expect("volume");
        assert!(!vol.is_attached());
        assert_eq!(vol.status, VolumeStatus::Bound);
    }

    #[test]
    fn volume_manager_attach_not_bound() {
        let manager = VolumeManager::new();

        let volume = create_test_volume("test-vol");
        let vol_id = manager.provision_available(volume).expect("provision");

        let result = manager.attach(&vol_id, "workload-1");
        assert!(matches!(result, Err(Error::InvalidVolumeState { .. })));
    }

    #[test]
    fn volume_manager_detach_not_attached() {
        let manager = VolumeManager::new();

        let volume = create_test_volume("test-vol");
        let vol_id = manager.provision_available(volume).expect("provision");

        let claim = create_test_claim("test-claim");
        manager.create_claim(claim).expect("create claim");
        manager.bind(&vol_id, "test-claim").expect("bind");

        let result = manager.detach(&vol_id);
        assert!(matches!(result, Err(Error::VolumeNotAttached { .. })));
    }

    // ===================
    // Release/Delete Operations
    // ===================

    #[test]
    fn volume_manager_release() {
        let manager = VolumeManager::new();

        let volume = create_test_volume("test-vol");
        let vol_id = manager.provision_available(volume).expect("provision");

        let claim = create_test_claim("test-claim");
        manager.create_claim(claim).expect("create claim");

        manager.bind(&vol_id, "test-claim").expect("bind");

        let policy = manager.release(&vol_id).expect("release");
        assert_eq!(policy, ReclaimPolicy::Retain);

        let vol = manager.get_volume(&vol_id).expect("volume");
        assert_eq!(vol.status, VolumeStatus::Releasing);
        assert!(vol.bound_claim.is_none());
    }

    #[test]
    fn volume_manager_release_while_attached() {
        let manager = VolumeManager::new();

        let volume = create_test_volume("test-vol");
        let vol_id = manager.provision_available(volume).expect("provision");

        let claim = create_test_claim("test-claim");
        manager.create_claim(claim).expect("create claim");

        manager.bind(&vol_id, "test-claim").expect("bind");
        manager.attach(&vol_id, "workload-1").expect("attach");

        let result = manager.release(&vol_id);
        assert!(matches!(result, Err(Error::VolumeAlreadyAttached { .. })));
    }

    #[test]
    fn volume_manager_delete() {
        let manager = VolumeManager::new();

        let volume = create_test_volume("test-vol");
        let vol_id = manager.provision_available(volume).expect("provision");

        manager.delete(&vol_id).expect("delete");

        assert!(manager.get_volume(&vol_id).is_none());
        assert_eq!(manager.volume_count(), 0);
    }

    #[test]
    fn volume_manager_delete_bound() {
        let manager = VolumeManager::new();

        let volume = create_test_volume("test-vol");
        let vol_id = manager.provision_available(volume).expect("provision");

        let claim = create_test_claim("test-claim");
        manager.create_claim(claim).expect("create claim");

        manager.bind(&vol_id, "test-claim").expect("bind");

        let result = manager.delete(&vol_id);
        assert!(matches!(result, Err(Error::InvalidVolumeState { .. })));
    }

    #[test]
    fn volume_manager_delete_claim() {
        let manager = VolumeManager::new();

        let claim = create_test_claim("test-claim");
        manager.create_claim(claim).expect("create claim");

        let result = manager.delete_claim("test-claim").expect("delete claim");
        assert!(result.is_none());
        assert!(manager.get_claim("test-claim").is_none());
    }

    // ===================
    // Storage Class Operations
    // ===================

    #[test]
    fn volume_manager_storage_class() {
        let manager = VolumeManager::new();

        let sc = StorageClass::new("fast", "claw.io/local-ssd")
            .with_reclaim_policy(ReclaimPolicy::Delete)
            .as_default();

        manager.register_storage_class(sc).expect("register");

        let retrieved = manager.get_storage_class("fast").expect("should exist");
        assert_eq!(retrieved.provisioner, "claw.io/local-ssd");
        assert!(retrieved.is_default);

        let default = manager.default_storage_class().expect("should have default");
        assert_eq!(default.name, "fast");
    }

    #[test]
    fn volume_manager_release_with_delete_policy() {
        let manager = VolumeManager::new();

        let sc = StorageClass::new("fast", "claw.io/local-ssd")
            .with_reclaim_policy(ReclaimPolicy::Delete);
        manager.register_storage_class(sc).expect("register");

        let volume = create_test_volume("test-vol").with_storage_class("fast");
        let vol_id = manager.provision_available(volume).expect("provision");

        let claim = create_test_claim("test-claim");
        manager.create_claim(claim).expect("create claim");

        manager.bind(&vol_id, "test-claim").expect("bind");

        let policy = manager.release(&vol_id).expect("release");
        assert_eq!(policy, ReclaimPolicy::Delete);

        let vol = manager.get_volume(&vol_id).expect("volume");
        assert_eq!(vol.status, VolumeStatus::Deleting);
    }

    // ===================
    // Stats and Events
    // ===================

    #[test]
    fn volume_manager_stats() {
        let manager = VolumeManager::new();

        // Create volumes in various states
        let vol1 = create_test_volume("vol1");
        let vol2 = create_test_volume("vol2");

        manager.provision_available(vol1).expect("provision");
        manager.provision(vol2).expect("provision");

        let claim = create_test_claim("claim1");
        manager.create_claim(claim).expect("create claim");

        let stats = manager.stats();
        assert_eq!(stats.total_volumes, 2);
        assert_eq!(stats.available_volumes, 1);
        assert_eq!(stats.pending_volumes, 1);
        assert_eq!(stats.total_claims, 1);
        assert_eq!(stats.pending_claims, 1);
        assert_eq!(stats.total_capacity, 20 * 1024 * 1024 * 1024);
    }

    #[test]
    fn volume_manager_events() {
        let manager = VolumeManager::new();

        let volume = create_test_volume("test-vol");
        let vol_id = manager.provision_available(volume).expect("provision");

        let events = manager.events();
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], VolumeEvent::Created(id) if id == &vol_id));
    }

    // ===================
    // Capacity Limits
    // ===================

    #[test]
    fn volume_manager_max_volumes() {
        let config = VolumeManagerConfig {
            max_volumes: 2,
            ..Default::default()
        };
        let manager = VolumeManager::with_config(config);

        manager
            .provision(create_test_volume("vol1"))
            .expect("provision 1");
        manager
            .provision(create_test_volume("vol2"))
            .expect("provision 2");

        let result = manager.provision(create_test_volume("vol3"));
        assert!(matches!(result, Err(Error::CapacityError { .. })));
    }

    #[test]
    fn volume_manager_max_claims() {
        let config = VolumeManagerConfig {
            max_claims: 2,
            ..Default::default()
        };
        let manager = VolumeManager::with_config(config);

        manager
            .create_claim(create_test_claim("claim1"))
            .expect("claim 1");
        manager
            .create_claim(create_test_claim("claim2"))
            .expect("claim 2");

        let result = manager.create_claim(create_test_claim("claim3"));
        assert!(matches!(result, Err(Error::CapacityError { .. })));
    }
}
