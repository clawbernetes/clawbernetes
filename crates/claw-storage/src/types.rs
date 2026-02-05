//! Core types for the storage module.
//!
//! This module defines the fundamental types used throughout the storage system:
//! - [`VolumeId`]: A validated identifier for volumes
//! - [`Volume`]: A storage volume with type, capacity, and access mode
//! - [`VolumeType`]: The type of storage backend (`HostPath`, NFS, S3, `EmptyDir`)
//! - [`VolumeMount`]: How a volume is mounted into a container
//! - [`VolumeClaim`]: A request for storage resources

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use crate::error::{Error, Result};

/// Maximum length of a volume identifier.
pub const VOLUME_ID_MAX_LENGTH: usize = 253;

/// Minimum length of a volume identifier.
pub const VOLUME_ID_MIN_LENGTH: usize = 1;

/// A validated identifier for a volume.
///
/// Volume IDs must:
/// - Be between 1 and 253 characters
/// - Contain only lowercase alphanumeric characters, hyphens, and underscores
/// - Start with an alphanumeric character
/// - Not end with a hyphen
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct VolumeId(String);

impl VolumeId {
    /// Creates a new `VolumeId` after validating the input.
    ///
    /// # Errors
    ///
    /// Returns an error if the identifier is invalid.
    pub fn new(id: impl Into<String>) -> Result<Self> {
        let id = id.into();
        Self::validate(&id)?;
        Ok(Self(id))
    }

    /// Returns the identifier as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Validates a volume identifier string.
    fn validate(id: &str) -> Result<()> {
        if id.len() < VOLUME_ID_MIN_LENGTH {
            return Err(Error::InvalidVolumeId {
                reason: "identifier cannot be empty".to_string(),
            });
        }

        if id.len() > VOLUME_ID_MAX_LENGTH {
            return Err(Error::InvalidVolumeId {
                reason: format!(
                    "identifier exceeds maximum length of {VOLUME_ID_MAX_LENGTH} characters"
                ),
            });
        }

        let first_char = id.chars().next().ok_or_else(|| Error::InvalidVolumeId {
            reason: "identifier cannot be empty".to_string(),
        })?;

        if !first_char.is_ascii_alphanumeric() {
            return Err(Error::InvalidVolumeId {
                reason: "identifier must start with an alphanumeric character".to_string(),
            });
        }

        let last_char = id.chars().last().ok_or_else(|| Error::InvalidVolumeId {
            reason: "identifier cannot be empty".to_string(),
        })?;

        if last_char == '-' {
            return Err(Error::InvalidVolumeId {
                reason: "identifier cannot end with a hyphen".to_string(),
            });
        }

        for c in id.chars() {
            if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '-' && c != '_' {
                return Err(Error::InvalidVolumeId {
                    reason: format!(
                        "identifier contains invalid character '{c}'; only lowercase alphanumeric, hyphens, and underscores are allowed"
                    ),
                });
            }
        }

        Ok(())
    }
}

impl fmt::Display for VolumeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for VolumeId {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<VolumeId> for String {
    fn from(id: VolumeId) -> Self {
        id.0
    }
}

impl AsRef<str> for VolumeId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Access mode for a volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AccessMode {
    /// The volume can be mounted as read-write by a single node.
    #[default]
    ReadWriteOnce,

    /// The volume can be mounted as read-only by many nodes.
    ReadOnlyMany,

    /// The volume can be mounted as read-write by many nodes.
    ReadWriteMany,

    /// The volume can be mounted as read-write by a single pod only.
    ReadWriteOncePod,
}

impl AccessMode {
    /// Check if this access mode allows multiple readers.
    #[must_use]
    pub const fn allows_multiple_readers(&self) -> bool {
        matches!(self, Self::ReadOnlyMany | Self::ReadWriteMany)
    }

    /// Check if this access mode allows writing.
    #[must_use]
    pub const fn allows_write(&self) -> bool {
        matches!(
            self,
            Self::ReadWriteOnce | Self::ReadWriteMany | Self::ReadWriteOncePod
        )
    }

    /// Check if the given access mode is compatible with this mode.
    #[must_use]
    pub fn is_compatible_with(&self, requested: &Self) -> bool {
        match self {
            Self::ReadWriteMany => true, // Supports all modes
            Self::ReadOnlyMany => !requested.allows_write(),
            Self::ReadWriteOnce | Self::ReadWriteOncePod => self == requested,
        }
    }
}

impl fmt::Display for AccessMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadWriteOnce => write!(f, "ReadWriteOnce"),
            Self::ReadOnlyMany => write!(f, "ReadOnlyMany"),
            Self::ReadWriteMany => write!(f, "ReadWriteMany"),
            Self::ReadWriteOncePod => write!(f, "ReadWriteOncePod"),
        }
    }
}

/// Status of a volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum VolumeStatus {
    /// The volume is being provisioned.
    #[default]
    Pending,

    /// The volume is available for use.
    Available,

    /// The volume is bound to a claim.
    Bound,

    /// The volume is currently attached to a workload.
    Attached,

    /// The volume is being released.
    Releasing,

    /// The volume has failed.
    Failed,

    /// The volume is being deleted.
    Deleting,
}

impl fmt::Display for VolumeStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "Pending"),
            Self::Available => write!(f, "Available"),
            Self::Bound => write!(f, "Bound"),
            Self::Attached => write!(f, "Attached"),
            Self::Releasing => write!(f, "Releasing"),
            Self::Failed => write!(f, "Failed"),
            Self::Deleting => write!(f, "Deleting"),
        }
    }
}

/// Host path type for volume mounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HostPathType {
    /// No checks are performed.
    #[default]
    Unset,

    /// Path must exist as a directory.
    Directory,

    /// Path must exist as a file.
    File,

    /// Path must exist as a socket.
    Socket,

    /// Path must exist as a character device.
    CharDevice,

    /// Path must exist as a block device.
    BlockDevice,

    /// Path must exist as a directory; create if missing.
    DirectoryOrCreate,

    /// Path must exist as a file; create if missing.
    FileOrCreate,
}

/// Configuration for a host path volume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostPathConfig {
    /// The path on the host.
    pub path: PathBuf,

    /// The type of the host path.
    pub host_path_type: HostPathType,
}

impl HostPathConfig {
    /// Create a new host path configuration.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            host_path_type: HostPathType::Unset,
        }
    }

    /// Set the host path type.
    #[must_use]
    pub fn with_type(mut self, host_path_type: HostPathType) -> Self {
        self.host_path_type = host_path_type;
        self
    }

    /// Validate the host path configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn validate(&self) -> Result<()> {
        if self.path.as_os_str().is_empty() {
            return Err(Error::InvalidMountPath {
                reason: "host path cannot be empty".to_string(),
            });
        }

        if !self.path.is_absolute() {
            return Err(Error::InvalidMountPath {
                reason: "host path must be absolute".to_string(),
            });
        }

        Ok(())
    }
}

/// Configuration for an NFS volume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NfsConfig {
    /// The NFS server address.
    pub server: String,

    /// The path on the NFS server.
    pub path: String,

    /// Whether to mount read-only.
    pub read_only: bool,

    /// NFS mount options.
    pub mount_options: Vec<String>,
}

impl NfsConfig {
    /// Create a new NFS configuration.
    #[must_use]
    pub fn new(server: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            server: server.into(),
            path: path.into(),
            read_only: false,
            mount_options: Vec::new(),
        }
    }

    /// Set read-only mode.
    #[must_use]
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    /// Add mount options.
    #[must_use]
    pub fn with_options(mut self, options: Vec<String>) -> Self {
        self.mount_options = options;
        self
    }

    /// Validate the NFS configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn validate(&self) -> Result<()> {
        if self.server.is_empty() {
            return Err(Error::InvalidNfsConfig {
                reason: "server address cannot be empty".to_string(),
            });
        }

        if self.path.is_empty() {
            return Err(Error::InvalidNfsConfig {
                reason: "export path cannot be empty".to_string(),
            });
        }

        if !self.path.starts_with('/') {
            return Err(Error::InvalidNfsConfig {
                reason: "export path must be absolute".to_string(),
            });
        }

        Ok(())
    }
}

/// Configuration for an S3-compatible volume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct S3Config {
    /// The bucket name.
    pub bucket: String,

    /// The endpoint URL (for S3-compatible services).
    pub endpoint: Option<String>,

    /// The region.
    pub region: Option<String>,

    /// The prefix/path within the bucket.
    pub prefix: Option<String>,

    /// Secret name containing credentials.
    pub secret_name: Option<String>,

    /// Whether to use path-style access.
    pub path_style: bool,
}

impl S3Config {
    /// Create a new S3 configuration.
    #[must_use]
    pub fn new(bucket: impl Into<String>) -> Self {
        Self {
            bucket: bucket.into(),
            endpoint: None,
            region: None,
            prefix: None,
            secret_name: None,
            path_style: false,
        }
    }

    /// Set the endpoint URL.
    #[must_use]
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Set the region.
    #[must_use]
    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }

    /// Set the prefix.
    #[must_use]
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Set the secret name for credentials.
    #[must_use]
    pub fn with_secret(mut self, secret_name: impl Into<String>) -> Self {
        self.secret_name = Some(secret_name.into());
        self
    }

    /// Enable path-style access.
    #[must_use]
    pub fn with_path_style(mut self) -> Self {
        self.path_style = true;
        self
    }

    /// Validate the S3 configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn validate(&self) -> Result<()> {
        if self.bucket.is_empty() {
            return Err(Error::InvalidS3Config {
                reason: "bucket name cannot be empty".to_string(),
            });
        }

        // Validate bucket name format (simplified S3 bucket naming rules)
        if self.bucket.len() < 3 || self.bucket.len() > 63 {
            return Err(Error::InvalidS3Config {
                reason: "bucket name must be between 3 and 63 characters".to_string(),
            });
        }

        Ok(())
    }
}

/// Configuration for an empty directory volume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EmptyDirConfig {
    /// The medium to use (empty string = default, "Memory" = tmpfs).
    pub medium: EmptyDirMedium,

    /// Size limit in bytes (0 = no limit).
    pub size_limit: u64,
}

/// Medium for empty directory volumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EmptyDirMedium {
    /// Use the default medium (disk).
    #[default]
    Default,

    /// Use memory-backed tmpfs.
    Memory,
}

impl EmptyDirConfig {
    /// Create a new empty directory configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Use memory-backed storage (tmpfs).
    #[must_use]
    pub fn memory(mut self) -> Self {
        self.medium = EmptyDirMedium::Memory;
        self
    }

    /// Set the size limit in bytes.
    #[must_use]
    pub fn with_size_limit(mut self, bytes: u64) -> Self {
        self.size_limit = bytes;
        self
    }

    /// Set the size limit in megabytes.
    #[must_use]
    pub fn with_size_limit_mb(mut self, mb: u64) -> Self {
        self.size_limit = mb * 1024 * 1024;
        self
    }

    /// Set the size limit in gigabytes.
    #[must_use]
    pub fn with_size_limit_gb(mut self, gb: u64) -> Self {
        self.size_limit = gb * 1024 * 1024 * 1024;
        self
    }
}

/// The type of volume and its configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VolumeType {
    /// A path on the host node.
    HostPath(HostPathConfig),

    /// An NFS mount.
    Nfs(NfsConfig),

    /// An S3-compatible object storage mount.
    S3(S3Config),

    /// An empty directory (ephemeral storage).
    EmptyDir(EmptyDirConfig),
}

impl VolumeType {
    /// Create a host path volume type.
    #[must_use]
    pub fn host_path(path: impl Into<PathBuf>) -> Self {
        Self::HostPath(HostPathConfig::new(path))
    }

    /// Create an NFS volume type.
    #[must_use]
    pub fn nfs(server: impl Into<String>, path: impl Into<String>) -> Self {
        Self::Nfs(NfsConfig::new(server, path))
    }

    /// Create an S3 volume type.
    #[must_use]
    pub fn s3(bucket: impl Into<String>) -> Self {
        Self::S3(S3Config::new(bucket))
    }

    /// Create an empty directory volume type.
    #[must_use]
    pub fn empty_dir() -> Self {
        Self::EmptyDir(EmptyDirConfig::new())
    }

    /// Validate the volume type configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::HostPath(config) => config.validate(),
            Self::Nfs(config) => config.validate(),
            Self::S3(config) => config.validate(),
            Self::EmptyDir(_) => Ok(()),
        }
    }

    /// Get a human-readable name for the volume type.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::HostPath(_) => "HostPath",
            Self::Nfs(_) => "NFS",
            Self::S3(_) => "S3",
            Self::EmptyDir(_) => "EmptyDir",
        }
    }
}

/// A storage volume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Volume {
    /// Unique identifier for the volume.
    pub id: VolumeId,

    /// The type and configuration of the volume.
    pub volume_type: VolumeType,

    /// Capacity in bytes.
    pub capacity: u64,

    /// Access mode for the volume.
    pub access_mode: AccessMode,

    /// Current status of the volume.
    pub status: VolumeStatus,

    /// Storage class name.
    pub storage_class: Option<String>,

    /// When the volume was created.
    pub created_at: DateTime<Utc>,

    /// When the volume was last updated.
    pub updated_at: DateTime<Utc>,

    /// Labels for the volume.
    pub labels: HashMap<String, String>,

    /// Annotations for the volume.
    pub annotations: HashMap<String, String>,

    /// The claim this volume is bound to (if any).
    pub bound_claim: Option<String>,

    /// The workload this volume is attached to (if any).
    pub attached_to: Option<String>,

    /// Error message if the volume is in a failed state.
    pub error_message: Option<String>,
}

impl Volume {
    /// Create a new volume.
    #[must_use]
    pub fn new(id: VolumeId, volume_type: VolumeType, capacity: u64) -> Self {
        let now = Utc::now();
        Self {
            id,
            volume_type,
            capacity,
            access_mode: AccessMode::default(),
            status: VolumeStatus::default(),
            storage_class: None,
            created_at: now,
            updated_at: now,
            labels: HashMap::new(),
            annotations: HashMap::new(),
            bound_claim: None,
            attached_to: None,
            error_message: None,
        }
    }

    /// Set the access mode.
    #[must_use]
    pub fn with_access_mode(mut self, mode: AccessMode) -> Self {
        self.access_mode = mode;
        self
    }

    /// Set the storage class.
    #[must_use]
    pub fn with_storage_class(mut self, class: impl Into<String>) -> Self {
        self.storage_class = Some(class.into());
        self
    }

    /// Add a label.
    #[must_use]
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Add an annotation.
    #[must_use]
    pub fn with_annotation(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.annotations.insert(key.into(), value.into());
        self
    }

    /// Set the status.
    pub fn set_status(&mut self, status: VolumeStatus) {
        self.status = status;
        self.updated_at = Utc::now();
    }

    /// Check if the volume is available.
    #[must_use]
    pub fn is_available(&self) -> bool {
        self.status == VolumeStatus::Available
    }

    /// Check if the volume is bound.
    #[must_use]
    pub fn is_bound(&self) -> bool {
        self.status == VolumeStatus::Bound || self.bound_claim.is_some()
    }

    /// Check if the volume is attached.
    #[must_use]
    pub fn is_attached(&self) -> bool {
        self.status == VolumeStatus::Attached || self.attached_to.is_some()
    }

    /// Validate the volume configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn validate(&self) -> Result<()> {
        self.volume_type.validate()
    }
}

/// How a volume is mounted into a container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeMount {
    /// The volume ID to mount.
    pub volume_id: VolumeId,

    /// The path inside the container where the volume should be mounted.
    pub mount_path: PathBuf,

    /// Whether the mount should be read-only.
    pub read_only: bool,

    /// Optional sub-path within the volume to mount.
    pub sub_path: Option<String>,

    /// Mount propagation mode.
    pub propagation: MountPropagation,
}

/// Mount propagation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MountPropagation {
    /// No propagation.
    #[default]
    None,

    /// Host-to-container propagation.
    HostToContainer,

    /// Bidirectional propagation.
    Bidirectional,
}

impl VolumeMount {
    /// Create a new volume mount.
    #[must_use]
    pub fn new(volume_id: VolumeId, mount_path: impl Into<PathBuf>) -> Self {
        Self {
            volume_id,
            mount_path: mount_path.into(),
            read_only: false,
            sub_path: None,
            propagation: MountPropagation::default(),
        }
    }

    /// Make the mount read-only.
    #[must_use]
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    /// Set a sub-path within the volume.
    #[must_use]
    pub fn with_sub_path(mut self, sub_path: impl Into<String>) -> Self {
        self.sub_path = Some(sub_path.into());
        self
    }

    /// Set the mount propagation mode.
    #[must_use]
    pub fn with_propagation(mut self, propagation: MountPropagation) -> Self {
        self.propagation = propagation;
        self
    }

    /// Validate the mount configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn validate(&self) -> Result<()> {
        if self.mount_path.as_os_str().is_empty() {
            return Err(Error::InvalidMountPath {
                reason: "mount path cannot be empty".to_string(),
            });
        }

        if !self.mount_path.is_absolute() {
            return Err(Error::InvalidMountPath {
                reason: "mount path must be absolute".to_string(),
            });
        }

        Ok(())
    }
}

/// Status of a volume claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ClaimStatus {
    /// The claim is waiting for a volume.
    #[default]
    Pending,

    /// The claim is bound to a volume.
    Bound,

    /// The claim has lost its binding.
    Lost,
}

impl fmt::Display for ClaimStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "Pending"),
            Self::Bound => write!(f, "Bound"),
            Self::Lost => write!(f, "Lost"),
        }
    }
}

/// A request for storage resources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeClaim {
    /// Unique identifier for the claim.
    pub id: String,

    /// Requested capacity in bytes.
    pub requested_capacity: u64,

    /// Requested access mode.
    pub access_mode: AccessMode,

    /// Storage class to use.
    pub storage_class: Option<String>,

    /// Volume selector labels.
    pub selector: HashMap<String, String>,

    /// Current status of the claim.
    pub status: ClaimStatus,

    /// The volume this claim is bound to (if any).
    pub bound_volume: Option<VolumeId>,

    /// When the claim was created.
    pub created_at: DateTime<Utc>,

    /// The workload that owns this claim.
    pub owner: Option<String>,
}

impl VolumeClaim {
    /// Create a new volume claim.
    #[must_use]
    pub fn new(id: impl Into<String>, requested_capacity: u64) -> Self {
        Self {
            id: id.into(),
            requested_capacity,
            access_mode: AccessMode::default(),
            storage_class: None,
            selector: HashMap::new(),
            status: ClaimStatus::default(),
            bound_volume: None,
            created_at: Utc::now(),
            owner: None,
        }
    }

    /// Set the access mode.
    #[must_use]
    pub fn with_access_mode(mut self, mode: AccessMode) -> Self {
        self.access_mode = mode;
        self
    }

    /// Set the storage class.
    #[must_use]
    pub fn with_storage_class(mut self, class: impl Into<String>) -> Self {
        self.storage_class = Some(class.into());
        self
    }

    /// Add a selector label.
    #[must_use]
    pub fn with_selector(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.selector.insert(key.into(), value.into());
        self
    }

    /// Set the owner.
    #[must_use]
    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.owner = Some(owner.into());
        self
    }

    /// Check if this claim matches a volume.
    #[must_use]
    pub fn matches_volume(&self, volume: &Volume) -> bool {
        // Check capacity
        if volume.capacity < self.requested_capacity {
            return false;
        }

        // Check access mode compatibility
        if !volume.access_mode.is_compatible_with(&self.access_mode) {
            return false;
        }

        // Check storage class
        if let Some(ref class) = self.storage_class {
            if volume.storage_class.as_ref() != Some(class) {
                return false;
            }
        }

        // Check selector labels
        for (key, value) in &self.selector {
            if volume.labels.get(key) != Some(value) {
                return false;
            }
        }

        true
    }

    /// Check if the claim is bound.
    #[must_use]
    pub fn is_bound(&self) -> bool {
        self.status == ClaimStatus::Bound && self.bound_volume.is_some()
    }
}

/// Storage class definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageClass {
    /// Name of the storage class.
    pub name: String,

    /// Provisioner responsible for creating volumes.
    pub provisioner: String,

    /// Parameters for the provisioner.
    pub parameters: HashMap<String, String>,

    /// Reclaim policy for volumes.
    pub reclaim_policy: ReclaimPolicy,

    /// Whether to allow volume expansion.
    pub allow_volume_expansion: bool,

    /// Volume binding mode.
    pub volume_binding_mode: VolumeBindingMode,

    /// Whether this is the default storage class.
    pub is_default: bool,
}

/// What to do with a volume when its claim is deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ReclaimPolicy {
    /// Retain the volume (manual cleanup required).
    #[default]
    Retain,

    /// Delete the volume.
    Delete,

    /// Recycle the volume (delete contents, keep volume).
    Recycle,
}

/// When to bind volumes to claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VolumeBindingMode {
    /// Bind immediately when claim is created.
    #[default]
    Immediate,

    /// Wait until a pod using the claim is scheduled.
    WaitForFirstConsumer,
}

impl StorageClass {
    /// Create a new storage class.
    #[must_use]
    pub fn new(name: impl Into<String>, provisioner: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            provisioner: provisioner.into(),
            parameters: HashMap::new(),
            reclaim_policy: ReclaimPolicy::default(),
            allow_volume_expansion: false,
            volume_binding_mode: VolumeBindingMode::default(),
            is_default: false,
        }
    }

    /// Add a parameter.
    #[must_use]
    pub fn with_parameter(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.parameters.insert(key.into(), value.into());
        self
    }

    /// Set the reclaim policy.
    #[must_use]
    pub fn with_reclaim_policy(mut self, policy: ReclaimPolicy) -> Self {
        self.reclaim_policy = policy;
        self
    }

    /// Allow volume expansion.
    #[must_use]
    pub fn with_expansion(mut self) -> Self {
        self.allow_volume_expansion = true;
        self
    }

    /// Set the binding mode.
    #[must_use]
    pub fn with_binding_mode(mut self, mode: VolumeBindingMode) -> Self {
        self.volume_binding_mode = mode;
        self
    }

    /// Mark as default storage class.
    #[must_use]
    pub fn as_default(mut self) -> Self {
        self.is_default = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    // ===================
    // VolumeId Tests
    // ===================

    #[test]
    fn volume_id_valid_simple() {
        let id = VolumeId::new("my-volume").expect("should be valid");
        assert_eq!(id.as_str(), "my-volume");
    }

    #[test]
    fn volume_id_valid_with_underscores() {
        let id = VolumeId::new("my_volume_data").expect("should be valid");
        assert_eq!(id.as_str(), "my_volume_data");
    }

    #[test]
    fn volume_id_valid_single_char() {
        let id = VolumeId::new("v").expect("should be valid");
        assert_eq!(id.as_str(), "v");
    }

    #[test]
    fn volume_id_valid_numbers() {
        let id = VolumeId::new("volume123").expect("should be valid");
        assert_eq!(id.as_str(), "volume123");
    }

    #[test_case("" ; "empty string")]
    #[test_case("-volume" ; "starts with hyphen")]
    #[test_case("_volume" ; "starts with underscore")]
    #[test_case("volume-" ; "ends with hyphen")]
    #[test_case("Volume" ; "contains uppercase")]
    #[test_case("my volume" ; "contains space")]
    #[test_case("my.volume" ; "contains dot")]
    #[test_case("my/volume" ; "contains slash")]
    fn volume_id_invalid(input: &str) {
        let result = VolumeId::new(input);
        assert!(result.is_err(), "expected '{}' to be invalid", input);
    }

    #[test]
    fn volume_id_max_length() {
        let long_id = "a".repeat(VOLUME_ID_MAX_LENGTH);
        let id = VolumeId::new(&long_id).expect("max length should be valid");
        assert_eq!(id.as_str().len(), VOLUME_ID_MAX_LENGTH);
    }

    #[test]
    fn volume_id_exceeds_max_length() {
        let too_long = "a".repeat(VOLUME_ID_MAX_LENGTH + 1);
        let result = VolumeId::new(&too_long);
        assert!(result.is_err());
    }

    #[test]
    fn volume_id_display() {
        let id = VolumeId::new("my-volume").expect("should be valid");
        assert_eq!(format!("{id}"), "my-volume");
    }

    #[test]
    fn volume_id_serde_roundtrip() {
        let original = VolumeId::new("my-volume").expect("should be valid");
        let json = serde_json::to_string(&original).expect("serialize");
        let restored: VolumeId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, restored);
    }

    // ===================
    // AccessMode Tests
    // ===================

    #[test]
    fn access_mode_default() {
        assert_eq!(AccessMode::default(), AccessMode::ReadWriteOnce);
    }

    #[test]
    fn access_mode_allows_write() {
        assert!(AccessMode::ReadWriteOnce.allows_write());
        assert!(AccessMode::ReadWriteMany.allows_write());
        assert!(AccessMode::ReadWriteOncePod.allows_write());
        assert!(!AccessMode::ReadOnlyMany.allows_write());
    }

    #[test]
    fn access_mode_allows_multiple_readers() {
        assert!(AccessMode::ReadOnlyMany.allows_multiple_readers());
        assert!(AccessMode::ReadWriteMany.allows_multiple_readers());
        assert!(!AccessMode::ReadWriteOnce.allows_multiple_readers());
        assert!(!AccessMode::ReadWriteOncePod.allows_multiple_readers());
    }

    #[test]
    fn access_mode_compatibility() {
        // ReadWriteMany supports all modes
        assert!(AccessMode::ReadWriteMany.is_compatible_with(&AccessMode::ReadWriteOnce));
        assert!(AccessMode::ReadWriteMany.is_compatible_with(&AccessMode::ReadOnlyMany));

        // ReadOnlyMany only supports read modes
        assert!(AccessMode::ReadOnlyMany.is_compatible_with(&AccessMode::ReadOnlyMany));
        assert!(!AccessMode::ReadOnlyMany.is_compatible_with(&AccessMode::ReadWriteOnce));

        // ReadWriteOnce only supports exact match
        assert!(AccessMode::ReadWriteOnce.is_compatible_with(&AccessMode::ReadWriteOnce));
        assert!(!AccessMode::ReadWriteOnce.is_compatible_with(&AccessMode::ReadWriteMany));
    }

    // ===================
    // VolumeType Tests
    // ===================

    #[test]
    fn volume_type_host_path() {
        let vt = VolumeType::host_path("/data");
        assert_eq!(vt.type_name(), "HostPath");
    }

    #[test]
    fn volume_type_nfs() {
        let vt = VolumeType::nfs("nfs.example.com", "/exports/data");
        assert_eq!(vt.type_name(), "NFS");
    }

    #[test]
    fn volume_type_s3() {
        let vt = VolumeType::s3("my-bucket");
        assert_eq!(vt.type_name(), "S3");
    }

    #[test]
    fn volume_type_empty_dir() {
        let vt = VolumeType::empty_dir();
        assert_eq!(vt.type_name(), "EmptyDir");
    }

    #[test]
    fn host_path_config_validate_valid() {
        let config = HostPathConfig::new("/data");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn host_path_config_validate_empty() {
        let config = HostPathConfig::new("");
        assert!(config.validate().is_err());
    }

    #[test]
    fn host_path_config_validate_relative() {
        let config = HostPathConfig::new("relative/path");
        assert!(config.validate().is_err());
    }

    #[test]
    fn nfs_config_validate_valid() {
        let config = NfsConfig::new("nfs.example.com", "/exports/data");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn nfs_config_validate_empty_server() {
        let config = NfsConfig::new("", "/exports/data");
        assert!(config.validate().is_err());
    }

    #[test]
    fn nfs_config_validate_relative_path() {
        let config = NfsConfig::new("nfs.example.com", "relative/path");
        assert!(config.validate().is_err());
    }

    #[test]
    fn s3_config_validate_valid() {
        let config = S3Config::new("my-bucket");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn s3_config_validate_empty_bucket() {
        let config = S3Config::new("");
        assert!(config.validate().is_err());
    }

    #[test]
    fn s3_config_validate_short_bucket() {
        let config = S3Config::new("ab");
        assert!(config.validate().is_err());
    }

    // ===================
    // Volume Tests
    // ===================

    #[test]
    fn volume_new() {
        let id = VolumeId::new("test-vol").expect("valid id");
        let vol = Volume::new(id.clone(), VolumeType::empty_dir(), 1024 * 1024 * 1024);

        assert_eq!(vol.id, id);
        assert_eq!(vol.capacity, 1024 * 1024 * 1024);
        assert_eq!(vol.status, VolumeStatus::Pending);
        assert_eq!(vol.access_mode, AccessMode::ReadWriteOnce);
    }

    #[test]
    fn volume_builder() {
        let id = VolumeId::new("test-vol").expect("valid id");
        let vol = Volume::new(id, VolumeType::empty_dir(), 1024)
            .with_access_mode(AccessMode::ReadWriteMany)
            .with_storage_class("fast")
            .with_label("app", "web")
            .with_annotation("description", "Web server data");

        assert_eq!(vol.access_mode, AccessMode::ReadWriteMany);
        assert_eq!(vol.storage_class, Some("fast".to_string()));
        assert_eq!(vol.labels.get("app"), Some(&"web".to_string()));
        assert_eq!(
            vol.annotations.get("description"),
            Some(&"Web server data".to_string())
        );
    }

    #[test]
    fn volume_status_transitions() {
        let id = VolumeId::new("test-vol").expect("valid id");
        let mut vol = Volume::new(id, VolumeType::empty_dir(), 1024);

        assert!(!vol.is_available());

        vol.set_status(VolumeStatus::Available);
        assert!(vol.is_available());

        vol.set_status(VolumeStatus::Bound);
        assert!(vol.is_bound());

        vol.set_status(VolumeStatus::Attached);
        assert!(vol.is_attached());
    }

    // ===================
    // VolumeMount Tests
    // ===================

    #[test]
    fn volume_mount_new() {
        let id = VolumeId::new("test-vol").expect("valid id");
        let mount = VolumeMount::new(id.clone(), "/data");

        assert_eq!(mount.volume_id, id);
        assert_eq!(mount.mount_path, PathBuf::from("/data"));
        assert!(!mount.read_only);
    }

    #[test]
    fn volume_mount_builder() {
        let id = VolumeId::new("test-vol").expect("valid id");
        let mount = VolumeMount::new(id, "/data")
            .read_only()
            .with_sub_path("subdir")
            .with_propagation(MountPropagation::Bidirectional);

        assert!(mount.read_only);
        assert_eq!(mount.sub_path, Some("subdir".to_string()));
        assert_eq!(mount.propagation, MountPropagation::Bidirectional);
    }

    #[test]
    fn volume_mount_validate_valid() {
        let id = VolumeId::new("test-vol").expect("valid id");
        let mount = VolumeMount::new(id, "/data");
        assert!(mount.validate().is_ok());
    }

    #[test]
    fn volume_mount_validate_empty_path() {
        let id = VolumeId::new("test-vol").expect("valid id");
        let mount = VolumeMount::new(id, "");
        assert!(mount.validate().is_err());
    }

    #[test]
    fn volume_mount_validate_relative_path() {
        let id = VolumeId::new("test-vol").expect("valid id");
        let mount = VolumeMount::new(id, "relative/path");
        assert!(mount.validate().is_err());
    }

    // ===================
    // VolumeClaim Tests
    // ===================

    #[test]
    fn volume_claim_new() {
        let claim = VolumeClaim::new("my-claim", 10 * 1024 * 1024 * 1024);

        assert_eq!(claim.id, "my-claim");
        assert_eq!(claim.requested_capacity, 10 * 1024 * 1024 * 1024);
        assert_eq!(claim.status, ClaimStatus::Pending);
        assert!(!claim.is_bound());
    }

    #[test]
    fn volume_claim_builder() {
        let claim = VolumeClaim::new("my-claim", 10 * 1024 * 1024 * 1024)
            .with_access_mode(AccessMode::ReadWriteMany)
            .with_storage_class("fast")
            .with_selector("app", "web")
            .with_owner("workload-1");

        assert_eq!(claim.access_mode, AccessMode::ReadWriteMany);
        assert_eq!(claim.storage_class, Some("fast".to_string()));
        assert_eq!(claim.selector.get("app"), Some(&"web".to_string()));
        assert_eq!(claim.owner, Some("workload-1".to_string()));
    }

    #[test]
    fn volume_claim_matches_volume() {
        let id = VolumeId::new("test-vol").expect("valid id");
        let vol = Volume::new(id, VolumeType::empty_dir(), 20 * 1024 * 1024 * 1024)
            .with_access_mode(AccessMode::ReadWriteMany)
            .with_storage_class("fast")
            .with_label("app", "web");

        let claim = VolumeClaim::new("my-claim", 10 * 1024 * 1024 * 1024)
            .with_access_mode(AccessMode::ReadWriteOnce)
            .with_storage_class("fast")
            .with_selector("app", "web");

        assert!(claim.matches_volume(&vol));
    }

    #[test]
    fn volume_claim_no_match_capacity() {
        let id = VolumeId::new("test-vol").expect("valid id");
        let vol = Volume::new(id, VolumeType::empty_dir(), 5 * 1024 * 1024 * 1024);

        let claim = VolumeClaim::new("my-claim", 10 * 1024 * 1024 * 1024);

        assert!(!claim.matches_volume(&vol));
    }

    #[test]
    fn volume_claim_no_match_access_mode() {
        let id = VolumeId::new("test-vol").expect("valid id");
        let vol = Volume::new(id, VolumeType::empty_dir(), 10 * 1024 * 1024 * 1024)
            .with_access_mode(AccessMode::ReadOnlyMany);

        let claim = VolumeClaim::new("my-claim", 5 * 1024 * 1024 * 1024)
            .with_access_mode(AccessMode::ReadWriteOnce);

        assert!(!claim.matches_volume(&vol));
    }

    #[test]
    fn volume_claim_no_match_storage_class() {
        let id = VolumeId::new("test-vol").expect("valid id");
        let vol = Volume::new(id, VolumeType::empty_dir(), 10 * 1024 * 1024 * 1024)
            .with_storage_class("slow");

        let claim = VolumeClaim::new("my-claim", 5 * 1024 * 1024 * 1024)
            .with_storage_class("fast");

        assert!(!claim.matches_volume(&vol));
    }

    #[test]
    fn volume_claim_no_match_selector() {
        let id = VolumeId::new("test-vol").expect("valid id");
        let vol = Volume::new(id, VolumeType::empty_dir(), 10 * 1024 * 1024 * 1024)
            .with_label("app", "api");

        let claim = VolumeClaim::new("my-claim", 5 * 1024 * 1024 * 1024)
            .with_selector("app", "web");

        assert!(!claim.matches_volume(&vol));
    }

    // ===================
    // StorageClass Tests
    // ===================

    #[test]
    fn storage_class_new() {
        let sc = StorageClass::new("fast", "claw.io/local-ssd");

        assert_eq!(sc.name, "fast");
        assert_eq!(sc.provisioner, "claw.io/local-ssd");
        assert_eq!(sc.reclaim_policy, ReclaimPolicy::Retain);
        assert!(!sc.allow_volume_expansion);
    }

    #[test]
    fn storage_class_builder() {
        let sc = StorageClass::new("fast", "claw.io/local-ssd")
            .with_parameter("type", "ssd")
            .with_reclaim_policy(ReclaimPolicy::Delete)
            .with_expansion()
            .with_binding_mode(VolumeBindingMode::WaitForFirstConsumer)
            .as_default();

        assert_eq!(sc.parameters.get("type"), Some(&"ssd".to_string()));
        assert_eq!(sc.reclaim_policy, ReclaimPolicy::Delete);
        assert!(sc.allow_volume_expansion);
        assert_eq!(sc.volume_binding_mode, VolumeBindingMode::WaitForFirstConsumer);
        assert!(sc.is_default);
    }

    // ===================
    // EmptyDirConfig Tests
    // ===================

    #[test]
    fn empty_dir_config_default() {
        let config = EmptyDirConfig::new();
        assert_eq!(config.medium, EmptyDirMedium::Default);
        assert_eq!(config.size_limit, 0);
    }

    #[test]
    fn empty_dir_config_memory() {
        let config = EmptyDirConfig::new().memory();
        assert_eq!(config.medium, EmptyDirMedium::Memory);
    }

    #[test]
    fn empty_dir_config_size_limit() {
        let config = EmptyDirConfig::new().with_size_limit_gb(10);
        assert_eq!(config.size_limit, 10 * 1024 * 1024 * 1024);
    }

    // ===================
    // Serde Tests
    // ===================

    #[test]
    fn volume_serde_roundtrip() {
        let id = VolumeId::new("test-vol").expect("valid id");
        let vol = Volume::new(id, VolumeType::nfs("nfs.example.com", "/data"), 1024)
            .with_access_mode(AccessMode::ReadWriteMany)
            .with_storage_class("fast");

        let json = serde_json::to_string(&vol).expect("serialize");
        let restored: Volume = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(vol.id, restored.id);
        assert_eq!(vol.access_mode, restored.access_mode);
        assert_eq!(vol.storage_class, restored.storage_class);
    }

    #[test]
    fn volume_claim_serde_roundtrip() {
        let claim = VolumeClaim::new("my-claim", 1024)
            .with_access_mode(AccessMode::ReadWriteOnce)
            .with_storage_class("fast");

        let json = serde_json::to_string(&claim).expect("serialize");
        let restored: VolumeClaim = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(claim.id, restored.id);
        assert_eq!(claim.requested_capacity, restored.requested_capacity);
        assert_eq!(claim.access_mode, restored.access_mode);
    }
}
