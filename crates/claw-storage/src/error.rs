//! Error types for the storage module.

use std::path::PathBuf;

use thiserror::Error;

use crate::types::{AccessMode, VolumeId, VolumeStatus};

/// Result type alias for storage operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur during storage operations.
#[derive(Debug, Error)]
pub enum Error {
    /// The volume was not found.
    #[error("volume not found: {id}")]
    VolumeNotFound {
        /// The volume ID that was not found.
        id: VolumeId,
    },

    /// The volume already exists.
    #[error("volume already exists: {id}")]
    VolumeAlreadyExists {
        /// The volume ID that already exists.
        id: VolumeId,
    },

    /// Invalid volume identifier.
    #[error("invalid volume id: {reason}")]
    InvalidVolumeId {
        /// The reason the volume ID is invalid.
        reason: String,
    },

    /// Invalid mount path.
    #[error("invalid mount path: {reason}")]
    InvalidMountPath {
        /// The reason the mount path is invalid.
        reason: String,
    },

    /// Invalid storage class.
    #[error("invalid storage class: {reason}")]
    InvalidStorageClass {
        /// The reason the storage class is invalid.
        reason: String,
    },

    /// The volume is in an invalid state for the requested operation.
    #[error("invalid volume state: expected {expected:?}, found {actual:?}")]
    InvalidVolumeState {
        /// The expected state.
        expected: VolumeStatus,
        /// The actual state.
        actual: VolumeStatus,
    },

    /// The volume is already attached.
    #[error("volume {volume_id} is already attached to workload {workload_id}")]
    VolumeAlreadyAttached {
        /// The volume ID.
        volume_id: VolumeId,
        /// The workload the volume is attached to.
        workload_id: String,
    },

    /// The volume is not attached.
    #[error("volume {volume_id} is not attached")]
    VolumeNotAttached {
        /// The volume ID.
        volume_id: VolumeId,
    },

    /// Capacity error.
    #[error("capacity error: {reason}")]
    CapacityError {
        /// The reason for the capacity error.
        reason: String,
    },

    /// Access mode mismatch.
    #[error("access mode mismatch: volume has {volume_mode:?}, requested {requested_mode:?}")]
    AccessModeMismatch {
        /// The volume's access mode.
        volume_mode: AccessMode,
        /// The requested access mode.
        requested_mode: AccessMode,
    },

    /// Provisioning failed.
    #[error("provisioning failed: {reason}")]
    ProvisioningFailed {
        /// The reason provisioning failed.
        reason: String,
    },

    /// Path does not exist.
    #[error("path does not exist: {path:?}")]
    PathNotFound {
        /// The path that was not found.
        path: PathBuf,
    },

    /// Invalid NFS configuration.
    #[error("invalid NFS configuration: {reason}")]
    InvalidNfsConfig {
        /// The reason the NFS config is invalid.
        reason: String,
    },

    /// Invalid S3 configuration.
    #[error("invalid S3 configuration: {reason}")]
    InvalidS3Config {
        /// The reason the S3 config is invalid.
        reason: String,
    },

    /// Claim not found.
    #[error("volume claim not found: {claim_id}")]
    ClaimNotFound {
        /// The claim ID that was not found.
        claim_id: String,
    },

    /// Claim already bound.
    #[error("volume claim {claim_id} is already bound to volume {volume_id}")]
    ClaimAlreadyBound {
        /// The claim ID.
        claim_id: String,
        /// The volume ID it is bound to.
        volume_id: VolumeId,
    },

    /// No matching volume for claim.
    #[error("no matching volume for claim {claim_id}: {reason}")]
    NoMatchingVolume {
        /// The claim ID.
        claim_id: String,
        /// The reason no match was found.
        reason: String,
    },

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_volume_not_found() {
        let err = Error::VolumeNotFound {
            id: VolumeId::new("test-vol").expect("valid id"),
        };
        assert!(err.to_string().contains("test-vol"));
    }

    #[test]
    fn test_error_display_invalid_volume_id() {
        let err = Error::InvalidVolumeId {
            reason: "too long".to_string(),
        };
        assert!(err.to_string().contains("too long"));
    }

    #[test]
    fn test_error_display_capacity_error() {
        let err = Error::CapacityError {
            reason: "insufficient space".to_string(),
        };
        assert!(err.to_string().contains("insufficient space"));
    }

    #[test]
    fn test_error_display_access_mode_mismatch() {
        let err = Error::AccessModeMismatch {
            volume_mode: AccessMode::ReadWriteOnce,
            requested_mode: AccessMode::ReadWriteMany,
        };
        let msg = err.to_string();
        assert!(msg.contains("ReadWriteOnce"));
        assert!(msg.contains("ReadWriteMany"));
    }
}
