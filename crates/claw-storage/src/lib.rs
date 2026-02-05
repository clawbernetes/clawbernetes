//! # Claw Storage
//!
//! Persistent storage and volume management for Clawbernetes workloads.
//!
//! This crate provides a comprehensive volume management system including:
//!
//! - **Multiple volume types**: `HostPath`, NFS, S3, and `EmptyDir`
//! - **Volume claims**: Request storage resources dynamically
//! - **Storage classes**: Define different tiers of storage
//! - **Volume lifecycle**: Provisioning, binding, attaching, and deletion
//! - **Workload integration**: Easy volume mounting for containers
//!
//! ## Example
//!
//! ```rust
//! use claw_storage::{
//!     VolumeManager, Volume, VolumeId, VolumeType, VolumeClaim,
//!     AccessMode, WorkloadVolumeSpec, WorkloadVolumeManager,
//!     ContainerVolumeMount,
//! };
//!
//! // Create a volume manager
//! let manager = VolumeManager::new();
//!
//! // Create a volume
//! let volume = Volume::new(
//!     VolumeId::new("data-vol").expect("valid id"),
//!     VolumeType::empty_dir(),
//!     10 * 1024 * 1024 * 1024, // 10 GB
//! ).with_access_mode(AccessMode::ReadWriteMany);
//!
//! // Provision the volume
//! manager.provision_available(volume).expect("provision");
//!
//! // Create a claim
//! let claim = VolumeClaim::new("my-claim", 5 * 1024 * 1024 * 1024);
//! manager.create_claim(claim).expect("create claim");
//!
//! // Bind the volume to the claim
//! let vol_id = VolumeId::new("data-vol").expect("valid id");
//! manager.bind(&vol_id, "my-claim").expect("bind");
//!
//! // Use the workload volume manager for container integration
//! let wvm = WorkloadVolumeManager::new(&manager, "my-workload");
//! let specs = vec![
//!     WorkloadVolumeSpec::empty_dir("cache"),
//!     WorkloadVolumeSpec::from_claim("data", "my-claim"),
//! ];
//! let mounts = vec![
//!     ContainerVolumeMount::new("cache", "/cache"),
//!     ContainerVolumeMount::new("data", "/data"),
//! ];
//!
//! let resolved = wvm.resolve(&specs, &mounts).expect("resolve");
//! wvm.attach_volumes(&resolved).expect("attach");
//!
//! // ... run workload ...
//!
//! // Cleanup
//! wvm.cleanup(&resolved).expect("cleanup");
//! ```
//!
//! ## Volume Types
//!
//! - **`EmptyDir`**: Ephemeral storage tied to the workload lifecycle
//! - **`HostPath`**: Mount a path from the host filesystem
//! - **NFS**: Mount an NFS export
//! - **S3**: Mount S3-compatible object storage (via FUSE)
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │                   Workload                       │
//! │  ┌─────────────┐  ┌─────────────────────────┐   │
//! │  │ Container 1 │  │ Container 2             │   │
//! │  │             │  │                         │   │
//! │  │ /cache (RW) │  │ /data (RO)  /logs (RW)  │   │
//! │  └──────┬──────┘  └─────┬────────────┬──────┘   │
//! └─────────│───────────────│────────────│──────────┘
//!           │               │            │
//!           ▼               ▼            ▼
//! ┌─────────────────────────────────────────────────┐
//! │            WorkloadVolumeManager                │
//! │  ┌──────────┐ ┌──────────┐ ┌──────────────────┐ │
//! │  │ Volume   │ │ Volume   │ │ Volume           │ │
//! │  │ Mount    │ │ Mount    │ │ Mount            │ │
//! │  └────┬─────┘ └────┬─────┘ └────┬─────────────┘ │
//! └───────│────────────│────────────│───────────────┘
//!         │            │            │
//!         ▼            ▼            ▼
//! ┌─────────────────────────────────────────────────┐
//! │                VolumeManager                     │
//! │  ┌─────────┐ ┌─────────┐ ┌─────────┐           │
//! │  │ Volume  │ │ Volume  │ │ Volume  │           │
//! │  │EmptyDir │ │  NFS    │ │HostPath │           │
//! │  └────┬────┘ └────┬────┘ └────┬────┘           │
//! └───────│───────────│───────────│─────────────────┘
//!         │           │           │
//!         ▼           ▼           ▼
//!     [tmpfs]    [NFS Server]  [Host FS]
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod manager;
pub mod types;
pub mod workload;

// Re-export commonly used types
pub use error::{Error, Result};
pub use manager::{VolumeEvent, VolumeManager, VolumeManagerConfig, VolumeManagerStats};
pub use types::{
    AccessMode, ClaimStatus, EmptyDirConfig, EmptyDirMedium, HostPathConfig, HostPathType,
    MountPropagation, NfsConfig, ReclaimPolicy, S3Config, StorageClass, Volume, VolumeClaim,
    VolumeId, VolumeMount, VolumeStatus, VolumeType, VolumeBindingMode,
};
pub use workload::{
    ContainerVolumeMount, ResolvedVolumes, VolumeSource, WorkloadVolumeManager, WorkloadVolumeSpec,
};

/// Library version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    /// Integration test demonstrating the full workflow.
    #[test]
    fn integration_test_full_workflow() {
        // Create manager
        let manager = VolumeManager::new();

        // Register a storage class
        let storage_class = StorageClass::new("standard", "claw.io/local")
            .with_reclaim_policy(ReclaimPolicy::Delete)
            .as_default();
        manager
            .register_storage_class(storage_class)
            .expect("register storage class");

        // Create some pre-provisioned volumes
        for i in 0..3 {
            let volume = Volume::new(
                VolumeId::new(format!("pv-{i}")).expect("valid id"),
                VolumeType::empty_dir(),
                100 * 1024 * 1024 * 1024, // 100 GB each
            )
            .with_access_mode(AccessMode::ReadWriteMany)
            .with_storage_class("standard")
            .with_label("tier", "standard");

            manager.provision_available(volume).expect("provision");
        }

        // Create a workload with various volume types
        let wvm = WorkloadVolumeManager::new(&manager, "ml-training-job");

        let volume_specs = vec![
            // Ephemeral scratch space
            WorkloadVolumeSpec::empty_dir_memory("scratch", 4 * 1024 * 1024 * 1024),
            // Dynamic claim for model storage
            WorkloadVolumeSpec::dynamic_claim(
                "models",
                50 * 1024 * 1024 * 1024,
                AccessMode::ReadWriteOnce,
            )
            .with_storage_class("standard"),
        ];

        let container_mounts = vec![
            ContainerVolumeMount::new("scratch", "/tmp/scratch"),
            ContainerVolumeMount::new("models", "/models"),
        ];

        // Resolve and attach
        let resolved = wvm.resolve(&volume_specs, &container_mounts).expect("resolve");

        assert_eq!(resolved.volume_map.len(), 2);
        assert!(resolved.volume_map.contains_key("scratch"));
        assert!(resolved.volume_map.contains_key("models"));
        assert_eq!(resolved.mounts.len(), 2);

        // Attach volumes to workload
        wvm.attach_volumes(&resolved).expect("attach");

        // Verify attachments
        let stats = manager.stats();
        assert!(stats.attached_volumes >= 1);

        // Simulate workload completion - cleanup
        wvm.cleanup(&resolved).expect("cleanup");

        // Ephemeral volumes should be cleaned up
        let stats = manager.stats();
        assert_eq!(stats.attached_volumes, 0);
    }

    /// Test volume claim binding workflow.
    #[test]
    fn integration_test_claim_binding() {
        let manager = VolumeManager::new();

        // Create multiple volumes with different characteristics
        let small_vol = Volume::new(
            VolumeId::new("small-vol").expect("valid id"),
            VolumeType::empty_dir(),
            10 * 1024 * 1024 * 1024,
        )
        .with_access_mode(AccessMode::ReadWriteOnce)
        .with_storage_class("standard");

        let large_vol = Volume::new(
            VolumeId::new("large-vol").expect("valid id"),
            VolumeType::empty_dir(),
            100 * 1024 * 1024 * 1024,
        )
        .with_access_mode(AccessMode::ReadWriteMany)
        .with_storage_class("premium");

        manager.provision_available(small_vol).expect("provision small");
        manager.provision_available(large_vol).expect("provision large");

        // Create claims
        let small_claim = VolumeClaim::new("small-claim", 5 * 1024 * 1024 * 1024)
            .with_access_mode(AccessMode::ReadWriteOnce)
            .with_storage_class("standard");

        let large_claim = VolumeClaim::new("large-claim", 50 * 1024 * 1024 * 1024)
            .with_access_mode(AccessMode::ReadWriteMany)
            .with_storage_class("premium");

        manager.create_claim(small_claim).expect("create small claim");
        manager.create_claim(large_claim).expect("create large claim");

        // Reconcile should bind both
        let bound = manager.reconcile_claims();
        assert_eq!(bound, 2);

        // Verify bindings
        let small_claim = manager.get_claim("small-claim").expect("small claim");
        assert!(small_claim.is_bound());
        assert_eq!(
            small_claim.bound_volume,
            Some(VolumeId::new("small-vol").expect("valid id"))
        );

        let large_claim = manager.get_claim("large-claim").expect("large claim");
        assert!(large_claim.is_bound());
        assert_eq!(
            large_claim.bound_volume,
            Some(VolumeId::new("large-vol").expect("valid id"))
        );
    }

    /// Test NFS volume configuration.
    #[test]
    fn integration_test_nfs_volume() {
        let manager = VolumeManager::new();

        let nfs_volume = Volume::new(
            VolumeId::new("nfs-share").expect("valid id"),
            VolumeType::Nfs(
                NfsConfig::new("nfs.example.com", "/exports/shared")
                    .with_options(vec!["vers=4".to_string(), "hard".to_string()])
                    .read_only(),
            ),
            0, // NFS doesn't have local capacity
        )
        .with_access_mode(AccessMode::ReadOnlyMany);

        manager.provision_available(nfs_volume).expect("provision");

        let volume = manager
            .get_volume(&VolumeId::new("nfs-share").expect("valid id"))
            .expect("volume");

        if let VolumeType::Nfs(config) = &volume.volume_type {
            assert_eq!(config.server, "nfs.example.com");
            assert_eq!(config.path, "/exports/shared");
            assert!(config.read_only);
            assert_eq!(config.mount_options.len(), 2);
        } else {
            panic!("Expected NFS volume type");
        }
    }

    /// Test S3 volume configuration.
    #[test]
    fn integration_test_s3_volume() {
        let manager = VolumeManager::new();

        let s3_volume = Volume::new(
            VolumeId::new("s3-artifacts").expect("valid id"),
            VolumeType::S3(
                S3Config::new("ml-artifacts")
                    .with_endpoint("https://s3.example.com")
                    .with_region("us-west-2")
                    .with_prefix("models/v1")
                    .with_secret("s3-credentials"),
            ),
            0,
        )
        .with_access_mode(AccessMode::ReadWriteMany);

        manager.provision_available(s3_volume).expect("provision");

        let volume = manager
            .get_volume(&VolumeId::new("s3-artifacts").expect("valid id"))
            .expect("volume");

        if let VolumeType::S3(config) = &volume.volume_type {
            assert_eq!(config.bucket, "ml-artifacts");
            assert_eq!(config.endpoint, Some("https://s3.example.com".to_string()));
            assert_eq!(config.region, Some("us-west-2".to_string()));
            assert_eq!(config.prefix, Some("models/v1".to_string()));
            assert_eq!(config.secret_name, Some("s3-credentials".to_string()));
        } else {
            panic!("Expected S3 volume type");
        }
    }
}
