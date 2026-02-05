//! Workload integration for volume management.
//!
//! This module provides types and utilities for integrating volumes with workloads:
//! - [`WorkloadVolumeSpec`]: Volume specification for workloads
//! - [`WorkloadVolumeManager`]: Helper for managing workload volumes

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::{Error, Result};
use crate::manager::VolumeManager;
use crate::types::{
    AccessMode, EmptyDirConfig, HostPathConfig, NfsConfig, S3Config, Volume, VolumeClaim,
    VolumeId, VolumeMount, VolumeStatus, VolumeType,
};

/// A volume specification for a workload.
///
/// This is similar to Kubernetes `PodSpec` volumes - it defines what volumes
/// a workload needs without necessarily specifying pre-existing volume IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkloadVolumeSpec {
    /// Name of the volume (unique within the workload).
    pub name: String,

    /// The volume source specification.
    pub source: VolumeSource,
}

/// Source specification for a workload volume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VolumeSource {
    /// Reference an existing volume by ID.
    ExistingVolume {
        /// The volume ID.
        volume_id: VolumeId,
    },

    /// Reference an existing volume claim.
    VolumeClaim {
        /// The claim name.
        claim_name: String,

        /// Whether the mount should be read-only.
        read_only: bool,
    },

    /// An ephemeral empty directory.
    EmptyDir(EmptyDirConfig),

    /// A host path mount.
    HostPath(HostPathConfig),

    /// An NFS mount.
    Nfs(NfsConfig),

    /// An S3 mount.
    S3(S3Config),

    /// Create a new claim with these parameters.
    DynamicClaim {
        /// Requested capacity in bytes.
        capacity: u64,

        /// Requested access mode.
        access_mode: AccessMode,

        /// Storage class to use.
        storage_class: Option<String>,
    },
}

impl WorkloadVolumeSpec {
    /// Create a volume spec referencing an existing volume.
    #[must_use]
    pub fn existing(name: impl Into<String>, volume_id: VolumeId) -> Self {
        Self {
            name: name.into(),
            source: VolumeSource::ExistingVolume { volume_id },
        }
    }

    /// Create a volume spec referencing a claim.
    #[must_use]
    pub fn from_claim(name: impl Into<String>, claim_name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: VolumeSource::VolumeClaim {
                claim_name: claim_name.into(),
                read_only: false,
            },
        }
    }

    /// Create an empty directory volume spec.
    #[must_use]
    pub fn empty_dir(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: VolumeSource::EmptyDir(EmptyDirConfig::default()),
        }
    }

    /// Create an empty directory with memory backing.
    #[must_use]
    pub fn empty_dir_memory(name: impl Into<String>, size_limit: u64) -> Self {
        Self {
            name: name.into(),
            source: VolumeSource::EmptyDir(EmptyDirConfig::default().memory().with_size_limit(size_limit)),
        }
    }

    /// Create a host path volume spec.
    #[must_use]
    pub fn host_path(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            source: VolumeSource::HostPath(HostPathConfig::new(path)),
        }
    }

    /// Create an NFS volume spec.
    #[must_use]
    pub fn nfs(
        name: impl Into<String>,
        server: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            source: VolumeSource::Nfs(NfsConfig::new(server, path)),
        }
    }

    /// Create an S3 volume spec.
    #[must_use]
    pub fn s3(name: impl Into<String>, bucket: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: VolumeSource::S3(S3Config::new(bucket)),
        }
    }

    /// Create a dynamic claim volume spec.
    #[must_use]
    pub fn dynamic_claim(
        name: impl Into<String>,
        capacity: u64,
        access_mode: AccessMode,
    ) -> Self {
        Self {
            name: name.into(),
            source: VolumeSource::DynamicClaim {
                capacity,
                access_mode,
                storage_class: None,
            },
        }
    }

    /// Set storage class for dynamic claim.
    #[must_use]
    pub fn with_storage_class(mut self, class: impl Into<String>) -> Self {
        if let VolumeSource::DynamicClaim {
            ref mut storage_class,
            ..
        } = self.source
        {
            *storage_class = Some(class.into());
        }
        self
    }

    /// Make read-only (for claim sources).
    #[must_use]
    pub fn read_only(mut self) -> Self {
        if let VolumeSource::VolumeClaim {
            ref mut read_only, ..
        } = self.source
        {
            *read_only = true;
        }
        self
    }
}

/// A container's volume mount specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerVolumeMount {
    /// Name of the volume (must match a `WorkloadVolumeSpec` name).
    pub name: String,

    /// The path inside the container.
    pub mount_path: PathBuf,

    /// Optional sub-path within the volume.
    pub sub_path: Option<String>,

    /// Whether to mount read-only.
    pub read_only: bool,
}

impl ContainerVolumeMount {
    /// Create a new container volume mount.
    #[must_use]
    pub fn new(name: impl Into<String>, mount_path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            mount_path: mount_path.into(),
            sub_path: None,
            read_only: false,
        }
    }

    /// Set a sub-path within the volume.
    #[must_use]
    pub fn with_sub_path(mut self, sub_path: impl Into<String>) -> Self {
        self.sub_path = Some(sub_path.into());
        self
    }

    /// Make the mount read-only.
    #[must_use]
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }
}

/// Result of resolving workload volumes.
#[derive(Debug, Clone)]
pub struct ResolvedVolumes {
    /// Map from volume spec name to resolved volume ID.
    pub volume_map: HashMap<String, VolumeId>,

    /// Volume mounts to apply to containers.
    pub mounts: Vec<VolumeMount>,

    /// Any claims that were created.
    pub created_claims: Vec<String>,

    /// Any ephemeral volumes that were created.
    pub created_volumes: Vec<VolumeId>,
}

impl Default for ResolvedVolumes {
    fn default() -> Self {
        Self::new()
    }
}

impl ResolvedVolumes {
    /// Create a new empty resolved volumes result.
    #[must_use]
    pub fn new() -> Self {
        Self {
            volume_map: HashMap::new(),
            mounts: Vec::new(),
            created_claims: Vec::new(),
            created_volumes: Vec::new(),
        }
    }
}

/// Helper for managing volumes for a specific workload.
pub struct WorkloadVolumeManager<'a> {
    /// The volume manager.
    manager: &'a VolumeManager,

    /// The workload ID.
    workload_id: String,
}

impl<'a> WorkloadVolumeManager<'a> {
    /// Create a new workload volume manager.
    #[must_use]
    pub fn new(manager: &'a VolumeManager, workload_id: impl Into<String>) -> Self {
        Self {
            manager,
            workload_id: workload_id.into(),
        }
    }

    /// Resolve volume specifications and prepare them for mounting.
    ///
    /// This will:
    /// - Look up existing volumes and claims
    /// - Create dynamic claims as needed
    /// - Create ephemeral volumes (`EmptyDir`)
    /// - Return the resolved volume IDs and mount configurations
    ///
    /// # Errors
    ///
    /// Returns an error if volumes cannot be resolved or created.
    pub fn resolve(
        &self,
        volume_specs: &[WorkloadVolumeSpec],
        container_mounts: &[ContainerVolumeMount],
    ) -> Result<ResolvedVolumes> {
        let mut result = ResolvedVolumes::new();

        // First pass: resolve/create volumes for each spec
        for spec in volume_specs {
            let volume_id = self.resolve_volume_spec(spec, &mut result)?;
            result.volume_map.insert(spec.name.clone(), volume_id);
        }

        // Second pass: create mounts for containers
        for container_mount in container_mounts {
            let volume_id = result
                .volume_map
                .get(&container_mount.name)
                .ok_or_else(|| {
                    // Try to create a VolumeId from the mount name for a better error message
                    // If the name is invalid, create a sanitized version
                    let sanitized_name: String = container_mount
                        .name
                        .chars()
                        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-' || *c == '_')
                        .take(253)
                        .collect();
                    let id_str = if sanitized_name.is_empty()
                        || !sanitized_name.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
                    {
                        "unknown".to_string()
                    } else {
                        sanitized_name
                    };
                    // This unwrap_or_else is safe because we've sanitized the input
                    #[allow(clippy::expect_used)]
                    let id = VolumeId::new(&id_str).unwrap_or_else(|_| {
                        VolumeId::new("unknown").expect("'unknown' is a valid volume id")
                    });
                    Error::VolumeNotFound { id }
                })?
                .clone();

            let mount = VolumeMount::new(volume_id, &container_mount.mount_path);
            let mount = if container_mount.read_only {
                mount.read_only()
            } else {
                mount
            };
            let mount = if let Some(ref sub_path) = container_mount.sub_path {
                mount.with_sub_path(sub_path)
            } else {
                mount
            };

            mount.validate()?;
            result.mounts.push(mount);
        }

        Ok(result)
    }

    /// Resolve a single volume spec to a volume ID.
    fn resolve_volume_spec(
        &self,
        spec: &WorkloadVolumeSpec,
        result: &mut ResolvedVolumes,
    ) -> Result<VolumeId> {
        match &spec.source {
            VolumeSource::ExistingVolume { volume_id } => {
                // Verify volume exists
                let _ = self.manager.get_volume(volume_id).ok_or_else(|| {
                    Error::VolumeNotFound {
                        id: volume_id.clone(),
                    }
                })?;
                Ok(volume_id.clone())
            }

            VolumeSource::VolumeClaim { claim_name, .. } => {
                // Look up the claim and get its bound volume
                let claim = self.manager.get_claim(claim_name).ok_or_else(|| {
                    Error::ClaimNotFound {
                        claim_id: claim_name.clone(),
                    }
                })?;

                claim.bound_volume.clone().ok_or_else(|| Error::NoMatchingVolume {
                    claim_id: claim_name.clone(),
                    reason: "claim is not bound to a volume".to_string(),
                })
            }

            VolumeSource::EmptyDir(config) => {
                // Create an ephemeral volume
                let volume_id = VolumeId::new(format!("{}-{}", self.workload_id, spec.name))?;
                let volume = Volume::new(
                    volume_id.clone(),
                    VolumeType::EmptyDir(config.clone()),
                    config.size_limit,
                );

                self.manager.provision_available(volume)?;
                result.created_volumes.push(volume_id.clone());
                Ok(volume_id)
            }

            VolumeSource::HostPath(config) => {
                // Create a host path volume
                config.validate()?;

                let volume_id = VolumeId::new(format!("{}-{}", self.workload_id, spec.name))?;
                let volume = Volume::new(
                    volume_id.clone(),
                    VolumeType::HostPath(config.clone()),
                    0, // Host path doesn't have a defined capacity
                );

                self.manager.provision_available(volume)?;
                result.created_volumes.push(volume_id.clone());
                Ok(volume_id)
            }

            VolumeSource::Nfs(config) => {
                // Create an NFS volume
                config.validate()?;

                let volume_id = VolumeId::new(format!("{}-{}", self.workload_id, spec.name))?;
                let volume = Volume::new(
                    volume_id.clone(),
                    VolumeType::Nfs(config.clone()),
                    0, // NFS doesn't have a local capacity concept
                );

                self.manager.provision_available(volume)?;
                result.created_volumes.push(volume_id.clone());
                Ok(volume_id)
            }

            VolumeSource::S3(config) => {
                // Create an S3 volume
                config.validate()?;

                let volume_id = VolumeId::new(format!("{}-{}", self.workload_id, spec.name))?;
                let volume = Volume::new(
                    volume_id.clone(),
                    VolumeType::S3(config.clone()),
                    0, // S3 doesn't have a capacity concept
                );

                self.manager.provision_available(volume)?;
                result.created_volumes.push(volume_id.clone());
                Ok(volume_id)
            }

            VolumeSource::DynamicClaim {
                capacity,
                access_mode,
                storage_class,
            } => {
                // Create a dynamic claim
                let claim_id = format!("{}-{}", self.workload_id, spec.name);
                let mut claim = VolumeClaim::new(&claim_id, *capacity)
                    .with_access_mode(*access_mode)
                    .with_owner(&self.workload_id);

                if let Some(sc) = storage_class {
                    claim = claim.with_storage_class(sc);
                }

                self.manager.create_claim(claim)?;
                result.created_claims.push(claim_id.clone());

                // Try to find and bind a matching volume
                if let Some(volume_id) = self.manager.find_matching_volume(
                    &self.manager.get_claim(&claim_id).ok_or_else(|| {
                        Error::ClaimNotFound {
                            claim_id: claim_id.clone(),
                        }
                    })?,
                ) {
                    self.manager.bind(&volume_id, &claim_id)?;
                    return Ok(volume_id);
                }

                Err(Error::NoMatchingVolume {
                    claim_id,
                    reason: "no available volume matches the claim".to_string(),
                })
            }
        }
    }

    /// Attach all resolved volumes to the workload.
    ///
    /// # Errors
    ///
    /// Returns an error if any volume cannot be attached.
    pub fn attach_volumes(&self, resolved: &ResolvedVolumes) -> Result<()> {
        for volume_id in resolved.volume_map.values() {
            // Get current volume state
            let volume = self.manager.get_volume(volume_id).ok_or_else(|| {
                Error::VolumeNotFound {
                    id: volume_id.clone(),
                }
            })?;

            // Only attach if bound (ephemeral volumes are created as Available, not Bound)
            match volume.status {
                VolumeStatus::Bound => {
                    self.manager.attach(volume_id, &self.workload_id)?;
                }
                VolumeStatus::Available => {
                    // Bind to a synthetic claim first for ephemeral volumes
                    let claim_id = format!("{}-ephemeral-{}", self.workload_id, volume_id);
                    let claim = VolumeClaim::new(&claim_id, 0).with_owner(&self.workload_id);

                    // Ignore error if claim already exists
                    let _ = self.manager.create_claim(claim);

                    self.manager.bind(volume_id, &claim_id)?;
                    self.manager.attach(volume_id, &self.workload_id)?;
                }
                VolumeStatus::Attached => {
                    // Already attached, check if it's to us
                    if volume.attached_to.as_ref() != Some(&self.workload_id) {
                        return Err(Error::VolumeAlreadyAttached {
                            volume_id: volume_id.clone(),
                            workload_id: volume
                                .attached_to
                                .unwrap_or_else(|| "unknown".to_string()),
                        });
                    }
                }
                _ => {
                    return Err(Error::InvalidVolumeState {
                        expected: VolumeStatus::Bound,
                        actual: volume.status,
                    });
                }
            }
        }

        Ok(())
    }

    /// Detach all volumes from the workload.
    ///
    /// # Errors
    ///
    /// Returns an error if any volume cannot be detached.
    pub fn detach_volumes(&self, resolved: &ResolvedVolumes) -> Result<()> {
        for volume_id in resolved.volume_map.values() {
            let Some(volume) = self.manager.get_volume(volume_id) else {
                continue; // Volume might have been deleted
            };

            if volume.attached_to.as_ref() == Some(&self.workload_id) {
                self.manager.detach(volume_id)?;
            }
        }

        Ok(())
    }

    /// Clean up volumes created for this workload.
    ///
    /// This releases claims and deletes ephemeral volumes.
    ///
    /// # Errors
    ///
    /// Returns an error if cleanup fails.
    pub fn cleanup(&self, resolved: &ResolvedVolumes) -> Result<()> {
        // First detach
        self.detach_volumes(resolved)?;

        // Delete created claims
        for claim_id in &resolved.created_claims {
            let _ = self.manager.delete_claim(claim_id);
        }

        // Delete ephemeral volumes
        for volume_id in &resolved.created_volumes {
            let Some(volume) = self.manager.get_volume(volume_id) else {
                continue;
            };

            // Release if bound
            if volume.is_bound() && !volume.is_attached() {
                let _ = self.manager.release(volume_id);
            }

            // Delete the volume
            if let Some(v) = self.manager.get_volume(volume_id) {
                if v.status == VolumeStatus::Available
                    || v.status == VolumeStatus::Releasing
                    || v.status == VolumeStatus::Failed
                {
                    let _ = self.manager.delete(volume_id);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EmptyDirMedium;

    fn setup_manager() -> VolumeManager {
        VolumeManager::new()
    }

    // ===================
    // WorkloadVolumeSpec Tests
    // ===================

    #[test]
    fn workload_volume_spec_existing() {
        let vol_id = VolumeId::new("test-vol").expect("valid id");
        let spec = WorkloadVolumeSpec::existing("data", vol_id.clone());

        assert_eq!(spec.name, "data");
        assert!(matches!(
            spec.source,
            VolumeSource::ExistingVolume { volume_id } if volume_id == vol_id
        ));
    }

    #[test]
    fn workload_volume_spec_from_claim() {
        let spec = WorkloadVolumeSpec::from_claim("data", "my-claim");

        assert_eq!(spec.name, "data");
        assert!(matches!(
            spec.source,
            VolumeSource::VolumeClaim { claim_name, read_only }
                if claim_name == "my-claim" && !read_only
        ));
    }

    #[test]
    fn workload_volume_spec_empty_dir() {
        let spec = WorkloadVolumeSpec::empty_dir("cache");

        assert_eq!(spec.name, "cache");
        assert!(matches!(spec.source, VolumeSource::EmptyDir(_)));
    }

    #[test]
    fn workload_volume_spec_empty_dir_memory() {
        let spec = WorkloadVolumeSpec::empty_dir_memory("cache", 1024 * 1024 * 1024);

        assert_eq!(spec.name, "cache");
        if let VolumeSource::EmptyDir(config) = spec.source {
            assert_eq!(config.medium, EmptyDirMedium::Memory);
            assert_eq!(config.size_limit, 1024 * 1024 * 1024);
        } else {
            panic!("Expected EmptyDir source");
        }
    }

    #[test]
    fn workload_volume_spec_host_path() {
        let spec = WorkloadVolumeSpec::host_path("logs", "/var/log");

        assert_eq!(spec.name, "logs");
        if let VolumeSource::HostPath(config) = spec.source {
            assert_eq!(config.path, PathBuf::from("/var/log"));
        } else {
            panic!("Expected HostPath source");
        }
    }

    #[test]
    fn workload_volume_spec_nfs() {
        let spec = WorkloadVolumeSpec::nfs("shared", "nfs.example.com", "/exports/data");

        assert_eq!(spec.name, "shared");
        if let VolumeSource::Nfs(config) = spec.source {
            assert_eq!(config.server, "nfs.example.com");
            assert_eq!(config.path, "/exports/data");
        } else {
            panic!("Expected Nfs source");
        }
    }

    #[test]
    fn workload_volume_spec_s3() {
        let spec = WorkloadVolumeSpec::s3("artifacts", "my-bucket");

        assert_eq!(spec.name, "artifacts");
        if let VolumeSource::S3(config) = spec.source {
            assert_eq!(config.bucket, "my-bucket");
        } else {
            panic!("Expected S3 source");
        }
    }

    #[test]
    fn workload_volume_spec_dynamic_claim() {
        let spec = WorkloadVolumeSpec::dynamic_claim("data", 10 * 1024 * 1024 * 1024, AccessMode::ReadWriteOnce)
            .with_storage_class("fast");

        assert_eq!(spec.name, "data");
        if let VolumeSource::DynamicClaim {
            capacity,
            access_mode,
            storage_class,
        } = spec.source
        {
            assert_eq!(capacity, 10 * 1024 * 1024 * 1024);
            assert_eq!(access_mode, AccessMode::ReadWriteOnce);
            assert_eq!(storage_class, Some("fast".to_string()));
        } else {
            panic!("Expected DynamicClaim source");
        }
    }

    // ===================
    // ContainerVolumeMount Tests
    // ===================

    #[test]
    fn container_volume_mount_new() {
        let mount = ContainerVolumeMount::new("data", "/mnt/data");

        assert_eq!(mount.name, "data");
        assert_eq!(mount.mount_path, PathBuf::from("/mnt/data"));
        assert!(!mount.read_only);
        assert!(mount.sub_path.is_none());
    }

    #[test]
    fn container_volume_mount_with_options() {
        let mount = ContainerVolumeMount::new("data", "/mnt/data")
            .with_sub_path("subdir")
            .read_only();

        assert!(mount.read_only);
        assert_eq!(mount.sub_path, Some("subdir".to_string()));
    }

    // ===================
    // WorkloadVolumeManager Tests
    // ===================

    #[test]
    fn resolve_existing_volume() {
        let manager = setup_manager();

        // Create a volume
        let vol_id = VolumeId::new("test-vol").expect("valid id");
        let volume = Volume::new(vol_id.clone(), VolumeType::empty_dir(), 1024);
        manager.provision_available(volume).expect("provision");

        // Create a claim and bind
        let claim = VolumeClaim::new("test-claim", 512);
        manager.create_claim(claim).expect("create claim");
        manager.bind(&vol_id, "test-claim").expect("bind");

        // Resolve
        let wvm = WorkloadVolumeManager::new(&manager, "workload-1");
        let specs = vec![WorkloadVolumeSpec::existing("data", vol_id.clone())];
        let mounts = vec![ContainerVolumeMount::new("data", "/data")];

        let resolved = wvm.resolve(&specs, &mounts).expect("resolve");

        assert_eq!(resolved.volume_map.len(), 1);
        assert_eq!(resolved.volume_map.get("data"), Some(&vol_id));
        assert_eq!(resolved.mounts.len(), 1);
    }

    #[test]
    fn resolve_empty_dir_volume() {
        let manager = setup_manager();

        let wvm = WorkloadVolumeManager::new(&manager, "workload-1");
        let specs = vec![WorkloadVolumeSpec::empty_dir("cache")];
        let mounts = vec![ContainerVolumeMount::new("cache", "/cache")];

        let resolved = wvm.resolve(&specs, &mounts).expect("resolve");

        assert_eq!(resolved.volume_map.len(), 1);
        assert!(resolved.volume_map.contains_key("cache"));
        assert_eq!(resolved.created_volumes.len(), 1);

        // Verify volume was created
        let vol_id = resolved.volume_map.get("cache").expect("cache volume");
        let volume = manager.get_volume(vol_id).expect("volume exists");
        assert!(matches!(volume.volume_type, VolumeType::EmptyDir(_)));
    }

    #[test]
    fn resolve_from_claim() {
        let manager = setup_manager();

        // Create a volume
        let vol_id = VolumeId::new("test-vol").expect("valid id");
        let volume = Volume::new(vol_id.clone(), VolumeType::empty_dir(), 10 * 1024 * 1024 * 1024);
        manager.provision_available(volume).expect("provision");

        // Create and bind a claim
        let claim = VolumeClaim::new("my-claim", 5 * 1024 * 1024 * 1024);
        manager.create_claim(claim).expect("create claim");
        manager.bind(&vol_id, "my-claim").expect("bind");

        // Resolve with claim reference
        let wvm = WorkloadVolumeManager::new(&manager, "workload-1");
        let specs = vec![WorkloadVolumeSpec::from_claim("data", "my-claim")];
        let mounts = vec![ContainerVolumeMount::new("data", "/data")];

        let resolved = wvm.resolve(&specs, &mounts).expect("resolve");

        assert_eq!(resolved.volume_map.get("data"), Some(&vol_id));
    }

    #[test]
    fn resolve_claim_not_bound() {
        let manager = setup_manager();

        // Create a claim but don't bind it
        let claim = VolumeClaim::new("my-claim", 5 * 1024 * 1024 * 1024);
        manager.create_claim(claim).expect("create claim");

        let wvm = WorkloadVolumeManager::new(&manager, "workload-1");
        let specs = vec![WorkloadVolumeSpec::from_claim("data", "my-claim")];
        let mounts = vec![];

        let result = wvm.resolve(&specs, &mounts);
        assert!(matches!(result, Err(Error::NoMatchingVolume { .. })));
    }

    #[test]
    fn attach_and_detach_volumes() {
        let manager = setup_manager();

        // Create and resolve an empty dir volume
        let wvm = WorkloadVolumeManager::new(&manager, "workload-1");
        let specs = vec![WorkloadVolumeSpec::empty_dir("cache")];
        let mounts = vec![ContainerVolumeMount::new("cache", "/cache")];

        let resolved = wvm.resolve(&specs, &mounts).expect("resolve");

        // Attach
        wvm.attach_volumes(&resolved).expect("attach");

        let vol_id = resolved.volume_map.get("cache").expect("volume");
        let volume = manager.get_volume(vol_id).expect("volume exists");
        assert!(volume.is_attached());
        assert_eq!(volume.attached_to, Some("workload-1".to_string()));

        // Detach
        wvm.detach_volumes(&resolved).expect("detach");

        let volume = manager.get_volume(vol_id).expect("volume exists");
        assert!(!volume.is_attached());
    }

    #[test]
    fn cleanup_ephemeral_volumes() {
        let manager = setup_manager();

        let wvm = WorkloadVolumeManager::new(&manager, "workload-1");
        let specs = vec![
            WorkloadVolumeSpec::empty_dir("cache1"),
            WorkloadVolumeSpec::empty_dir("cache2"),
        ];
        let mounts = vec![
            ContainerVolumeMount::new("cache1", "/cache1"),
            ContainerVolumeMount::new("cache2", "/cache2"),
        ];

        let resolved = wvm.resolve(&specs, &mounts).expect("resolve");

        // Attach
        wvm.attach_volumes(&resolved).expect("attach");
        assert_eq!(manager.volume_count(), 2);

        // Cleanup
        wvm.cleanup(&resolved).expect("cleanup");

        // Volumes should be deleted
        assert_eq!(manager.volume_count(), 0);
    }

    #[test]
    fn resolve_mount_validation() {
        let manager = setup_manager();

        let wvm = WorkloadVolumeManager::new(&manager, "workload-1");
        let specs = vec![WorkloadVolumeSpec::empty_dir("data")];

        // Invalid mount path (relative)
        let mounts = vec![ContainerVolumeMount::new("data", "relative/path")];

        let result = wvm.resolve(&specs, &mounts);
        assert!(matches!(result, Err(Error::InvalidMountPath { .. })));
    }

    #[test]
    fn resolve_mount_not_found() {
        let manager = setup_manager();

        let wvm = WorkloadVolumeManager::new(&manager, "workload-1");
        let specs = vec![WorkloadVolumeSpec::empty_dir("cache")];

        // Mount references non-existent volume
        let mounts = vec![ContainerVolumeMount::new("nonexistent", "/data")];

        let result = wvm.resolve(&specs, &mounts);
        assert!(matches!(result, Err(Error::VolumeNotFound { .. })));
    }

    #[test]
    fn resolve_dynamic_claim_with_matching_volume() {
        let manager = setup_manager();

        // Create an available volume
        let vol_id = VolumeId::new("available-vol").expect("valid id");
        let volume = Volume::new(
            vol_id.clone(),
            VolumeType::empty_dir(),
            20 * 1024 * 1024 * 1024,
        )
        .with_access_mode(AccessMode::ReadWriteMany);
        manager.provision_available(volume).expect("provision");

        // Resolve with dynamic claim
        let wvm = WorkloadVolumeManager::new(&manager, "workload-1");
        let specs = vec![WorkloadVolumeSpec::dynamic_claim(
            "data",
            10 * 1024 * 1024 * 1024,
            AccessMode::ReadWriteOnce,
        )];
        let mounts = vec![ContainerVolumeMount::new("data", "/data")];

        let resolved = wvm.resolve(&specs, &mounts).expect("resolve");

        // Should have bound to the available volume
        assert_eq!(resolved.volume_map.get("data"), Some(&vol_id));
        assert_eq!(resolved.created_claims.len(), 1);

        // Verify binding
        let claim = manager
            .get_claim(&resolved.created_claims[0])
            .expect("claim");
        assert!(claim.is_bound());
    }

    #[test]
    fn resolve_dynamic_claim_no_matching_volume() {
        let manager = setup_manager();

        let wvm = WorkloadVolumeManager::new(&manager, "workload-1");
        let specs = vec![WorkloadVolumeSpec::dynamic_claim(
            "data",
            10 * 1024 * 1024 * 1024,
            AccessMode::ReadWriteOnce,
        )];
        let mounts = vec![];

        let result = wvm.resolve(&specs, &mounts);
        assert!(matches!(result, Err(Error::NoMatchingVolume { .. })));
    }

    #[test]
    fn resolve_host_path() {
        let manager = setup_manager();

        let wvm = WorkloadVolumeManager::new(&manager, "workload-1");
        let specs = vec![WorkloadVolumeSpec::host_path("logs", "/var/log")];
        let mounts = vec![ContainerVolumeMount::new("logs", "/container/logs")];

        let resolved = wvm.resolve(&specs, &mounts).expect("resolve");

        assert!(resolved.volume_map.contains_key("logs"));
        assert_eq!(resolved.created_volumes.len(), 1);

        let vol_id = resolved.volume_map.get("logs").expect("logs volume");
        let volume = manager.get_volume(vol_id).expect("volume exists");
        assert!(matches!(volume.volume_type, VolumeType::HostPath(_)));
    }

    #[test]
    fn resolve_nfs() {
        let manager = setup_manager();

        let wvm = WorkloadVolumeManager::new(&manager, "workload-1");
        let specs = vec![WorkloadVolumeSpec::nfs(
            "shared",
            "nfs.example.com",
            "/exports/data",
        )];
        let mounts = vec![ContainerVolumeMount::new("shared", "/shared")];

        let resolved = wvm.resolve(&specs, &mounts).expect("resolve");

        let vol_id = resolved.volume_map.get("shared").expect("shared volume");
        let volume = manager.get_volume(vol_id).expect("volume exists");
        assert!(matches!(volume.volume_type, VolumeType::Nfs(_)));
    }

    #[test]
    fn resolve_s3() {
        let manager = setup_manager();

        let wvm = WorkloadVolumeManager::new(&manager, "workload-1");
        let specs = vec![WorkloadVolumeSpec::s3("artifacts", "my-bucket")];
        let mounts = vec![ContainerVolumeMount::new("artifacts", "/artifacts")];

        let resolved = wvm.resolve(&specs, &mounts).expect("resolve");

        let vol_id = resolved.volume_map.get("artifacts").expect("artifacts volume");
        let volume = manager.get_volume(vol_id).expect("volume exists");
        assert!(matches!(volume.volume_type, VolumeType::S3(_)));
    }
}
